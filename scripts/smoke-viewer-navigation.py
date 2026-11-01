#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise viewer links, outline and location history through the real control channel."""
import argparse
import json
from pathlib import Path
import socket
import subprocess
import time


def navigation_pdf(path):
    objects = []
    def add(value):
        objects.append(value)
        return len(objects)
    catalog = add('')
    pages = add('')
    font = add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>')
    page_ids = []
    for index in range(3):
        page_ids.append(add(''))
        content = f'BT /F1 24 Tf 60 720 Td (Navigation page {index + 1}) Tj ET\n0.2 0.6 0.9 RG 2 w 60 600 240 40 re S\nBT /F1 18 Tf 70 612 Td (Go to chapter) Tj ET'
        stream = add(f'<< /Length {len(content)} >>\nstream\n{content}\nendstream')
        geometry = '/MediaBox [0 0 640 860] /CropBox [20 30 620 830] /Rotate 90' if index == 1 else '/MediaBox [0 0 600 800]'
        objects[page_ids[-1]-1] = f'<< /Type /Page /Parent {pages} 0 R {geometry} /Resources << /Font << /F1 {font} 0 R >> >> /Contents {stream} 0 R LINK_ANNOTS >>'
    links = [
        add(f'<< /Type /Annot /Subtype /Link /Rect [60 600 300 640] /Dest [{page_ids[1]} 0 R /XYZ 80 700 1.25] >>'),
        add('<< /Type /Annot /Subtype /Link /Rect [60 540 300 580] /Dest /chapter >>'),
        add('<< /Type /Annot /Subtype /Link /Rect [60 480 300 520] /A << /S /URI /URI (https://example.org) >> >>'),
        add('<< /Type /Annot /Subtype /Link /Rect [60 420 300 460] /A << /S /Launch /F (untrusted.exe) >> >>'),
    ]
    for index, page in enumerate(page_ids):
        refs = '/Annots [' + ' '.join(f'{link} 0 R' for link in links) + ']' if index == 0 else ''
        objects[page-1] = objects[page-1].replace('LINK_ANNOTS', refs)
    outline = add('')
    first = add('')
    child = add('')
    last = add('')
    objects[outline-1] = f'<< /Type /Outlines /First {first} 0 R /Last {last} 0 R /Count 3 >>'
    objects[first-1] = f'<< /Title (  Start\n\n) /Parent {outline} 0 R /First {child} 0 R /Last {child} 0 R /Count 1 /Next {last} 0 R /Dest [{page_ids[0]} 0 R /Fit] >>'
    objects[child-1] = f'<< /Title (Rotated\tchapter\n) /Parent {first} 0 R /Dest /chapter >>'
    objects[last-1] = f'<< /Title (Detail) /Parent {outline} 0 R /Prev {first} 0 R /Dest [{page_ids[2]} 0 R /FitR 40 400 340 700] >>'
    objects[catalog-1] = f'<< /Type /Catalog /Pages {pages} 0 R /Outlines {outline} 0 R /Dests << /chapter [{page_ids[1]} 0 R /XYZ 80 700 1.25] >> >>'
    objects[pages-1] = '<< /Type /Pages /Kids [' + ' '.join(f'{page} 0 R' for page in page_ids) + '] /Count 3 >>'
    pdf = '%PDF-1.7\n'
    offsets = []
    for index, obj in enumerate(objects):
        offsets.append(len(pdf))
        pdf += f'{index + 1} 0 obj\n{obj}\nendobj\n'
    xref = len(pdf)
    pdf += f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'
    pdf += ''.join(f'{offset:010} 00000 n \n' for offset in offsets)
    pdf += f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
    path.write_text(pdf)


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/release/pdfoxium-viewer')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--book', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    endpoint_path = args.output / 'control.json'
    assert not endpoint_path.exists()
    report = []
    pdf = (args.output / 'navigation.pdf').resolve()
    navigation_pdf(pdf)
    with (args.output / 'app.log').open('w') as log:
        app = subprocess.Popen([str(args.binary), str(pdf), '--control', str(endpoint_path.resolve()), '--control-only-input'], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while not endpoint_path.exists():
                assert app.poll() is None, 'viewer exited; see app.log'
                assert time.monotonic() < deadline, 'viewer did not start'
                time.sleep(0.05)
            endpoint = json.loads(endpoint_path.read_text())

            def call(method, params=None, error=False):
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as conn:
                    conn.sendall(json.dumps({'jsonrpc': '2.0', 'id': len(report), 'token': endpoint['token'], 'method': method, 'params': params or {}}).encode() + b'\n')
                    response = json.loads(conn.makefile('rb').readline())
                report.append({'method': method, 'params': params or {}, 'response': response})
                if error:
                    assert 'error' in response, response
                    return response['error']
                assert 'error' not in response, response
                return response['result']

            def wait():
                state = call('ui.wait')
                assert state['settled'] and state['active']['render_complete']
                return state

            initial = wait()
            assert not initial['active']['navigation']['can_back']
            outline = call('ui.outline')
            assert [item['title'] for item in outline['items']] == ['Start', 'Rotated chapter', 'Detail']
            assert [item['depth'] for item in outline['items']] == [0, 1, 0]
            links = call('ui.links', {'page': 1})
            assert not links['pending'] and len(links['links']) == 4
            assert links['links'][2]['target'] == 'https://example.org'
            def snapshot(state):
                return [state['active'][key] for key in ['scroll', 'zoom', 'rotation']]
            def command(id, **params):
                call('ui.command', {'id': id, **params})
                return wait()
            # Click the actual rendered annotation, rather than a navigation command.
            x, y, w, h = links['links'][0]['screen_rects'][0]
            call('ui.click', {'position': [x+w/2, y+h/2]})
            chapter = wait()
            assert chapter['active']['page'] == 2 and chapter['active']['zoom'] == 1.25
            assert chapter['active']['navigation']['can_back']
            back = command('navigation.back')
            assert snapshot(back) == snapshot(initial), (snapshot(back), snapshot(initial))
            assert back['active']['navigation']['can_forward']
            forward = command('navigation.forward')
            assert snapshot(forward) == snapshot(chapter)
            command('navigation.back')
            named = command('navigation.link', page=1, index=links['links'][1]['index'])
            assert snapshot(named) == snapshot(chapter)
            command('navigation.back')
            call('ui.click', {'id': 'view.outline'})
            wait()
            sidebar = call('ui.outline')
            assert sidebar['open']
            rows = [item['rect'] for item in sidebar['items']]
            assert all(row is not None for row in rows)
            assert max(row[3] for row in rows) - min(row[3] for row in rows) < 0.01
            assert abs((rows[1][1]-rows[0][1]) - (rows[2][1]-rows[1][1])) < 0.01
            path = (args.output / 'outline-rows.png').resolve()
            call('ui.screenshot', {'path': str(path)})
            row = call('ui.outline')['items'][1]['rect']
            assert row is not None
            x, y, w, h = row
            call('ui.click', {'position': [x+w/2, y+h/2]})
            chapter = wait()
            assert chapter['active']['page'] == 2
            path = (args.output / 'outline-chapter.png').resolve()
            call('ui.screenshot', {'path': str(path)})
            detail = command('navigation.outline', index=2)
            assert detail['active']['page'] == 3
            assert detail['active']['zoom'] > 1
            command('navigation.back')
            command('view.page', page=1)
            assert not wait()['active']['navigation']['can_forward']
            # A drag that begins inside a link selects text without following it.
            call('ui.click', {'id': 'view.outline'})
            command('navigation.outline', index=0)
            for rotation in [0, 90, 180, 270]:
                if rotation:
                    command('view.rotate')
                source = command('navigation.outline', index=0)
                link = call('ui.links', {'page': 1})['links'][0]
                x, y, w, h = link['screen_rects'][0]
                if rotation == 0:
                    call('ui.drag', {'from': [x+w/3, y+h/2], 'to': [x+2*w/3, y+h/2]})
                    assert wait()['active']['page'] == 1
                call('ui.click', {'position': [x+w/2, y+h/2]})
                target = wait()['active']
                target_rect = next(item['rect'] for item in target['pages_on_screen'] if item['page'] == 2)
                # /XYZ (80,700) in the cropped, intrinsically 90-degree page.
                unit = (0.8375, 0.1)
                for _ in range(rotation//90):
                    unit = (1-unit[1], unit[0])
                target_y = target_rect[1] + unit[1]*target_rect[3]
                assert abs(target_y - target['viewport'][1] - 8) <= 1.1, (rotation, target_y, target['viewport'])
                restored = command('navigation.back')
                assert snapshot(restored) == snapshot(source)
            call('ui.command', {'id': 'navigation.link', 'page': 1, 'index': 3}, error=True)
            call('ui.command', {'id': 'navigation.outline', 'index': 999}, error=True)
            call('ui.links', {'page': 99}, error=True)
            # A navigation to a new document must discard history and link models.
            call('ui.open', {'path': str(pdf)})
            reopened = wait()
            assert not reopened['active']['navigation']['can_back']
            assert not reopened['active']['navigation']['can_forward']
            if args.book:
                call('ui.open', {'path': str(args.book.resolve())})
                wait()
                if not call('ui.outline')['open']:
                    command('view.outline')
                book_outline = call('ui.outline', {'limit': 500})
                path = (args.output / 'book-outline.png').resolve()
                call('ui.screenshot', {'path': str(path)})
                roots = [item for item in book_outline['items'] if item['depth'] == 0 and item['rect'] is not None]
                assert [item['title'] for item in roots] == ['', 'Mechanical Computation', '', 'Background', '', 'Languages, Grammars, and Graphs', '', 'Automata', '', 'Computational Complexity', 'Appendix'], roots
                pitches = [b['rect'][1]-a['rect'][1] for a,b in zip(roots, roots[1:])]
                assert max(pitches)-min(pitches) < 0.01, pitches
                # Empty titles still represent real destinations and remain clickable.
                empty = roots[0]
                assert empty['actionable'] and empty['destination_page'] == 11
                x, y, w, h = empty['rect']
                call('ui.click', {'position': [x+w/2, y+h/2]})
                empty_target = wait()['active']
                assert 11 in empty_target['visible_pages'], empty_target
                command('navigation.back')
            call('ui.quit')
            assert app.wait(timeout=15) == 0
            assert not endpoint_path.exists()
            print(f'Navigation viewer smoke passed: {len(report)} records; screenshots in {args.output}')
        finally:
            (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
            if app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()


if __name__ == '__main__':
    main()
