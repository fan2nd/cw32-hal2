#!/usr/bin/env python3
"""ARM compile-only PAC access checks. No HAL, test harness, MMIO or execution."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]

def main():
 ap=argparse.ArgumentParser();ap.add_argument('--target-dir',type=Path,required=True);ap.add_argument('--receipts',type=Path,required=True);args=ap.parse_args()
 env=os.environ.copy();env.update(CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0')
 representatives=[('cw32f030c8t7',True),('cw32l010f8p6',False),('cw32l011k8t6',False),('cw32l031c8t6',True)]
 receipts=[]
 with tempfile.TemporaryDirectory(prefix='cw32-ram-pac-') as tmp:
  tmp=Path(tmp);(tmp/'src').mkdir()
  for part,has_en in representatives:
   (tmp/'Cargo.toml').write_text('[package]\nname="ram-pac-access"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n'+f'cw32-metapac={{path={json.dumps(str(ROOT/"cw32-metapac"))},default-features=false,features=["pac",{json.dumps(part)}]}}\n')
   def compile_case(name,body,forbidden=None):
    (tmp/'src/lib.rs').write_text('#![no_std]\nuse cw32_metapac as pac;\npub fn accesses() {\n'+body+'\n}\n')
    command=['cargo','check','--offline','--target','thumbv6m-none-eabi','--message-format=json','--manifest-path',str(tmp/'Cargo.toml'),'--target-dir',str(args.target_dir)]
    r=subprocess.run(command,env=env,text=True,capture_output=True)
    messages=[json.loads(l)['message'] for l in r.stdout.splitlines() if l.startswith('{') and json.loads(l).get('reason')=='compiler-message']
    errors=[m for m in messages if m['level']=='error']
    if forbidden:
     assert r.returncode!=0 and errors and all(m.get('code',{}).get('code')=='E0599' and '`'+forbidden+'`' in m['message'] for m in errors), r.stdout+r.stderr
    else:assert r.returncode==0,r.stdout+r.stderr
    receipts.append({'part':part,'case':name,'expected':'E0599 '+forbidden if forbidden else 'success','exit_code':r.returncode,'command':command,'compiler_errors':errors,'source':body})
    print('PASS',part,name,flush=True)
   positive='pac::RAM.ier().modify(|v| v.set_parity(false));\nlet _ = pac::RAM.ier().read().parity();\nlet _ = pac::RAM.addr().read().addr();\nlet _ = pac::RAM.isr().read().parity();\npac::RAM.icr().write(|v| { *v = pac::ram::regs::Icr::write_noop(); v.set_parity(false); });'
   if has_en:positive+='\nlet _ = pac::RAM.ier().read().en();'
   compile_case('positive_fields',positive)
   for reg in ['addr','isr']:
    for method in ['write','modify']:compile_case(reg+'_'+method,f'pac::RAM.{reg}().{method}(|_| {{}});',method)
   compile_case('no_parity_enable_setter','pac::RAM.ier().modify(|v| v.set_en(false));','set_en')
   if not has_en:compile_case('no_unimplemented_enable_status','let _ = pac::RAM.ier().read().en();','en')
 args.receipts.write_text(json.dumps({'target':'thumbv6m-none-eabi','scope':'PAC expressions compiled only; no HAL harness or execution','cases':receipts},indent=2)+'\n')
 print(f'{len(receipts)} expected PAC compilation outcomes verified')
if __name__=='__main__':main()
