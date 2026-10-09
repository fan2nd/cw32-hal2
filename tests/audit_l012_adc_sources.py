#!/usr/bin/env python3
"""Pin own L012 electrical/register evidence; no hardware access or SDK macro reuse."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import subprocess
import yaml
ROOT=Path(__file__).resolve().parents[1]
SOURCES={
 'CW32L012_UserManual_CN_V1.4.pdf':'a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340',
 'CW32L012_DataSheet_CN_V1.0.pdf':'08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76',
 'cw32l012/Libraries/inc/cw32l012_adc.h':'13a189fef19ca32b64c6fa289c2e144069e8961ed4cf72bdbe62cd190266b045',
 'cw32l012/Libraries/src/cw32l012_adc.c':'8b26760851f1ed6e83ce1d8e53b30fb436b51a924e0c69b2610a5e9db84bdd50',
 'cw32l012/Libraries/inc/cw32l012_sysctrl.h':'3746c16b7a3fa72d1b43d42c080111e64faa36e8beb4b1f19c13daf3c71b993c',
}
def page(pdf,number):
 return subprocess.run(['pdftotext','-layout','-f',str(number),'-l',str(number),str(pdf),'-'],capture_output=True,text=True,check=True).stdout

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--sources',default=os.environ.get('CW32_SOURCES','/workspace/shared/cw32-sources'));args=parser.parse_args();root=Path(args.sources)
 for name,expected in SOURCES.items(): assert hashlib.sha256((root/name).read_bytes()).hexdigest()==expected,name
 rm=root/'CW32L012_UserManual_CN_V1.4.pdf';ds=root/'CW32L012_DataSheet_CN_V1.0.pdf'
 sample=page(rm,602)
 rows=re.findall(r'^\s*([01]{4})\s+(\d+)\s*$',sample,re.M)
 assert [(int(code,2),int(cycles)) for code,cycles in rows]==list(enumerate([6,7,9,12,18,24,30,42,54,70,102,134,166,198,262,518]))
 rates=page(rm,603)
 for start,end,rate,clock in [('1.7','1.8','200K','6'),('1.8','2.8','500K','12'),('2.8','3.3','1M','24'),('3.3','5.5','1M','48')]:
  assert re.search(rf'{re.escape(start)}V\s*~\s*{re.escape(end)}V\s+{rate}\s*SPS\s+{clock}MHz',rates)
 cr=page(rm,615);assert re.search(r'Reset value:\s*0x0000\s+0100',cr)
 assert re.search(r'31:8\s+RFU',cr) and re.search(r'7\s+SLAVE',cr) and re.search(r'3:2\s+CLK',cr)
 icr=page(rm,623).split('25.12.10')[1]
 assert re.search(r'Reset value:\s*0x0000\s+000F',icr)
 for bit,field in [(0,'EOC'),(1,'EOS'),(3,'AWDL'),(4,'AWDH')]: assert re.search(rf'{bit}\s+{field}\s+R1W0',icr)
 assert re.search(r'2\s+RFU',icr)
 bgr=page(rm,625)
 for token in ['ADCEN','TSEN','VC1/2/3/4','OPA1/2','POR reset']:
  assert token in bgr,token
 assert re.search(r'BGR\s*只能软件失能或\s*POR reset\s*才能失能',bgr)
 internal=page(rm,616);assert re.search(r'40\s*[μµ]s',internal)
 adc=page(ds,63)
 assert re.search(r'fADC\s+ADC 时钟频率\s+-\s+4\s+48\s+96\s+MHz',adc)
 for v1,v2,limit in [('1.7','1.8','1'),('1.8','2.8','0.5'),('2.8','3.3','0.25'),('3.3','5.5','0.125')]:
  assert re.search(rf'{re.escape(v1)}V<VDD<{re.escape(v2)}V\s+{re.escape(limit)}\s',adc)
 assert re.search(r'tCONV\s+总转换时间（含采样保持）\s+-\s+21\s+-\s+405',adc)
 ts=page(ds,66);assert re.search(r'tSTART\s+TS 内置温度传感器建立时间\s+-\s+-\s+40',ts)
 hdr=(root/'cw32l012/Libraries/inc/cw32l012_adc.h').read_text()
 assert re.search(r'ADC_SampTime518Clk\s+\(\(uint32_t\)0x0000000F\)',hdr)
 # Preserve the disagreement as an audit condition, rather than silently using these obsolete SDK masks.
 assert re.search(r'ADC_IT_AWDL\s+\(\(uint16_t\)0x0004\)',hdr)
 assert re.search(r'ADC_IT_AWDH\s+\(\(uint16_t\)0x0008\)',hdr)
 registers=yaml.safe_load((ROOT/'cw32-data/registers/adc_cw32l012_v1.yaml').read_text())
 offsets={item['name']:item['byte_offset'] for item in registers['block/ADC']['items']}
 for name,offset in [('CR',0),('START',4),('AWDCR',12),('TRIGGER',16),('SAMPLE',24),('SQRCFR',32),('RESULT',48),('IER',112),('ICR',116),('ISR',120)]: assert offsets[name]==offset
 for reg in ['ICR','ISR']:
  assert {f['name']:f['bit_offset'] for f in registers[f'fieldset/{reg}']['fields']}==dict(EOC=0,EOS=1,AWDL=3,AWDH=4)
 assert {f['name']:f['bit_offset'] for f in registers['fieldset/IER']['fields']}==dict(EOC=0,EOS=1,AWDL=3,AWDH=4,DMAEOC=5,DMAEOS=6)
 for name in ['ISR','RESULT']:
  assert next(item for item in registers['block/ADC']['items'] if item['name']==name)['access']=='Read'
 result = next(item for item in registers['block/ADC']['items'] if item['name']=='RESULT')
 assert result['array']=={'len':8,'stride':4}
 print('PASS L012 own-source electrical/register audit: five hashes,16 acquisition codes, voltage/rate/clock/acquisition limits, BGR sharing, sparse ICR/reserved bits and three recorded SDK/DS discrepancies')
if __name__=='__main__':main()
