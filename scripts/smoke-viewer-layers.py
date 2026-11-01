#!/usr/bin/env python3
# Copyright 2026 The pdfoxium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise the native layers dropdown through the viewer's normal input path.
Start pdfoxium-viewer --control FILE, keep its window visible, then run this script FILE.
"""
import json
from pathlib import Path
import subprocess
import sys
import time

root = Path(__file__).resolve().parents[1]
control = sys.argv[1]
output = root / 'output/playwright/layers'
output.mkdir(parents=True, exist_ok=True)

def call(method, **params):
    for attempt in range(3):
        process = subprocess.run([str(root/'target/debug/pdfoxium-viewer'), 'ui', '--control', control, method, json.dumps(params)], text=True, capture_output=True)
        transient = any(message in process.stderr for message in
                        ('Connection reset by peer', 'EOF while parsing a value'))
        if process.returncode and method in ('ui.inspect','ui.state','ui.wait') and transient and attempt < 2:
            time.sleep(0.2)
            continue
        assert process.returncode == 0, (method,process.stdout,process.stderr)
        reply = json.loads(process.stdout)
        assert 'error' not in reply, reply
        return reply['result']
def wait():
    call('ui.wait')
def command(id, **params):
    call('ui.command', id=id, **params)
    wait()
def active():
    return call('ui.state')['active']
def click(id):
    widgets = call('ui.inspect')['widgets']
    widget = next(w for w in widgets if w['id'] == id)
    assert widget['enabled'], widget
    x,y,w,h = widget['rect']
    call('ui.click', position=[x+w/2,y+h/2])
    wait()
def escape():
    call('ui.key', key='Escape')
    wait()
def capture(name):
    call('ui.screenshot', path=str(output/name))

command('demo.choose', sample='layers')
command('view.zoom', zoom=0.8)
command('view.thumbnails')
assert len(active()['layers']['groups']) == 4
assert all(row['source']=='rendered' for row in active()['thumbnails']['rows'] if row['ready'])
capture('native-all.png')
click('menu.layers')
capture('native-menu.png')
widgets=call('ui.inspect')['widgets']
assert not next(w for w in widgets if w['id']=='layers.3')['enabled']
click('layers.0')
escape()
assert not active()['layers']['groups'][0]['visible']
assert active()['thumbnails']['open'] and active()['thumbnails']['cached'] > 0
capture('native-no-roads.png')
command('page.next')
assert not active()['layers']['groups'][0]['visible']
command('page.previous')
click('menu.layers')
click('layers.configuration.1')
escape()
assert [g['visible'] for g in active()['layers']['groups']]==[False,True,False,True]
capture('native-buildings.png')
click('menu.layers')
click('layers.reset')
escape()
assert all(g['visible'] for g in active()['layers']['groups'])
command('navigation.link', page=1, index=0)
assert not active()['layers']['groups'][0]['visible']
assert not active()['forms']['dirty']
command('view.annotations')  # Switch to the owning document worker.
command('layers.set', index=1, visible=False)
assert not active()['layers']['groups'][1]['visible']
command('layers.reset')
assert all(g['visible'] for g in active()['layers']['groups'])
assert not active()['errors']
print('Native layers: dropdown, lock, both pages, configurations, reset, PDF action, editor worker and transient state passed')
