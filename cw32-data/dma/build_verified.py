#!/usr/bin/env python3
"""Rebuild reviewed DMA facts from pinned official manuals and SDK headers.

This is a fact-extraction audit, not the chip importer. Never infer requests from
another family or a CMSIS instance alias. The reviewed manual page selections
and normalization rules below are deliberate inputs. Requires pdftotext -layout
text companions in --source-root; downloaded documents stay outside the repo.
"""
import argparse
import hashlib
import json
import yaml
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
from source_provenance import enrich_source_refs
# Manual stem, download date, count, count PDF index, request PDF indices,
# interrupt-table PDF indices, printed-page offset from zero-based PDF index.
CONFIG = {
    'CW32F030': ('CW32x030_UserManual_EN_V1.0', '20240920', 5, 130, [147, 148], [97, 98], 0),
    'CW32A030': ('CW32x030_UserManual_EN_V1.0', '20240920', 5, 130, [147, 148], [97, 98], 0),
    'CW32F020': ('CW32F020_UserManual_CN_V1.4', '20240920', 2, 122, [137], [93], 0),
    'CW32L012': ('CW32L012_UserManual_CN_V1.4', '20260603', 4, 132, [146], [95], -25),
    'CW32L031': ('CW32L031_UserManual_CN_V1.6', '20240920', 4, 119, [133], [90], 0),
    'CW32L052': ('CW32L052_UserManual_CN_V1.5', '20240920', 4, 124, [138], [95], 0),
    'CW32L083': ('CW32L083_UserManual_CN_V2.0', '20240920', 5, 134, [148, 149], [104], 0),
    'CW32R031': ('CW32R031_UserManual_CN_V1.3', '20240920', 4, 121, [135], [92], 0),
    'CW32W031': ('CW32W031_UserManual_CN_V1.4', '20240920', 4, 120, [134], [91], 0),
}
NO_DMA = {'CW32F002': [25], 'CW32F003': [26], 'CW32L010': [26], 'CW32L011': [33]}
EXPECTED_REQUESTS = {
    'CW32F030': set(range(43)), 'CW32A030': set(range(43)),
    'CW32F020': set(range(43)) - {17, 18}, 'CW32L012': set(range(64)),
    'CW32L031': set(range(8)) | set(range(10, 31)) | {50},
    'CW32R031': set(range(8)) | set(range(10, 31)) | {50},
    'CW32W031': set(range(8)) | set(range(10, 31)) | {50},
    'CW32L052': set(range(37)) | {49, 50}, 'CW32L083': set(range(51)),
}


def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix in {".yaml", ".yml"} else json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source(path, root, **extra):
    return {'path': str(path.relative_to(root)), 'sha256': digest(path), **extra}


def manual_rows(pages, indices):
    rows = {}
    for i in indices:
        for line in pages[i].splitlines():
            matches = list(re.finditer(r'\b([01]{6})[:：]\s*', line))
            for m, nxt in zip(matches, matches[1:] + [None]):
                number = int(m[1], 2)
                assert number not in rows, ('Duplicate request code', number)
                rows[number] = {'binary': m[1], 'description': line[m.end():nxt.start() if nxt else None].strip(),
                                'pdf_page_index': i}
    return rows


def sdk_rows(path, l012=False):
    lines = path.read_text().splitlines()
    out = {}
    for i, line in enumerate(lines, 1):
        if l012:
            m = re.match(r'\s*(DMA_TRIGGER_SRC_\w+)(?:\s*=\s*(\d+))?\s*[,\s]', line)
            if m:
                value = int(m[2]) if m[2] else len(out)
                out[value] = {'symbol': m[1], 'line': i, 'raw_value': value, 'encoding': 'unshifted_enum'}
        else:
            m = re.match(r'#define\s+(DMA_HardTrig_\w+)\s+\(uint32_t\)\((0x[0-9A-Fa-f]+)UL\s*<<\s*2\)', line)
            if m:
                value = int(m[2], 16)
                assert value not in out
                out[value] = {'symbol': m[1], 'line': i, 'raw_value': value << 2, 'encoding': 'shifted_left_2_define'}
    return out


def normalize(symbol, family, description):
    if symbol.startswith('DMA_HardTrig_'):
        name = symbol[len('DMA_HardTrig_'):]
        peripheral, event = name.split('_', 1)
        signals = {'RXBufferNE': 'RX', 'TXBufferE': 'TX', 'OVERINT': 'UP', 'TRIGINT': 'TRIG',
                   'CH1A2A3A4': 'CH1A2A3A4_UP', 'CH1B2B3B': 'CH1B2B3B_UP',
                   'SINGLETRANSCOM': 'SINGLE', 'FRAMEINT': 'FRAME'}
        if event == 'TRANSCOMPLETE':
            signal = 'COMPLETE' if family in ('CW32F030', 'CW32A030', 'CW32F020') else 'SEQ'
        elif re.fullmatch(r'CH[1-4]COMINT', event):
            signal = event[:3]
        else:
            signal = signals[event]
    else:
        name = symbol[len('DMA_TRIGGER_SRC_'):]
        if name == 'HALLTIM':
            peripheral, signal = 'HALLTIM', 'EVENT'
        else:
            peripheral, event = name.split('_', 1)
            if peripheral in ('DAC1', 'DAC2'):
                signal = f'CH{peripheral[-1]}_DHR_UNDERRUN'
                peripheral = 'DAC'
            else:
                peripheral = peripheral.replace('LPI2C', 'I2C')
                signal = {'UPD': 'UP'}.get(event, event)
                if re.fullmatch(r'CC[1-6]', signal):
                    signal = 'CH' + signal[2:]
    # Reject an unrelated manual row even if SDK numbering accidentally matches.
    expected = ('DAC' + signal[2]) if peripheral == 'DAC' else peripheral
    aliases = [expected]
    if peripheral == 'SPI1' and family in ('CW32L031', 'CW32R031', 'CW32W031'):
        aliases.append('SPI')
    assert any(re.search(re.escape(alias) + r'(?![0-9])', description, re.I) for alias in aliases), (symbol, description)
    return peripheral, signal


def cmsis_source(root, sdk_family, sdk_url):
    choices = [p for p in (root / sdk_family.lower()).rglob(sdk_family.lower() + '.h') if p.parent.name == 'inc' and p.parent.parent.name == 'Libraries']
    assert len(choices) == 1, choices
    path = choices[0]
    return path, source(path, root, archive_url=sdk_url)


def build_profile(family, root, sdk_urls, datasheets):
    generated = load(ROOT / 'cw32-data/data/chips' / (family + '.json'))['cores'][0]
    peripherals = {p['name']: p for p in generated['peripherals']}
    sdk_family = 'CW32F030' if family == 'CW32A030' else family
    cmsis, cmsis_ref = cmsis_source(root, sdk_family, sdk_urls[sdk_family])
    result = {'schema_version': 1, 'family': family, 'presence': 'present',
              'coverage': {'channels': 'complete', 'interrupts': 'complete', 'requests': 'complete',
                           'meaning': 'Complete for the cited official manual request table; no silicon validation.'},
              'hardware_validated': False, 'sources': {'cmsis_header': cmsis_ref},
              'evidence': {}, 'core_dma_channels': [], 'channel_details': [],
              'peripheral_dma_channels': {}, 'requests': [], 'excluded_sdk_requests': [], 'unknowns': []}
    if family in NO_DMA:
        result['presence'] = 'absent'
        info = datasheets[family]
        pdf = root / Path(info['path']).name
        pages = pdf.with_suffix('.txt').read_text().split('\f')
        for index in NO_DMA[family]:
            assert 'DMA' not in pages[index] and '0x4002' in pages[index]
        assert 'DMA' not in peripherals and not any(p.startswith('DMACHANNEL') for p in peripherals)
        assert not re.search(r'\b(?:CW_DMA|DMA_BASE|DMACH\d+_IRQn)\b', cmsis.read_text())
        result['sources']['datasheet'] = source(pdf, root, url=info['url'])
        result['evidence']['absence'] = {'source': 'datasheet', 'section': '6, Table 6-1: complete memory/peripheral map',
                                        'pdf_page_indices': NO_DMA[family],
                                        'interpretation': 'No DMA controller in the complete peripheral map, CMSIS header, or independently imported SVD inventory.'}
        result['coverage']['meaning'] = 'No DMA controller in the cited family datasheet, CMSIS header, and SVD inventory; empty routing is intentional.'
        return result
    stem, date, count, count_page, request_pages, irq_pages, page_offset = CONFIG[family]
    pdf = root / (stem + '.pdf')
    pages = pdf.with_suffix('.txt').read_text().split('\f')
    manual = source(pdf, root, url=f'https://www.whxy.com/uploads/files/{date}/{stem}.pdf',
                    text_sha256=digest(pdf.with_suffix('.txt')))
    result['sources']['manual'] = manual
    dh = cmsis.with_name(sdk_family.lower() + '_dma.h')
    result['sources']['dma_header'] = source(dh, root, archive_url=sdk_urls[sdk_family])
    rows = manual_rows(pages, request_pages)
    assert set(rows) == EXPECTED_REQUESTS[family], (family, set(rows), EXPECTED_REQUESTS[family])
    assert re.search(fr'{count}\s*(?:条独立\s*DMA\s*通道|independent DMA channels)', pages[count_page])
    if family == 'CW32L083':
        # The HARDSRC table continues from the immediately preceding register heading.
        heading_page = 147
    else:
        heading_page = request_pages[0]
    assert re.search(fr'y\s*=\s*1[~～]\s*{count}', pages[heading_page]), (family, 'missing common channel range')
    result['evidence'] = {
        'channel_count': {'source': 'manual', 'section': '8.2', 'pdf_page_indices': [count_page],
                          'printed_pages': [count_page + page_offset]},
        'request_table': {'source': 'manual', 'section': '8.8.4, DMA_TRIGy.HARDSRC[7:2]',
                          'pdf_page_indices': request_pages, 'printed_pages': [p + page_offset for p in request_pages]},
        'channel_constraint': {'source': 'manual', 'section': '8.8.4', 'pdf_page_indices': [heading_page],
                               'printed_pages': [heading_page + page_offset],
                               'interpretation': f'One common HARDSRC table applies to every channel y=1~{count}; no channel-specific restriction is documented.'},
        'interrupts': {'source': 'manual', 'section': '5.4, Table 5-1', 'pdf_page_indices': irq_pages,
                       'printed_pages': [p + page_offset for p in irq_pages]},
    }
    if family == 'CW32L083':
        # Reviewed software-copy evidence from this family's pinned manual.
        result['evidence'].update({'software_block_copy': {'source': 'manual',
                                 'section': '8.4.1-8.4.2, 8.5',
                                 'pdf_page_indices': [136, 137, 138, 142],
                                 'printed_pages': [136, 137, 138, 142],
                                 'interpretation': 'Software BLOCK uses matching 8/16/32-bit widths, '
                                                   'REPEAT=1, count 1..65535, EN then SOFTSRC.'},
         'successful_copy_completion': {'source': 'manual',
                                        'section': '8.6, 8.8.3, 8.8.4',
                                        'pdf_page_indices': [143, 147, 149],
                                        'printed_pages': [143, 147, 149],
                                        'interpretation': 'TC means all data transferred correctly; '
                                                          'release owned resources only for TC without TE, '
                                                          'STATUS=5, and SOFTSRC=0. EN-clear or TE is not '
                                                          'documented as bus drain.'},
         'software_copy_sram': {'source': 'manual',
                                'section': '2.3, 6.1-6.3',
                                'pdf_page_indices': [32, 114, 115],
                                'printed_pages': [32, 114, 115],
                                'interpretation': 'DMA can access SRAM in aligned 8/16/32-bit widths; '
                                                  'exact capacity comes from parts.yaml and each selected '
                                                  'generated memory map.'},
         'controller_shared_gate': {'source': 'manual',
                                    'section': '4.7.12, 4.7.15',
                                    'pdf_page_indices': [85, 89],
                                    'printed_pages': [85, 89],
                                    'interpretation': 'AHBEN.DMA is the shared enable and AHBRST.DMA the '
                                                      'shared active-low reset; use central RCC controls '
                                                      'without channel-local reset or gate disable.'}})
        # Own-family peripheral DMA qualification; selectors remain extracted below.
        result['evidence'].update({'staged_hardware_block': {'source': 'manual',
                                   'section': '8.4.4, 8.5, 8.6, 8.8.3-8.8.4',
                                   'pdf_page_indices': [140, 142, 143, 147, 148, 149],
                                   'printed_pages': [140, 142, 143, 147, 148, 149],
                                   'interpretation': 'UART1-6 and SPI1-2 RX/TX are qualified for static '
                                                     'staged byte BLOCK requests on all five channels. TC '
                                                     'without TE plus STATUS=5 permits retirement after '
                                                     'peripheral request closure; SOFTSRC is not hardware '
                                                     'completion evidence. Other L083 routes are outside '
                                                     'this HAL subset.'},
         'staged_uart': {'source': 'manual',
                         'section': '5.4, 19.3.3.4, 19.5-19.7.1.6, 19.9.9',
                         'pdf_page_indices': [105, 378, 383, 384, 389, 390, 400],
                         'printed_pages': [105, 378, 383, 384, 389, 390, 400],
                         'interpretation': 'Byte TX/RX enables the channel before DMATX/DMARX and closes '
                                           'the request after TC. TX additionally waits TXBUSY for wire '
                                           'idle. UART1/4, 2/5 and 3/6 share vectors. FE/PE and overwrite '
                                           'without overrun reporting require finite-chunk loss caveats '
                                           'and retained unresolved leases.'},
         'staged_spi': {'source': 'manual',
                        'section': '20.3.9-20.6.1.3, 20.8.5-20.8.7',
                        'pdf_page_indices': [415, 416, 417, 420, 421, 431, 432],
                        'printed_pages': [415, 416, 417, 420, 421, 431, 432],
                        'interpretation': 'Master byte DMA uses paired TX/RX descriptors and both clean '
                                          'terminals, closes both requests, then waits BUSY=0 before CS '
                                          'release. MODF/OV/SSERR/UD do not prove DMA drain; errors retain '
                                          'the pair. Existing 12MHz/divisor-4 minimum SPI clock policy '
                                          'remains authoritative.'}})
    if family == 'CW32A030':
        result['evidence']['shared_manual'] = {'source': 'manual', 'pdf_page_indices': [130, 147],
                                              'interpretation': 'The manual explicitly covers CW32F030/CW32A030. F030 SDK symbol corroboration is shared-source evidence, not an independent A030 SDK.'}
    cmsis_text = cmsis.read_text()
    irq_matches = {m[1]: (int(m[2]), cmsis_text[:m.start()].count('\n') + 1) for m in re.finditer(r'\b(DMACH\d+)_IRQn\s*=\s*(\d+)', cmsis_text)}
    for physical in range(1, count + 1):
        if family == 'CW32L012':
            irq = 'DMACH12' if physical <= 2 else 'DMACH34'
        elif physical == 1:
            irq = 'DMACH1'
        elif count == 2:
            irq = 'DMACH2'
        elif physical <= 3:
            irq = 'DMACH23'
        else:
            irq = 'DMACH4' if count == 4 else 'DMACH45'
        number, line = irq_matches[irq]
        reg = f'DMACHANNEL{physical}'
        assert any(i['name'] == irq and i['number'] == number for i in generated['interrupts'])
        assert {'signal': 'GLOBAL', 'interrupt': irq} in peripherals[reg]['interrupts']
        core = {'name': f'DMA_CH{physical}', 'dma': 'DMA', 'channel': physical - 1}
        result['core_dma_channels'].append(core)
        result['channel_details'].append({'name': core['name'], 'physical_channel': physical,
                                         'register_peripheral': reg, 'interrupt': irq, 'interrupt_number': number,
                                         'cmsis_symbol': irq + '_IRQn', 'cmsis_line': line,
                                         'evidence': ['channel_count', 'interrupts']})
    assert len([p for p in peripherals if re.fullmatch(r'DMACHANNEL\d+', p)]) == count
    sdk = sdk_rows(dh, family == 'CW32L012')
    excluded = set(sdk) - set(rows)
    assert excluded == (set(range(37, 49)) if family == 'CW32L052' else set()), (family, excluded)
    for request, row in sorted(rows.items()):
        definition = sdk[request]
        peripheral, signal = normalize(definition['symbol'], family, row['description'])
        assert peripheral in peripherals, (family, peripheral)
        route = {'signal': signal, 'dma': 'DMA', 'request': request}
        result['peripheral_dma_channels'].setdefault(peripheral, []).append(route)
        result['requests'].append({'peripheral': peripheral, **route,
                                   'channels': [c['name'] for c in result['core_dma_channels']],
                                   'sdk': definition, 'manual': {**row, 'printed_page': row['pdf_page_index'] + page_offset},
                                   'evidence': ['request_table', 'channel_constraint']})
    for request in sorted(excluded):
        result['excluded_sdk_requests'].append({'request': request, **sdk[request],
                                                'reason': 'Absent from this family manual HARDSRC table and peripheral inventory; copied SDK define is not sufficient routing evidence.'})
    if family == 'CW32F020':
        result['source_discrepancies'] = [{'source': 'dma_header', 'symbol': 'IS_DMA_ALL_PERIPH',
                                         'description': 'SDK accepts CMSIS aliases for DMACHANNEL3/4/5, but manual sections 8.2 and 8.8.4 limit this family to physical channels 1/2. Do not create channels 3-5.'}]
    result['request_register'] = {'peripheral_kind': 'dmachannel', 'register': 'TRIG', 'field': 'HARDSRC', 'bit_offset': 2, 'bit_size': 6}
    result['normalization'] = {'channel_index': 'Zero-based upstream channel index; physical_channel is the one-based vendor ordinal.',
                               'request': 'Unshifted six-bit HARDSRC value, not the SDK macro value shifted left by two.',
                               'irq': 'CMSIS/generated interrupt spelling; the manual abbreviates DMACH as DMA.',
                               'signals': 'UART/SPI/I2C TX/RX; timer UP/TRIG/CHn; grouped ATIM sources remain grouped; ADC COMPLETE or SEQ/SINGLE matches the manual wording.',
                               'dmamux': 'No separate DMAMUX instance: request selection is internal to each DMA channel.'}
    if family == 'CW32L012':
        result['normalization']['aliases'] = 'SDK LPI2C1/2 maps to manual and generated I2C1/2; manual DAC1/2 are channels 1/2 of generated DAC, not separate peripherals.'
        result['unknowns'] = ['The HARDSRC table labels request 18 only HALLTIM. EVENT preserves that generic source without inventing a more specific Hall event.']
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--source-root', type=Path, default=ROOT.parent / 'cw32-sources')
    ap.add_argument('--check', action='store_true', help='Compare extracted facts with authored YAML without writing')
    args = ap.parse_args()
    sdk_urls = load(args.source_root / 'official-sdk-urls.json')
    datasheets = {x['family']: x for x in load(args.source_root / 'official-datasheet-manifest.json')}
    counts = {'profiles': 0, 'channels': 0, 'requests': 0, 'excluded_sdk_requests': 0}
    for family in sorted(set(CONFIG) | set(NO_DMA)):
        profile = build_profile(family, args.source_root, sdk_urls, datasheets)
        enrich_source_refs(profile, load(ROOT / 'sources/evidence-sources.json'))
        out = OUT / (family.lower() + '.yaml')
        if args.check:
            assert load(out) == profile, f'{family}: reviewed DMA data differs from pinned sources'
        else:
            out.write_text(yaml.safe_dump(profile, sort_keys=False, allow_unicode=True, width=100))
        counts['profiles'] += 1
        for key, field in [('channels', 'core_dma_channels'), ('requests', 'requests'), ('excluded_sdk_requests', 'excluded_sdk_requests')]:
            counts[key] += len(profile[field])
    print(json.dumps(counts, sort_keys=True))


if __name__ == '__main__':
    main()
