#!/usr/bin/env python3
"""Check sensible owned Rust module layout and literal path references.

Leaf modules use name.rs. A directory/mod.rs is reserved for a module containing
real sibling files or submodules. Generated PAC and its templates follow the same
rule. Crate/test entrypoints and inline namespaces keep normal Rust semantics.
"""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
CRATES = ['embassy-cw32', 'cw32-data-gen', 'cw32-data-serde',
          'cw32-data-macros', 'cw32-metapac-gen', 'cw32-metapac']
count = 0
for crate in CRATES:
    sources = [ROOT / crate / 'src']
    if crate == 'cw32-metapac-gen':
        sources.append(ROOT / crate / 'res/src')
    for source in sources:
        for file in source.rglob('*.rs'):
            if file.name == 'mod.rs':
                siblings = [p for p in file.parent.iterdir() if p.name != 'mod.rs']
                assert siblings, f'Pointless one-file module directory: {file.relative_to(ROOT)}; use {file.parent.name}.rs'
            text = file.read_text()
            assert not re.search(r'(?s)#\[\s*(?:path\s*=|cfg_attr\([^]]*\bpath\s*=)', text), f'Forbidden module path attribute: {file.relative_to(ROOT)}'
            for target in re.findall(r'\binclude(?:_str|_bytes)?!\(\s*"([^"\\]+)"', text):
                # The metadata template receives these generated siblings in the output crate.
                if crate == 'cw32-metapac-gen' and 'res' in file.parts and target in {'all_chips.rs', 'all_peripheral_versions.rs'}:
                    continue
                assert (file.parent / target).is_file(), f'Dangling include path: {file.relative_to(ROOT)} -> {target}'
            count += 1
# Build scripts and generated root build-data modules use the same rule.
extra_sources = [file for crate in CRATES for file in (ROOT / crate).glob('*.rs')]
extra_sources += list((ROOT / 'cw32-metapac-gen/res').glob('*.rs'))
extra_sources += [file for file in (ROOT / 'examples').rglob('*.rs') if 'target' not in file.parts]
for file in extra_sources:
    assert not re.search(r'(?s)#\[\s*(?:path\s*=|cfg_attr\([^]]*\bpath\s*=)', file.read_text()), f'Forbidden module path attribute: {file.relative_to(ROOT)}'
print(f'PASS module layout: {count} Rust source files; flat leaves, meaningful grouped modules, no path attributes, valid literal includes')
