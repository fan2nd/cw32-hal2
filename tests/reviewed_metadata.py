"""Independent, exact projections of reviewed sidecars for generated-data audits.

No Rust generator code is reused here. This checks the curated inputs' projection;
source extraction and field-reference checks are covered by the pinout/DMA/clock
contracts separately.
"""
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import re
from test_clock_contracts import project as clock_projection
import yaml
from route_metadata import hardware_facts

ROOT = Path(__file__).resolve().parents[1]


@lru_cache(maxsize=None)
def load(relative):
    return hardware_facts(yaml.safe_load((ROOT / relative).read_text()) if (ROOT / relative).suffix == '.yaml' else json.loads((ROOT / relative).read_text()))


def gpio_names(package):
    return {signal for pin in package['pins'] for signal in pin['signals']
            if re.fullmatch(r'P[A-Z](?:[0-9]|1[0-5])', signal)}


def pin_projection(chip, manifest):
    sidecar = load(manifest['pinout_metadata'])
    assert sidecar['family'] == chip['line']
    exact = next((p for p in sidecar['packages'] if p['name'] == chip['name']), None)
    if exact:
        packages = [{'name': chip['name'], 'package': exact['package'], 'pins': exact['pins']}]
        pins = gpio_names(exact)
    else:
        assert manifest['alias_pin_policy'] == 'common-package-intersection'
        applicable = [p for p in sidecar['packages'] if chip['name'] in (sidecar['family'], p['family_part'])]
        assert applicable, 'Alias has no reviewed package scope'
        pins = set.intersection(*(gpio_names(p) for p in applicable))
        packages = []
    return packages, [{'name': name} for name in sorted(pins)]



@lru_cache(maxsize=None)
def f020_qualified_routes(kind):
    """Independent F020-only source tuple and package-position qualification."""
    assert kind in ('analog', 'pwm')
    data = load(f'cw32-data/af/cw32f020-{kind}.yaml')
    proof = load('docs/f020-adc-pwm-route-evidence.json')
    pinouts = load('cw32-data/pinouts/cw32f020.yaml')
    candidate = load('cw32-data/af/cw32f020.yaml')
    assert data['schema_version'] == 1 and data['profile'] == 'CW32F020' and data['kind'] == kind
    assert data['status'] == ('verified-sdk-datasheet-reference-manual' if kind == 'analog'
                              else 'verified-sdk-and-datasheet')
    assert data['alias_pin_policy'] == 'common-package-intersection'
    assert data['sources'] == proof['sources']
    sources = data['sources']
    expected_sources = {
        'datasheet': ('current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf',
                      '1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0'),
        'reference_manual': ('CW32F020_UserManual_CN_V1.4.pdf',
                             '279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed'),
        'sdk_archive': ('CW32F020_StandardPeripheralLib_V1.2.zip',
                        '1d77fece47a0c615c8ae51374ea946b17ab489042222f33e38d93e6969945b5d'),
        'adc_header': ('cw32f020/Libraries/inc/cw32f020_adc.h',
                       'e3b36174dea21de3d8d74c248c18c59450056728dbebce25751a55c92f3dd0cf'),
        'gpio_header': ('cw32f020/Libraries/inc/cw32f020_gpio.h',
                        'eccc3bb68452d2e3218397b2a795c4330e4a3d9b5a76481de128655e4602874d'),
    }
    for name, (file, digest) in expected_sources.items():
        assert sources[name]['file'] == file and sources[name]['sha256'] == digest
    for name, path in [('pinouts', 'cw32-data/pinouts/cw32f020.yaml'),
                       ('sdk_candidates', 'cw32-data/af/cw32f020.yaml')]:
        assert sources[name]['file'] == path
        assert sources[name]['sha256'] == hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
    assert pinouts['family'] == candidate['profile'] == 'CW32F020'
    assert pinouts['source']['sha256'] == sources['datasheet']['sha256']
    assert candidate['status'] == 'candidate-sdk-and-register-verified'
    assert candidate['source']['header_sha256'] == sources['gpio_header']['sha256']
    candidate_routes = {(r['pin'], r['af']): r for r in candidate['routes']}
    analog_pins = [f'PA{i}' for i in range(8)] + ['PB0', 'PB1', 'PB2', 'PB10', 'PB11']
    assert len(data['routes']) == (13 if kind == 'analog' else 46)
    seen = set()
    for route in data['routes']:
        pin = route['pin']
        row = next(r for r in pinouts['table_rows'] if pin in r['signals'])
        assert row['pin_type'] == 'I/O'
        assert not set(row['signals']) & {'SWDIO', 'SWCLK', 'NRST', 'BOOT'}
        assert route['pin_cell'] == dict(pin=pin, source_name=row['source_name'], table='5-2',
            pdf_page=row['pdf_page_index'] + 1, printed_page=row['pdf_page_index'],
            positions=row['positions'], pin_type='I/O', sha256=sources['datasheet']['sha256'])
        assert route['package_pins'] == {p['name']: row['positions'][p['table_column']]
                                         for p in pinouts['packages']}
        assert route['oscillator_aliases'] == [s for s in row['signals'] if s.startswith('OSC')]
        if kind == 'analog':
            mux = route['mux']
            assert 0 <= mux < 13 and pin == analog_pins[mux]
            assert route['peripheral'] == 'ADC' and route['af'] is None
            assert route['channel'] == mux and route['signal'] == f'IN{mux}'
            assert route['source_signal'] == f'ADC_IN{mux}'
            assert route['source_macro'] == f'ADC_ExInputCH{mux}'
            assert route['source_line'] == 206 + mux and route['sdk_pin_comment_line'] == 189 + mux
            assert route['source_sha256'] == sources['adc_header']['sha256']
            assert route['manual_cell'] == dict(table='21-5', pdf_page=378, printed_page=377,
                source_signal=f'AIN{mux}', mux_bits=f'{mux:04b}', pin=pin,
                sha256=sources['reference_manual']['sha256'])
            identity = mux
        else:
            assert re.fullmatch(r'GTIM[1-4]', route['peripheral'])
            assert route['signal'] in ('CH1', 'CH2', 'CH3', 'CH4')
            assert route['channel'] == int(route['signal'][-1]) and 1 <= route['af'] <= 7
            assert route['source_signal'] == route['signal'] and route['source_kind'] == 'sdk-and-datasheet'
            sdk = candidate_routes[(pin, route['af'])]
            assert all(route[key] == sdk[key] for key in ('pin', 'af', 'function', 'source_macro',
                'source_line', 'gpio_register', 'gpio_field', 'peripheral', 'signal'))
            assert route['source_sha256'] == sources['gpio_header']['sha256']
            page = 26 if pin[1] == 'A' else 27
            assert route['datasheet_cell'] == dict(pin=pin, af=route['af'],
                table={'A': '5-3', 'B': '5-4', 'C': '5-5', 'F': '5-6'}[pin[1]],
                pdf_page=page + 1, printed_page=page, function=f"{route['peripheral']}_{route['signal']}",
                sha256=sources['datasheet']['sha256'])
            identity = (pin, route['peripheral'], route['signal'])
        assert identity not in seen
        seen.add(identity)
    for part, counts in [('CW32F020F6U7', (9, 17)), ('CW32F020K6U7', (11, 32)),
                         ('CW32F020C6U7', (13, 46))]:
        assert sum(r['package_pins'][part] is not None for r in data['routes']) == counts[kind == 'pwm']
    return data['routes']


def af_projection(manifest, pins):
    result = {}
    if not manifest.get('af_metadata'):
        return result
    af = load(manifest['af_metadata'])
    assert af['schema_version'] == 1 and af['profile'] == manifest['line']
    assert af['status'] == 'verified-sdk-and-datasheet', 'Candidate AF routing must not be emitted'
    assert af['datasheet']['af_evidence'] and re.fullmatch(r'[0-9a-f]{64}', af['datasheet']['sha256'])
    # Independent family policy; L012's source tables have AF8/9. Never
    # infer valid selectors from a four-bit PAC register field.
    limits = {name: (7, 1) for name in ('CW32A030', 'CW32F002', 'CW32F003',
              'CW32F020', 'CW32F030', 'CW32L010', 'CW32L031', 'CW32R031',
              'CW32W031', 'CW32L052', 'CW32L083')}
    limits.update(CW32L011=(7, 3), CW32L012=(9, 3))
    max_af, page_offset = limits[manifest['line']]
    if manifest['line'] in ('CW32F002', 'CW32F003', 'CW32L010', 'CW32L011', 'CW32L012'):
        capability = af['selector_capability']
        assert (capability['min'], capability['max'], capability['pdf_printed_page_offset']) == (1, max_af, page_offset)
        assert capability['evidence']
    available = {p['name'] for p in pins}
    seen = set()
    for route in af['routes']:
        identity = (route['peripheral'], route['pin'], route['signal'])
        assert identity not in seen, 'Duplicate/conflicting AF route'
        seen.add(identity)
        assert 1 <= route['af'] <= max_af, 'Undocumented AF selector'
        kind = route.get('source_kind')
        if kind is not None:
            assert kind in ('datasheet', 'sdk-and-datasheet', 'datasheet-corrected-sdk')
            cell = route['datasheet_cell']
            assert cell['pin'] == route['pin'] and cell['af'] == route['af']
            assert re.fullmatch(r'5-[3-8]', cell['table']) and cell['function']
            assert cell['pdf_page'] == cell['printed_page'] + page_offset and cell['pdf_page'] > page_offset
        if kind == 'datasheet':
            assert route.get('source_macro') is None and route.get('source_line') is None
        else:
            assert route['source_macro'] and route['source_line'] > 0
        if route['pin'] in available:
            result.setdefault(route['peripheral'], []).append({key: route[key] for key in ('pin', 'signal', 'af')})
    if manifest.get('pwm_metadata'):
        if manifest['line'] == 'CW32F020':
            assert manifest['pwm_metadata'] == 'cw32-data/af/cw32f020-pwm.yaml'
            pwm_routes = f020_qualified_routes('pwm')
        elif manifest['line'] in ('CW32L010', 'CW32L011', 'CW32L012'):
            from verify_buffered_pwm_routes import qualified_routes
            pwm_routes = qualified_routes(manifest['line'])
        else:
            from verify_classic_pwm_routes import qualified_routes
            pwm_routes = qualified_routes(manifest['line'])
        for route in pwm_routes:
            assert (route['peripheral'], route['pin'], route['signal']) not in seen
            if route['pin'] in available:
                result.setdefault(route['peripheral'], []).append({key: route[key] for key in ('pin', 'signal', 'af')})
    if manifest.get('atim_pwm_metadata'):
        from verify_atim_pwm_routes import construct, sources
        family=manifest['line'][4:]
        evidence=load('docs/atim-pwm-route-evidence.json')['families'][manifest['line']]
        cells=[{(r['pin'],r['af']):r for r in evidence[k]} for k in ('datasheet_cells','manual_cells','sdk_cells')]
        sidecar,rebuilt=construct(family,sources(family),*cells)
        assert rebuilt==evidence and sidecar==load(manifest['atim_pwm_metadata'])
        result['ATIM']=[r for r in result.get('ATIM',[]) if r['signal'] not in ('CH1A','CH2A','CH3A','CH1','CH2','CH3','CH4')]
        result['ATIM'].extend({k:r[k] for k in ('pin','signal','af')} for r in sidecar['routes'] if r['pin'] in available)
    classic = load('cw32-data/classic-timer-input.yaml')['profiles'].get(manifest['line'])
    if classic:
        assert hashlib.sha256((ROOT / classic['route_source']).read_bytes()).hexdigest() == classic['route_source_sha256']
        for route in classic['routes']:
            assert route['peripheral'] in classic['instances'] and 1 <= route['channel'] <= 4
            if route['pin'] in available:
                source = dict(pin=route['pin'], signal=f"CH{route['channel']}", af=route['af'])
                assert source in result[route['peripheral']]
                result[route['peripheral']].append(dict(source, signal=f"CAP{route['channel']}"))
    if manifest.get('atim_complementary_metadata'):
        complementary = load(manifest['atim_complementary_metadata'])
        assert complementary['profile'] == manifest['line']
        assert complementary['kind'] == 'complementary-pwm'
        assert complementary['status'] == 'verified-sdk-and-datasheet'
        if manifest['line'] in ('CW32F030', 'CW32A030'):
            result['ATIM'] = [r for r in result.get('ATIM', []) if r['signal'] not in ('CH1B', 'CH2B', 'CH3B')]
        for route in complementary['routes']:
            assert route['peripheral'] == 'ATIM'
            assert route['signal'] in (('CH1B', 'CH2B', 'CH3B') if manifest['line'] in ('CW32F030', 'CW32A030') else ('CH1N', 'CH2N', 'CH3N', 'CH4N', 'BK'))
            if route['pin'] in available:
                projected = {key: route[key] for key in ('pin', 'signal', 'af')}
                assert projected not in result.setdefault('ATIM', [])
                result['ATIM'].append(projected)
    if manifest.get('halltim_metadata'):
        assert manifest['line'] == 'CW32L012'
        assert manifest['halltim_metadata'] == 'cw32-data/af/cw32l012-halltim.yaml'
        hall = load(manifest['halltim_metadata'])
        assert hall['status'] == 'verified-sdk-and-datasheet'
        assert hall['profile'] == 'CW32L012' and hall['register_version'] == 'cw32l012_v1'
        assert hall['datasheet']['sha256'] == '08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76'
        expected = {1: ['PA3', 'PB2', 'PB5', 'PB13'], 2: ['PA4', 'PB6', 'PB10', 'PB14'], 3: ['PA5', 'PB7', 'PB11', 'PB15']}
        actual = {(r['pin'], r['signal'], r['af']) for r in hall['routes']}
        assert len(hall['routes']) == 12 and actual == {(p, f'CH{c}', 9) for c, ps in expected.items() for p in ps}
        for route in hall['routes']:
            assert route['peripheral'] == 'HALLTIM'
            assert route['datasheet_cell'] == dict(pin=route['pin'], af=9,
                table='5-3' if route['pin'].startswith('PA') else '5-4', pdf_page=39,
                printed_page=36, function='HALLTIM_' + route['signal'])
            if route['pin'] in available:
                result.setdefault('HALLTIM', []).append({k:route[k] for k in ('pin', 'signal', 'af')})
    if manifest.get('analog_metadata'):
        if manifest['line'] == 'CW32F020':
            assert manifest['analog_metadata'] == 'cw32-data/af/cw32f020-analog.yaml'
            routes = f020_qualified_routes('analog')
        elif manifest['line'] in ('CW32F002', 'CW32F003', 'CW32L031', 'CW32R031', 'CW32W031', 'CW32L052', 'CW32L083'):
            from verify_classic_adc_routes import qualified_routes
            routes = qualified_routes(manifest['line'])
        elif manifest['line'] == 'CW32L012':
            from verify_l012_adc_routes import qualified_routes
            routes = qualified_routes()
        elif manifest['line'] in ('CW32L010', 'CW32L011'):
            from verify_low_adc_routes import qualified_routes
            assert manifest['analog_metadata'] == f"cw32-data/af/{manifest['line'].lower()}-analog.yaml"
            routes = qualified_routes(manifest['line'])
        else:
            analog = load(manifest['analog_metadata'])
            assert manifest['line'] in analog['profiles']
            assert analog['status'] == 'verified-sdk-and-datasheets-and-reference-manuals'
            assert {r['channel'] for r in analog['routes']} == set(range(13))
            routes = analog['routes']
        for route in routes:
            assert route['af'] is None and route['signal'] == f"IN{route['channel']}"
            if route['pin'] in available:
                result.setdefault(route['peripheral'], []).append({'pin':route['pin'],'signal':route['signal'],'adc_mux':route['mux']})
    if manifest.get('comparator_metadata'):
        comparator = load(manifest['comparator_metadata'])
        assert comparator['profile'] == manifest['line']
        assert comparator['status'] == 'verified-own-manual-and-datasheet'
        for route in comparator['routes']:
            assert route['af'] is None and route['direction'] in ('INP', 'INN')
            assert route['signal'].startswith(route['direction'])
            if route['pin'] in available:
                result.setdefault(route['peripheral'], []).append({
                    'pin': route['pin'], 'signal': route['signal'], 'comparator_mux': route['mux']})
    if manifest['line'] == 'CW32L012':
        analog_outputs = load('cw32-data/dac-opa.yaml')
        assert analog_outputs['profile'] == manifest['line']
        for route in analog_outputs['routes']:
            assert route['af'] is None
            if route['pin'] in available:
                result.setdefault(route['peripheral'], []).append({key: route[key] for key in ('pin', 'signal')})
    lvd_ir = load('cw32-data/lvd-ir.yaml')
    assert lvd_ir['evidence'] == 'docs/lvd-ir-evidence.json'
    profile = lvd_ir['profiles'][manifest['line']]
    controller = profile['ir_controller']
    for route in profile['ir_routes']:
        assert route['peripheral'] == controller and route['signal'] == 'IR_OUT'
        assert route['source_kind'] in ('sdk-and-datasheet', 'manual-and-datasheet')
        if route['pin'] in available:
            result.setdefault(controller, []).append({key: route[key] for key in ('pin', 'signal', 'af')})
    if manifest['line'] in ('CW32L052', 'CW32L083'):
        lcd = load('cw32-data/lcd.yaml')['profiles'][manifest['line']]
        result['LCD'] = [{k: r[k] for k in ('pin', 'signal')} for r in lcd['routes'] if r['pin'] in available]
    for routes in result.values():
        routes.sort(key=lambda route: (route['pin'], route['signal'], route.get('af')))
    return result


def projection_errors(chip, manifest):
    errors = []
    if 'device_id' in chip:
        errors.append('Unknown device ID must not become zero')
    expected_packages, expected_pins = pin_projection(chip, manifest)
    if chip['packages'] != expected_packages:
        errors.append('Physical package projection differs from reviewed table')
    af = af_projection(manifest, expected_pins)
    electrical = load(manifest['electrical_metadata'])['profiles'][chip['line']]['clock_limits']
    oscillator_signals = []
    if electrical.get('hse') is not None:
        oscillator_signals.extend([('OSC_IN', 'HSE_IN'), ('OSC_OUT', 'HSE_OUT')])
    if electrical.get('lse') is not None:
        oscillator_signals.extend([('OSC32_IN', 'LSE_IN'), ('OSC32_OUT', 'LSE_OUT')])
    if oscillator_signals:
        # Oscillator pads are physical analog aliases, with no digital AF selector.
        pinouts = load(manifest['pinout_metadata'])
        exact = [p for p in pinouts['packages'] if p['name'] == chip['name']]
        applicable = exact or [p for p in pinouts['packages']
                               if chip['name'] in (pinouts['family'], p['family_part'])]
        assert applicable
        columns = {p['table_column'] for p in applicable}
        for alias, signal in oscillator_signals:
            rows = [row for row in pinouts['table_rows'] if alias in row['signals']]
            assert len(rows) <= 1
            if rows and all(rows[0]['positions'][column] is not None for column in columns):
                labels = [s for s in rows[0]['signals'] if re.fullmatch(r'P[A-Z](?:[0-9]|1[0-5])', s)]
                assert len(labels) == 1 and labels[0] in {p['name'] for p in expected_pins}
                af.setdefault('SYSCTRL', []).append({'pin': labels[0], 'signal': signal})
        af['SYSCTRL'].sort(key=lambda route: (route['pin'], route['signal'], route.get('af')))
    if electrical.get('hex') is not None:
        # Independent HEX routes come from their own review; exact-package
        # availability still comes from the unchanged physical pin tables.
        qualified = load('cw32-data/hex-qualified.yaml')['families'][chip['line']]
        assert electrical['hex'] == qualified['hex']
        names = {p['name'] for p in expected_pins}
        af.setdefault('SYSCTRL', []).extend(route for route in qualified['routes'] if route['pin'] in names)
        af['SYSCTRL'].sort(key=lambda route: (route['pin'], route['signal'], route.get('af')))
    dma = load(manifest['dma_metadata'])
    clocks = load(manifest['clock_metadata'])
    assert dma['family'] == clocks['profile'] == chip['line']
    clock_records = {r['name']: r for r in clocks['peripherals']}
    for core in chip['cores']:
        if core['pins'] != expected_pins:
            errors.append('Core GPIO projection differs from reviewed package scope')
        if core['dma_channels'] != dma['core_dma_channels']:
            errors.append('Core DMA projection differs from reviewed channels')
        for peripheral in core['peripherals']:
            name = peripheral['name']
            expected_rtc = load('cw32-data/rtc-calendar.yaml')['profiles'].get(manifest['line']) if name == 'RTC' else None
            if peripheral.get('rtc_calendar') != expected_rtc:
                errors.append('RTC source facts differ from reviewed own-family evidence')
            expected_lcd = (load('cw32-data/lcd.yaml')['profiles'][manifest['line']]['facts']
                            if name == 'LCD' else None)
            if peripheral.get('lcd') != expected_lcd:
                errors.append('LCD capabilities differ from reviewed own-family facts')
            if peripheral.get('dma_channels', []) != dma['peripheral_dma_channels'].get(name, []):
                errors.append(f'{name}: DMA projection differs from reviewed requests')
            if peripheral.get('rcc') != clock_projection(clock_records[name], clocks['controller']):
                errors.append(f'{name}: RCC projection differs from reviewed supported record')
            if peripheral.get('pins', []) != af.get(name, []):
                errors.append(f'{name}: AF projection differs from reviewed package-filtered routes')
            expected_triggers = []
            if manifest.get('trigger_metadata'):
                routes = load(manifest['trigger_metadata'])['routes']
                expected_triggers = [{key: route[key] for key in ('signal', 'source', 'registers')}
                                     for route in routes if route['destination'] == name]
            if peripheral.get('triggers', []) != expected_triggers:
                errors.append(f'{name}: trigger projection differs from reviewed routes')
            for key in ('afio',):
                if peripheral.get(key):
                    errors.append(f'Unreviewed {name}.{key}')
    return errors
