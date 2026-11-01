#!/usr/bin/env python3
"""Check the copied safe declarations against the pinned upstream interface.

SAFE_API.json records the public prelude of pdfium-render 0.9.4, including
browser methods and trait declarations. Native binding/bootstrap methods are
explicitly excluded. Macro-generated transforms are checked separately.
"""
import json
import hashlib
import re
import sys
from pathlib import Path
sys.dont_write_bytecode = True
from pdfium_render_interface import methods, trait_methods, declaration, mask

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / 'crates/pdfoxium-render'
BASELINE = json.loads((CRATE / 'SAFE_API.json').read_text())
SOURCES = sorted((CRATE / 'src/backend').glob('*.rs')) + sorted((CRATE / 'src').glob('*.rs'))
ACTUAL = {}
for path in SOURCES:
    if path.name.endswith('tests.rs'):
        continue
    source = path.read_text()
    for kind, name in re.findall(r'pub\s+(enum|struct|trait|type)\s+(\w+)', mask(source)):
        # Root facade definitions override private backend helpers with the same name.
        if name not in ACTUAL or path.parent == CRATE / 'src':
            previous = ACTUAL.get(name, {}).get('methods', {}) if path.parent == CRATE / 'src' else {}
            ACTUAL[name] = {'kind': kind, 'source': source, 'methods': previous | methods(source, name)}
    # Browser implementations and typed-object implementations reside in separate files.
    for name in BASELINE['types']:
        found = methods(source, name)
        if found:
            item = ACTUAL.setdefault(name, {'kind': BASELINE['types'][name]['kind'], 'source': source, 'methods': {}})
            item['methods'].update(found)


def canonical(signature):
    return re.sub(r'\s+', '', signature)


def derived_traits(source, name):
    text=mask(source)
    match=re.search(r'pub\s+(?:struct|enum)\s+'+re.escape(name)+r'\b',text)
    if not match:
        return set()
    prefix=text[:match.start()]
    traits=set()
    for attribute in re.finditer(r'#\[derive\(([^)]*)\)\]',prefix):
        suffix=re.sub(r'#\[[\s\S]*?\]','',prefix[attribute.end():])
        if not suffix.strip():
            traits.update(t.strip() for t in attribute[1].split(',') if t.strip())
    return traits


def enum_variants(source, name):
    text = re.sub(r'#\[[\s\S]*?\]', '', declaration(source, 'enum', name))
    text = mask(text)
    start = text.index('{') + 1
    depth = 0
    parts = []
    for index in range(start, len(text) - 1):
        char = text[index]
        depth += (char in '({[') - (char in ')}]')
        if char == ',' and depth == 0:
            parts.append(text[start:index])
            start = index + 1
    parts.append(text[start:-1])
    result = {}
    counter = -1
    for part in parts:
        part = part.strip()
        if not part:
            continue
        variant = re.match(r'\w+', part)[0]
        value = part[len(variant):].strip()
        if value.startswith('='):
            expression = value[1:].strip().split(' as ')[0]
            expression = BASELINE['constants'].get(expression, expression)
            try:
                counter = int(expression, 0)
                value = '=' + str(counter)
            except ValueError:
                value = '=' + expression
        else:
            counter += 1
            value = canonical(value) or '=' + str(counter)
        result[variant] = value
    return result


errors = []
for name, expected in BASELINE['types'].items():
    actual = ACTUAL.get(name)
    if actual is None:
        errors.append(f'Missing type: {name}')
        continue
    for method, signature in expected['methods'].items():
        found = actual['methods'].get(method)
        if found is None:
            errors.append(f'Missing method: {name}::{method}')
        elif canonical(found) != canonical(signature):
            errors.append(f'Signature mismatch: {name}::{method}\n  expected: {signature}\n  actual: {found}')
    missing_derives=set(expected.get('derives',[]))-derived_traits(actual['source'],name)
    if missing_derives:
        errors.append(f'Missing derives on {name}: {sorted(missing_derives)}')
    if expected['kind'] == 'trait':
        found = trait_methods(actual['source'], name)
        for method, signature in expected['trait_methods'].items():
            if method not in found or canonical(found[method][0]) != canonical(signature):
                errors.append(f'Trait signature mismatch: {name}::{method}')
    if expected['kind'] == 'enum':
        variants = enum_variants(actual['source'], name)
        if set(variants)!=set(expected['variants']):
            errors.append(f'Enum variant set mismatch: {name}')
        for variant, value in expected['variants'].items():
            if variants.get(variant) != value:
                errors.append(f'Enum variant mismatch: {name}::{variant}: {value} != {variants.get(variant)}')
    if 'public_fields' in expected:
        fields = re.findall(r'pub\s+(\w+)\s*:\s*([^,}\n]+)', mask(declaration(actual['source'], 'struct', name)))
        if [(n, canonical(t)) for n,t in fields] != [(n, canonical(t)) for n,t in expected['public_fields']]:
            errors.append(f'Public fields mismatch: {name}')

macro_source=(CRATE / 'src/backend/transform.rs').read_text()
macro_hash=hashlib.sha256(canonical(mask(macro_source)).encode()).hexdigest()
if macro_hash!=BASELINE['transform_code_sha256']:
    errors.append('Copied transformation macros differ from upstream')

if errors:
    print('\n'.join(errors), file=sys.stderr)
    sys.exit(1)
print(f"Safe interface matches {BASELINE['upstream']}: {len(BASELINE['types'])} public definitions, "
      f"{sum(len(i['methods']) for i in BASELINE['types'].values())} inherent methods, "
      f"{sum(len(i.get('trait_methods', {})) for i in BASELINE['types'].values())} trait methods.")
