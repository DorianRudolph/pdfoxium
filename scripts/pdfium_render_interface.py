#!/usr/bin/env python3
"""Read Rust public signatures without changing their spelling.

Used by the interface audit and by the porting generator. Function implementations
are deliberately excluded: the contract comes from upstream, the backend from core.
"""
import re
from pathlib import Path


def mask(source):
    pattern = r'//[^\n]*|/\*[\s\S]*?\*/|r(#+)?"[\s\S]*?"\1|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\''
    return re.sub(pattern, lambda m: re.sub(r'[^\n]', ' ', m.group()), source)


def close_brace(text, start):
    depth = 1
    for index in range(start + 1, len(text)):
        depth += (text[index] == '{') - (text[index] == '}')
        if depth == 0:
            return index
    raise ValueError(f'unclosed brace at {start}')


def implementations(source):
    text = mask(source)
    pattern = r'\bimpl(?:\s*<[^{}]*?>)?\s+([^{}]+?)\s*\{'
    for match in re.finditer(pattern, text):
        header = match.group(1).strip()
        if ' for ' in header or header.startswith('Default '):
            continue
        name = re.match(r'(\w+)', header)
        if name:
            start, end = match.end() - 1, close_brace(text, match.end() - 1)
            yield name.group(1), source[match.start():start].strip(), start, end


def methods(source, type_name):
    text = mask(source)
    result = {}
    for name, impl_header, start, end in implementations(source):
        if name != type_name:
            continue
        pattern = r'\bpub\s+(?:(?:const|async|unsafe)\s+)*fn\s+(\w+)'
        for match in re.finditer(pattern, text[start + 1:end]):
            offset = start + 1 + match.start()
            opening = text.index('{', offset)
            signature = source[offset:opening].strip()
            if 'unsafe fn ' not in signature:
                result[match.group(1)] = signature
    return result


def declaration(source, kind, name):
    text = mask(source)
    match = re.search(r'\bpub\s+' + kind + r'\s+' + name + r'\b', text)
    if not match:
        raise ValueError((kind, name))
    opening = text.index('{', match.start())
    return source[match.start():close_brace(text, opening) + 1]


def emit_impl(source, type_name, impl_header, bodies):
    signatures = methods(source, type_name)
    missing = set(signatures) - set(bodies)
    extra = set(bodies) - set(signatures)
    if missing or extra:
        raise ValueError(f'{type_name}: missing bodies {sorted(missing)}, unknown methods {sorted(extra)}')
    return impl_header + ' {\n' + '\n'.join(
        signature + ' {\n' + bodies[name] + '\n}'
        for name, signature in signatures.items()
    ) + '\n}\n'


def emit_native_impl(source, type_name, impl_header, bodies):
    """Emit native signatures; browser-only functions are audited separately."""
    signatures = {name: sig for name, sig in methods(source, type_name).items()
                  if 'Blob' not in sig and not sig.startswith('pub async fn')}
    if set(signatures) != set(bodies):
        raise ValueError((type_name, sorted(set(signatures)-set(bodies)), sorted(set(bodies)-set(signatures))))
    return impl_header + ' {\n' + '\n'.join(signature + ' {\n' + bodies[name] + '\n}'
        for name, signature in signatures.items()) + '\n}\n'


def trait_methods(source, name):
    text = mask(source)
    match = re.search(r'\bpub\s+trait\s+' + name + r'\b', text)
    start = text.index('{', match.start())
    end = close_brace(text, start)
    result = {}
    position = start + 1
    while position < end:
        match = re.search(r'\bfn\s+(\w+)', text[position:end])
        if not match:
            break
        offset = position + match.start()
        delimiter = re.search(r'[;{]', text[offset:end])
        stop = offset + delimiter.start()
        result[match.group(1)] = (source[offset:stop].strip(), text[stop] == ';')
        position = stop + 1 if text[stop] == ';' else close_brace(text, stop) + 1
    return result
