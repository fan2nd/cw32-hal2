#!/usr/bin/env python3
"""Compile-time metadata proof: F020 FAULT31 belongs to SYSCTRL, never CRC."""
import json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
SOURCE='''#![no_std]
use cw32_metapac::metadata::METADATA;
const fn equal(a:&str,b:&str)->bool {let a=a.as_bytes();let b=b.as_bytes();if a.len()!=b.len(){return false;}let mut i=0;while i<a.len(){if a[i]!=b[i]{return false;}i+=1;}true}
const fn owns(name:&str)->bool {let mut p=0;while p<METADATA.peripherals.len(){let v=&METADATA.peripherals[p];if equal(v.name,name){let mut i=0;while i<v.interrupts.len(){if equal(v.interrupts[i].interrupt,"FAULT"){return true;}i+=1;}}p+=1;}false}
const fn number()->u16 {let mut i=0;while i<METADATA.interrupts.len(){if equal(METADATA.interrupts[i].name,"FAULT"){return METADATA.interrupts[i].number as u16;}i+=1;}0xffff}
const _: () = assert!(number()==31);
const _: () = assert!(owns("SYSCTRL"));
const _: () = assert!(!owns("CRC"));
'''
def main():
 env=os.environ.copy();env['CARGO_INCREMENTAL']='0'
 if (ROOT/'.cargo/bin/cargo').exists():
  env.update(CARGO_HOME=str(ROOT/'.cargo'),RUSTUP_HOME=str(ROOT/'.rustup'),PATH=str(ROOT/'.cargo/bin')+os.pathsep+env.get('PATH',''))
 with tempfile.TemporaryDirectory(prefix='cw32-fault-owner-') as temp:
  d=Path(temp);(d/'src').mkdir()
  for chip in ['cw32f020','cw32f020f6u7','cw32f020k6u7','cw32f020c6u7']:
   (d/'Cargo.toml').write_text('[package]\nname="cw32-fault-owner"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncw32-metapac={path='+json.dumps(str(ROOT/'cw32-metapac'))+',default-features=false,features=["metadata",'+json.dumps(chip)+']}\n')
   def check(source):
    (d/'src/lib.rs').write_text(source)
    return subprocess.run(['cargo','check','--offline','--manifest-path',str(d/'Cargo.toml'),'--target','thumbv6m-none-eabi','--target-dir',str(ROOT/'target/fault-owner-contracts'),'--message-format=json'],env=env,text=True,capture_output=True)
   result=check(SOURCE);assert result.returncode==0,result.stdout+result.stderr
   result=check(SOURCE+'\nconst _: () = assert!(owns("CRC"));\n')
   messages=[json.loads(x).get('message',{}) for x in result.stdout.splitlines() if json.loads(x).get('reason')=='compiler-message']
   assert result.returncode!=0 and any(m.get('code',{}).get('code')=='E0080' for m in messages if m.get('code')),result.stdout+result.stderr
   print(f'PASS {chip}: SYSCTRL owns FAULT31; CRC ownership rejected at compile time')
 print('4 positive and4 compiler-negative ownership checks passed; no hardware execution')
if __name__=='__main__':main()
