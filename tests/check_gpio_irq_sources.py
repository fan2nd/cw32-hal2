#!/usr/bin/env python3
"""Audit own-manual GPIO ICR command semantics, independently of reset values."""
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
EVIDENCE = json.loads((ROOT / 'docs/gpio-async-evidence.json').read_text())
assert len(EVIDENCE['families']) == 13
for record in EVIDENCE['families']:
    pdf = SOURCES / record['source']
    text = pdf.with_suffix('.txt')
    assert hashlib.sha256(pdf.read_bytes()).hexdigest() == record['sha256'], pdf
    assert hashlib.sha256(text.read_bytes()).hexdigest() == record['text_sha256'], text
    document = text.read_text()
    offset = document.rfind('GPIOx_ICR')
    assert offset >= 0, record['family']
    block = document[offset:offset + 1100]
    width = record['command_bits']
    assert width == (8 if record['family'] in ('CW32F002', 'CW32F003') else 16)
    assert re.search(rf'31\s*:\s*{width}\s+RFU', block), (record['family'], 'upper reserved bits')
    assert re.search(rf'{width-1}\s*:\s*0\s+R1W0', block), (record['family'], 'command field width')
    assert re.search(r'W1\s*[：:]\s*无功能', block), (record['family'], 'W1 no-op')
    assert re.search(r'W0\s*[：:]\s*清除', block), (record['family'], 'W0 clear')
    assert int(record['no_op_mask'], 16) == (1 << width) - 1
    assert 'Reset value' in block
    print(f"PASS {record['family']}: own-manual ICR PIN0..{width-1} W1 no-op; RFU31:{width} zero; reset not used as command mask")
print('PASS GPIO command semantics: 13 families, 12 distinct own/shared manuals; source evidence, not silicon execution')
