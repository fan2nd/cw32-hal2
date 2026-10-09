#!/usr/bin/env python3
"""RAM own-source/PAC/data contract audit. Never executes MMIO or HAL firmware."""
from pathlib import Path
import hashlib,json,os,re,zipfile
import yaml
ROOT=Path(__file__).resolve().parents[1]
SOURCES=Path(os.environ.get('CW32_SOURCES','/workspace/shared/cw32-sources'))
def load(p):return yaml.safe_load(p.read_text()) if p.suffix == '.yaml' else json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def compact(s):return re.sub(r'\s+','',s)
def main():
 e=load(ROOT/'docs/ram-parity-evidence.json');c=load(ROOT/'cw32-data/ram-parity.yaml');lock=load(ROOT/'sources/evidence-sources.json')
 assert e['common']==c['common'] and len(c['profiles'])==13
 assert e['common']=={'always_enabled':True,'software_parity_enable':False,'software_parity_disable':False,'initialization_command':False,'injection_command':False,'reset_command':False,'independent_clock_gate':False,'register_offsets':{'IER':0,'ADDR':4,'ISR':8,'ICR':12},'status_bit':0,'clear_bit':0,'clear_value':0,'clear_noop':1,'address_reset':0x20000000,'status_reset':0,'parity_bits_per_data_byte':1}
 restrictions=load(ROOT/'cw32-data/field-access.yaml')
 for family,row in e['families'].items():
  p=c['profiles'][family];assert p==row['profile']
  for source in row['sources'].values():
   assert source['url'].startswith('https://www.whxy.com/') and sha(SOURCES/source['path'])==source['sha256']
   original=next(a for a in lock['artifacts'] if a['id']==source['source_ref']);assert original['sha256']==source['sha256']
  manual=row['sources']['manual'];pdf=next(a for a in lock['artifacts'] if a['id']==manual['source_ref']);text=SOURCES/pdf['text']['path'];assert sha(text)==pdf['text']['sha256']
  pages=text.read_text().split('\f');section=compact(''.join(pages[i-1] for i in row['manual_chapter_pdf_pages']))
  assert '用户不可配置' in section and '上电后奇偶校验功能默认打开' in section
  assert '31:0ADDRRO' in section and 'Resetvalue:0x20000000' in section
  assert 'PARITYR1W0W0：清除奇偶校验错误标志' in section and 'W1：无功能' in section
  assert f"{p['ier_parity_bit']}PARITYRW" in section and '0PARITYRO' in section
  assert ('0ENRO' in section)==p['enable_status']
  assert '9bit' in section and '8bit' in section and '1bit' in section
  assert all(f'RAM_BASE+0x{offset:02X}' in section for offset in [0,4,8,12])
  assert 'HardFault' in section
  assert 'RAM_BASE=0x40022400' in section
  assert p['interrupt_number']==3 and p['base_address']==0x40022400 and p['address_bits']==32
  # A source text is pinned by its parent PDF's own provenance entry.
  ds=next(a for a in lock['artifacts'] if a['id']==row['sources']['datasheet']['source_ref']);dtext=SOURCES/ds['text']['path'];assert sha(dtext)==ds['text']['sha256']
  dp=dtext.read_text().split('\f');assert all('奇偶校验' in dp[i-1] for i in row['datasheet_parity_pdf_pages'])
  sdk=row['sources']['sdk']
  with zipfile.ZipFile(SOURCES/sdk['path']) as archive:
   for m in row['sdk_members']:
    assert len(m['members'])==1 and archive.read(m['members'][0])==(SOURCES/m['path']).read_bytes()
    assert sha(SOURCES/m['path'])==m['sha256']
  header=(SOURCES/row['sdk_members'][0]['path']).read_text(errors='replace');driver=(SOURCES/row['sdk_members'][2]['path']).read_text(errors='replace');rh=(SOURCES/row['sdk_members'][1]['path']).read_text(errors='replace')
  assert re.search(r'\b'+p['interrupt']+r'_IRQn\s*=\s*3\b',header)
  assert re.search(r'RAM_IER_PARITY_Pos\s*\('+str(p['ier_parity_bit'])+r'UL\)',header)
  assert re.search(r'RAM_ISR_PARITY_Pos\s*\(0UL\)',header) and re.search(r'RAM_ICR_PARITY_Pos\s*\(0UL\)',header)
  assert not re.search(r'SYSCTRL_\w*(?:EN|RST)\w*_RAM_(?:Msk|Pos)',header)
  assert 'CW_RAM->ADDR' in driver and 'CW_RAM->ICR' in driver
  if p['enable_status'] and family not in ['CW32F002','CW32F003']:
   assert re.search(r'#define\s+RAM_IT_PARITY\s+\(bv1\)',rh)
   assert 'CW_RAM->ISR & RAM_IT' in driver and 'CW_RAM->ICR &= (~RAM_IT)' in driver
   assert any('SDK RAM_IT_PARITY' in s for s in row['conflicts'])
  ir=load(ROOT/f"cw32-data/data/registers/ram_{p['register_version']}.json")
  assert {i['name']:i['byte_offset'] for i in ir['block/RAM']['items']}==c['common']['register_offsets']
  assert next(f for f in ir['fieldset/ADDR']['fields'] if f['name']=='ADDR')['bit_size']==32
  for reg in ['ADDR','ISR']:assert next(i for i in ir['block/RAM']['items'] if i['name']==reg)['access']=='Read'
  ier={f['name']:f for f in ir['fieldset/IER']['fields']};assert ier['PARITY']['bit_offset']==p['ier_parity_bit'] and ('EN' in ier)==p['enable_status']
  key='ram_'+p['register_version'];pac=(ROOT/f'cw32-metapac/src/peripherals/{key}.rs').read_text()
  assert 'fn set_en(' not in pac and ('fn en(' in pac)==p['enable_status']
  assert 'fn set_parity(' in pac and 'fn write_noop(' in pac
  if p['enable_status']:
   projection=load(ROOT/f'cw32-data/data/field-access/{key}.json');assert projection=={'schema_version':1,'registers':{key:restrictions['registers'][key]}}
   field=projection['registers'][key][0];assert (field['register'],field['field'],field['bit_offset'],field['bit_size'])==('IER','EN',0,1)
  print(f'PASS {family}: own manual/datasheet/SDK bytes, full address, fixed checking, W0C, shared IRQ, no RAM gate')
 count=0
 for cp in (ROOT/'cw32-data/data/chips').glob('*.json'):
  chip=load(cp);p=c['profiles'][chip['line']];core=chip['cores'][0];ram=next(x for x in core['peripherals'] if x['name']=='RAM');flash=next(x for x in core['peripherals'] if x['name']=='FLASH')
  assert ram['registers']['version']==p['register_version'] and ram['address']==p['base_address']
  assert 'rcc' not in ram and 'rcc_control' not in ram
  assert ram['interrupts']==[{'interrupt':p['interrupt'],'signal':'GLOBAL'}]
  assert any(i['interrupt']==p['interrupt'] for i in flash['interrupts'])
  assert next(i for i in core['interrupts'] if i['name']==p['interrupt'])['number']==3
  count+=1
 print(f'PASS {count} chip selections: source-only IRQ/register projection; no HAL execution')
if __name__=='__main__':main()
