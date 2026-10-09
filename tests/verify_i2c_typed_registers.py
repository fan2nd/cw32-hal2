#!/usr/bin/env python3
"""Audit own-manual I2C field/enumeration provenance and PAC command seeds.

This checks source documents and generated data/PAC only. It contains no HAL
register model, fake MMIO engine, compile fixture, or firmware execution.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', str(ROOT.parent / 'cw32-sources')))
EVIDENCE = json.loads((ROOT / 'docs/i2c-typed-register-evidence.json').read_text())


def digest(data):
    return hashlib.sha256(data).hexdigest()


def pages(fact, name):
    path = SOURCES / fact['manual']['filename']
    result = []
    for page in fact[name]:
        number = str(page['pdf_page_1_based'])
        data = subprocess.check_output(['pdftotext', '-f', number, '-l', number,
                                        '-layout', str(path), '-'])
        assert digest(data) == page['extract_sha256'], (path.name, number)
        result.append(data.decode())
    return '\n'.join(result)


def field(ir, register, name):
    return next(f for f in ir[f'fieldset/{register}']['fields'] if f['name'] == name)


def flat(text):
    return re.sub(r'\s+', '', text)


def main():
    assert len(EVIDENCE['families']) == 13 and len(EVIDENCE['registers']) == 6
    classic_fields = {'FLT': 0, 'AA': 2, 'SI': 3, 'STO': 4, 'STA': 5, 'EN': 6}
    # Distinct own-manual status rows, including slave transitions after arbitration.
    status_meaning = {
        0x00: '总线错误', 0x08: '已发送起始信号', 0x10: '已发送重复起始信号',
        0x18: '已发送SLA+W，已接收ACK', 0x20: '已发送SLA+W，已接收NACK',
        0x28: '已接收ACK', 0x30: '已接收NACK', 0x38: '丢失仲裁',
        0x40: '已发送SLA+R，已接收ACK', 0x48: '已发送SLA+R，已接收NACK',
        0x50: '已接收数据字节，ACK已返回', 0x58: '已接收数据字节，NACK已返回',
        0x60: '已接收自身的SLA+W', 0x68: '丢失仲裁',
        0x70: '已接收广播地址', 0x78: '丢失仲裁',
        0x80: '前一次寻址使用自身从地址，已接收数据字节，已返回ACK',
        0x88: '前一次寻址使用自身从地址，已接收数据字节，已返回NACK',
        0x90: '前一次寻址使用广播地址，已接收数据字节，已返回ACK',
        0x98: '前一次寻址使用广播地址，已接收数据字节，已返回NACK',
        0xA0: '接收到停止条件或重复起始条件', 0xA8: '已接收自身的SLA+R',
        0xB0: '丢失仲裁', 0xB8: '已发送数据字节，已接收ACK',
        0xC0: '已发送数据字节，已接收NACK', 0xC8: '从机最后一个数据字节已被发送',
        0xF8: '无可用的相关状态信息',
    }
    for family, fact in EVIDENCE['families'].items():
        for source in ['manual', 'sdk_header']:
            assert digest((SOURCES / fact[source]['filename']).read_bytes()) == fact[source]['sha256'], (family, source)
        ir = json.loads((ROOT / f'cw32-data/data/registers/{fact["register_version"]}.json').read_text())
        control = pages(fact, 'control_pages')
        status = pages(fact, 'status_pages')
        if family != 'CW32L012':
            for value, meaning in status_meaning.items():
                rows = [flat(line) for line in status.splitlines() if re.search(rf'\b{value:02X}H\b', line)]
                assert any(meaning in row for row in rows), (family, hex(value), meaning)
            assert set(EVIDENCE['registers'][fact['register_version']]['variants'].values()) == set(status_meaning)
            assert 'Reset value: 0x0000 0000' in control
            for name, bit in classic_fields.items():
                f = field(ir, 'CR', name)
                assert f['bit_offset'] == bit and f['bit_size'] == 1, (family, name)
                assert re.search(rf'\b{bit}\s+{name}\s+', control), (family, name)
            assert '在SI被清除后' in flat(control), family
            for name, offset in [('SCLINSRC', 8), ('SDAINSRC', 11)]:
                if fact['gpio_input_selectors']:
                    f = field(ir, 'CR', name)
                    assert f['bit_offset'] == offset and f['bit_size'] == 3
                    assert f'000：I2C_{"SCL" if offset == 8 else "SDA"}引脚' in flat(control)
                else:
                    assert name not in control
            assert field(ir, 'STAT', 'STAT')['enum'] == 'Status'
            print(f'PASS {family}: own STAT table, CR fields, SI release and GPIO-source differences')
        else:
            command = flat(pages(fact, 'command_pages'))
            for encoding, text in [('000', '向总线发送一个字节数据'), ('001', '从总线接收'),
                                   ('010', '在总线上产生STOP信号'), ('011', '从总线接收'),
                                   ('100', '在总线上生成（Repeated）START信号'),
                                   ('101', '在总线上生成（Repeated）START信号')]:
                assert encoding + '：' + text in command, encoding
            assert '不存入MRDR寄存器' in command and '期待从总线收到NACK' in command
            assert field(ir, 'MTDR', 'CMD')['enum'] == 'Command'
            micr = pages(fact, 'micr_pages')
            assert 'Reset value: 0x0000 7F04' in micr
            assert re.search(r'7:0\s+RFU\s+-\s+保留位，请保持默认值', micr)
            assert micr.count('W1：无功能') == 7
            assert '先通过MCR0.TXFIFORST复位TXFIFO后' in flat(micr)
            for bit, name in enumerate(fact['clearable_fields'], start=8):
                assert re.search(rf'\b{bit}\s+{name}\s+R1W0', micr), name
                assert field(ir, 'MICR', name)['bit_offset'] == bit
            for name, bit in [('MEN', 0), ('RESET', 1), ('TXFIFORST', 8), ('RXFIFORST', 9)]:
                assert re.search(rf'\b{bit}\s+{name}\s+', control), name
                assert field(ir, 'MCR0', name)['bit_offset'] == bit
            assert '00：PCLK' in flat(control)
            for name, bit in [('TXE', 0), ('RXNE', 1), ('MSTBUSY', 24), ('BUSBUSY', 25)]:
                assert re.search(rf'\b{bit}\s+{name}\s+RO', status), name
                assert field(ir, 'MISR', name)['bit_offset'] == bit
            receive = pages(fact, 'receive_pages')
            assert re.search(r'14\s+EMPTY\s+RO', receive)
            assert field(ir, 'MRDR', 'EMPTY')['bit_offset'] == 14
            print('PASS CW32L012: own command, control/status, atomic receive and MICR R1W0 semantics')
    for version, fact in EVIDENCE['registers'].items():
        ir = json.loads((ROOT / f'cw32-data/data/registers/{version}.json').read_text())
        variants = ir['enum/' + fact['enum']]['variants']
        assert {v['name']: v['value'] for v in variants} == fact['variants'], version
        pac = (ROOT / f'cw32-metapac/src/peripherals/{version}.rs').read_text()
        assert re.search(rf'pub (?:enum|struct) {fact["enum"]}\b', pac), version
        for name in fact['variants']:
            rust = ''.join(word.title() for word in name.split('_'))
            assert re.search(rf'\b{rust}(?:\s*:\s*Self)?\s*=', pac), (version, rust)
    authored = yaml.safe_load((ROOT / 'cw32-data/register-writes.yaml').read_text())
    micr = authored['registers']['i2c_cw32l012_v1'][0]
    assert micr['register'] == micr['fieldset'] == 'MICR' and micr['block'] == 'I2C'
    assert micr['reset_value'] == micr['write_noop'] == 0x7f04
    assert micr['zero_to_clear_fields'] == EVIDENCE['families']['CW32L012']['clearable_fields']
    projected = json.loads((ROOT / 'cw32-data/data/register-writes/i2c_cw32l012_v1.json').read_text())
    assert projected['registers']['i2c_cw32l012_v1'][0] == micr
    pac = (ROOT / 'cw32-metapac/src/peripherals/i2c_cw32l012_v1.rs').read_text()
    code = pac.split('impl regs::Micr {', 1)[1]
    for method in ['reset_value', 'write_noop']:
        assert re.search(rf'pub const fn {method}\(\) -> Self\s*\{{\s*Self\(32516\)', code), method
    assert re.search(r'impl Default for Micr.*?(?:Self|Micr)\(0\)', pac, re.S)
    print('PASS all 6 typed PAC variants, 13 source families, explicit MICR seed and unchanged zero Default')


if __name__ == '__main__':
    main()
