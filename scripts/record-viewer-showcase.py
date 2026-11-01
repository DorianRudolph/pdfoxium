#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Record a real-time viewer walkthrough through JSON-RPC (Pillow, ReportLab).

Raw app frames, pointer events and captions are kept separately. The companion
encoder overlays the same pointer coordinates and preserves elapsed time.
"""
import argparse
import hashlib
import itertools
import json
import math
from pathlib import Path
import socket
import subprocess
import threading
import time
from PIL import Image
from reportlab.pdfbase.pdfmetrics import stringWidth

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT/'crates/pdfoxium-viewer/assets/demos'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=ROOT/'target/release/pdfoxium-viewer')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--rehearse', action='store_true', help='Run faster and keep chapter stills instead of video frames')
parser.add_argument('--reel', action='store_true', help='Record a compact feature reel, cutting document setup between scenes')
parser.add_argument('--capture-fps', type=float, default=20)
parser.add_argument('--stop-after', default='')
parser.add_argument('--start-at', default='', choices=['','xfa'])
args = parser.parse_args()
if not 1 <= args.capture_fps <= 60: parser.error('--capture-fps must be 1..60')
args.output = args.output.resolve()
args.output.mkdir(parents=True, exist_ok=True)
endpoint_path = args.output/'control.json'
assert not endpoint_path.exists()
assert not (args.output/'timeline.json').exists(), 'Use a fresh output directory'
assert not (args.output/'frames').exists(), 'Use a fresh output directory'
class FinishedEarly(Exception): pass

CHAPTERS = ['intro', 'navigation', 'layers', 'search', 'annotations', 'pages', 'forms', 'xfa', 'dynamic', 'animation', 'outro']

def include(chapter):
    return not args.reel and (not args.start_at or CHAPTERS.index(chapter)>=CHAPTERS.index(args.start_at))

speed = .12 if args.rehearse else (.5 if args.reel else 1.)
ids = itertools.count()
events, frames, calls, capture_errors, capture_retries = [], [], [], [], []
stop = threading.Event()
rpc_lock = threading.Lock()
transport_retries = []
epoch = time.monotonic()
cursor = [1180., 740.]
source_hashes = {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in ASSETS.glob('*.pdf')}

def now(): return time.monotonic()-epoch
def hold(seconds): time.sleep(seconds*speed)

def rpc(method, **params):
    request = dict(jsonrpc='2.0', id=next(ids), token=endpoint['token'], method=method, params=params)
    # Serialize capture and input transport; do not consume several server slots
    # while a GPU readback is pending. Only read-only calls may be retried.
    readonly = method in {'ui.state','ui.inspect','ui.wait','ui.forms','ui.annotations',
                          'ui.thumbnails','ui.outline','ui.selection','ui.links','ui.screenshot'}
    readonly = readonly or (method == 'ui.pointer' and params.get('action') == 'move')
    for attempt in range(4):
        try:
            with rpc_lock:
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=50) as conn:
                    conn.sendall((json.dumps(request)+'\n').encode())
                    with conn.makefile('rb') as reader:
                        response = json.loads(reader.readline())
                time.sleep(.01)
            break
        except (OSError, ValueError) as error:
            if not readonly or attempt == 3: raise
            transport_retries.append(dict(t=now(),method=method,error=str(error)))
            time.sleep(.08)
    if method != 'ui.screenshot': calls.append(dict(t=now(), method=method, params=json.loads(json.dumps(params)), response=response))
    if 'error' in response: raise RuntimeError((method, params, response['error']))
    if method != 'ui.screenshot': time.sleep(.02)
    return response['result']

def wait():
    value = rpc('ui.wait')
    assert value['settled'], value
    if capture_errors: raise RuntimeError(capture_errors)
    return value

def command(id, **params):
    rpc('ui.command', id=id, **params)
    return wait()

def state(): return rpc('ui.state')['active']
def annotations(): return rpc('ui.annotations')
def widget(id):
    return next(w for w in rpc('ui.inspect', query=id)['widgets'] if w['id']==id)
def center(rect): return [rect[0]+rect[2]/2, rect[1]+rect[3]/2]

def move(point, duration=.45):
    start = cursor.copy()
    duration *= speed
    began = time.monotonic()
    while True:
        t = min(1., (time.monotonic()-began)/max(.001,duration))
        eased = t*t*(3-2*t)
        cursor[:] = [a+(b-a)*eased for a,b in zip(start,point)]
        events.append(dict(t=now(), kind='pointer', position=cursor.copy()))
        rpc('ui.pointer', action='move', position=cursor)
        if t>=1: break
        time.sleep(.012)

def click(id=None, point=None, settle=True):
    if id:
        w = widget(id)
        assert w['enabled'], w
        point = center(w['rect'])
    move(point)
    events.append(dict(t=now(),kind='click',position=cursor.copy()))
    rpc('ui.pointer',action='down',position=cursor)
    hold(.10)
    rpc('ui.pointer',action='up',position=cursor)
    if settle: wait()

def key(key, modifiers=None):
    rpc('ui.key',key=key,modifiers=modifiers or [])

def type_text(text, interval=.045):
    if args.reel and '\n' not in text:
        rpc('ui.type',text=text)
        wait()
        return
    for ch in text:
        if ch=='\n': key('Enter')
        else: rpc('ui.type',text=ch)
        hold(interval)
    wait()

def edit(id, text, enter=False):
    click(id)
    key('A',['command'])
    type_text(text)
    if enter: key('Enter');wait()

def drag(a,b,duration=1.1):
    move(a)
    events.append(dict(t=now(),kind='click',position=cursor.copy()))
    rpc('ui.pointer',action='down',position=cursor)
    hold(.12)
    move(b,duration)
    hold(.12)
    rpc('ui.pointer',action='up',position=cursor)
    wait()

def page_point(page,x,y):
    r = next(p['rect'] for p in state()['pages_on_screen'] if p['page']==page)
    return [r[0]+r[2]*x, r[1]+r[3]*y]

def field(name): return next(f for f in rpc('ui.forms')['fields'] if f['name']==name)
def field_click(name,left=False,settle=True):
    r=field(name)['screen_rect']
    click(point=[r[0]+(6 if left else r[2]/2),r[1]+r[3]/2],settle=settle)
def field_type(name,text):
    field_click(name)
    key('A',['command']);type_text(text)
    key('Tab');wait()

def menu(id): click('menu.file');click(id)
def zoom(id): click('menu.zoom');click(id)
def choose(sample):
    menu('demo.open');hold(.55);click('demo.'+sample)
    if rpc('ui.state')['departure'] is not None:
        hold(.7);click('document.discard')
    wait()
    command('view.zoom',zoom=.85)

def save_copy(name):
    path = Path('target')/f'showcase-{args.output.name}-{name}.pdf'
    assert not (ROOT/path).exists(), path
    menu('form.save');hold(.6)
    edit('form.save_path',str(path))
    click('form.save_confirm');hold(1.2)
    assert not state()['forms']['dirty']
    return ROOT/path

def still(name):
    if args.rehearse:
        rpc('ui.screenshot',path=str(args.output/f'{name}.png'),settled=False)
    events.append(dict(t=now(),kind='checkpoint',name=name))

def choose_border_color():
    """Use the actual HSV popup; locate its saturated hue strip in the frame."""
    opened=now()
    click('annotation.style.border_color');hold(.4)
    if args.rehearse:
        path=args.output/'color-picker.png'
        rpc('ui.screenshot',path=str(path),settled=False)
    else:
        deadline=time.monotonic()+4
        while not frames or frames[-1]['t']<opened+.2:
            assert time.monotonic()<deadline
            time.sleep(.03)
        path=args.output/frames[-1]['path']
    image=Image.open(path).convert('RGB').resize((1280,800))
    pixels=image.load();rows=[]
    for y in range(100,780):
        xs=[x for x in range(800,1280) if max(pixels[x,y])>245 and min(pixels[x,y])<12]
        if len(xs)>230 and 260<max(xs)-min(xs)<290: rows.append((y,min(xs),max(xs)))
    assert rows, 'HSV hue strip not found'
    y,x0,x1=rows[len(rows)//2];width=x1-x0
    click(point=[x0+width*.43,y])
    top=rows[0][0]-8-width
    click(point=[x0+width*.75,top+width*.35])
    hold(.6)
    # Clicking the heading closes the popover without clearing the draft.
    click(point=[1050,49])

def chapter(id,title,subtitle):
    events.append(dict(t=now(),kind='chapter',id=id,title=title,subtitle=subtitle))
    print(f'{now():6.1f}s  {title}',flush=True)
    if args.stop_after and id==args.stop_after: raise FinishedEarly

def capture_loop():
    directory=args.output/'frames';directory.mkdir()
    index=0
    while not stop.is_set():
        began=time.monotonic();filename=f'frames/{index:06}.png'
        for attempt in range(6):
            attempt_began=time.monotonic()
            try:
                result=rpc('ui.screenshot',path=str(args.output/filename),settled=False)
                frames.append(dict(t=(attempt_began+time.monotonic())/2-epoch,path=filename,revision=result['revision']))
                break
            except Exception as error:
                # A screenshot request is idempotent. Retry capture transport
                # interruptions without ever replaying a click or typed input.
                capture_retries.append(dict(t=now(),error=str(error)))
                if attempt==5:
                    capture_errors.append(str(error));return
                stop.wait(.1)
        index+=1
        stop.wait(max(0,1/args.capture_fps-(time.monotonic()-began)))

def reel_setup(action):
    """Cut file loading and scene preparation, never speed up app interactions."""
    events.append(dict(t=now(),kind='cut_start'))
    action()
    wait()
    hold(.6)
    events.append(dict(t=now(),kind='cut_end'))
    events.append(dict(t=now(),kind='pointer',position=cursor.copy(),jump=True))

def reel_open(sample, sidebar=None):
    rpc('ui.open',path=str(ASSETS/(sample+'.pdf')));wait()
    if rpc('ui.state')['departure'] is not None:
        click('document.discard');wait()
    command('view.zoom',zoom=.85)
    s=rpc('ui.state')
    # Scene setup is excluded from the reel. The actions below remain real UI input.
    if rpc('ui.thumbnails')['open']:
        click('sidebar.toggle')
    if s['active']['annotations']['open']:
        click('view.annotations')
    if sidebar: click(sidebar)

def record_layers():
    chapter('layers','PDF layers','Toggle map details without changing the document')
    hold(.8);still('layers-all')
    click('menu.layers');click('layers.0');key('Escape');wait()
    assert not state()['layers']['groups'][0]['visible']
    hold(1.2);still('layers-hidden')
    click('menu.layers');click('layers.reset');key('Escape');wait()
    assert all(g['visible'] for g in state()['layers']['groups'])
    assert not state()['forms']['dirty']
    hold(1.2);still('layers-restored')

def record_reel():
    chapter('intro','pdfoxium','Read, search, annotate and interact — powered by safe Rust')
    hold(2.8);still('launcher')

    reel_setup(lambda: reel_open('showcase-navigation','sidebar.toggle'))
    chapter('navigation','Read and navigate','Bookmarks, page previews, links and crisp zoom')
    click('view.outline');hold(.7)
    item=next(i for i in rpc('ui.outline')['items'] if i['title']=='Reading and search')
    click(point=center(item['rect']));hold(.9);still('outline')
    click('view.thumbnails')
    row=next(r for r in rpc('ui.thumbnails')['rows'] if r['page']==3)
    click(point=center(row['rect']));hold(.7)
    point=page_point(3,.5,.45);move(point)
    for _ in range(3): rpc('ui.pinch',factor=1.35,x=point[0],y=point[1]);hold(.15)
    wait();hold(1.3);still('zoom-detail')

    reel_setup(lambda: reel_open('layers-demo'))
    record_layers()

    def search_setup():
        reel_open('showcase-navigation');command('view.zoom',zoom=.75);command('view.page',page=2)
    reel_setup(search_setup)
    chapter('search','Find and select','Search from the current page; copy continuous text selections')
    click('search.open');type_text('document');hold(1.7)
    click('search.next');hold(1.)
    click('search.clear');command('view.page',page=2)
    a=page_point(2,44/612,(792-599)/792)
    b=page_point(2,(44+stringWidth('Search starts on the current page.','Helvetica',15))/612,(792-599)/792)
    drag(a,b,.8);hold(1.2);key('C',['command']);still('selection')
    assert 'Search starts' in rpc('ui.selection')['text']

    reel_setup(lambda: reel_open('editing-demo','view.annotations'))
    chapter('annotations','Annotate and save','Markup, notes, styled text and shapes — move, resize, save')
    hit=next(h for p in annotations()['visible'] for h in p['items'] if h['kind']=='FreeText')
    click(point=center(hit['screen_rects'][0]));edit('annotation.comment','Reviewed in pdfoxium.')
    edit('annotation.style.font_size','18',True);click('annotation.apply_comment');hold(1.2)
    handle=center(annotations()['move_handle']['rect'])
    drag(handle,[handle[0]+12,handle[1]+18],.7);hold(.8);still('text-edit')
    # Use the real Save-copy dialog; the output stays outside the source fixtures.
    menu('form.save')
    reel_setup(lambda: edit('form.save_path',f'target/reel-{args.output.name}.pdf'))
    click('form.save_confirm');hold(1.2);still('saved-reopened')
    assert not state()['forms']['dirty']

    reel_setup(lambda: reel_open('editing-demo'))
    chapter('pages','Edit the document','Insert images, rotate pages and import another PDF')
    def prepare_image():
        menu('edit.open');edit('edit.image_path','crates/pdfoxium-viewer/assets/demos/insert-chart.png')
    reel_setup(prepare_image);click('page.image');hold(1.2)
    menu('edit.open');click('page.rotate');hold(.8)
    def prepare_import():
        menu('edit.open');edit('edit.import_path','crates/pdfoxium-viewer/assets/demos/import-page.pdf')
    reel_setup(prepare_import);click('page.import')
    hold(1.2);still('page-import')

    reel_setup(lambda: reel_open('acroform-demo'))
    chapter('forms','Forms + JavaScript','Type, choose and check — with live currency formatting')
    field_type('name','Grace Hopper');field_type('amount','42.5')
    field_click('approved',True);hold(1.)
    field_click('delivery');hold(.7);key('ArrowDown');key('Enter');command('form.blur')
    hold(1.2);still('acroform')
    assert field('name')['value']=='Grace Hopper' and field('approved')['checked']

    reel_setup(lambda: reel_open('xfa-demo'))
    chapter('xfa','Interactive XFA','Real widgets, data binding and JavaScript actions')
    field_type('customer','Grace Hopper');field_click('approved',True)
    field_click('confirm');command('form.blur');hold(1.7);still('xfa')
    assert field('status')['value']=='Confirmed'

    reel_setup(lambda: reel_open('dynamic-xfa-demo','sidebar.toggle'))
    chapter('dynamic','Dynamic XFA','Add rows; pages and live previews follow the new layout')
    field_click('add');hold(.5);field_click('add');hold(1.)
    assert state()['page_count']==2
    row=next(r for r in rpc('ui.thumbnails')['rows'] if r['page']==2)
    click(point=center(row['rect']));hold(1.5);still('dynamic-page-two')

    reel_setup(lambda: reel_open('animation-demo','sidebar.toggle'))
    chapter('animation','PDF animation','Click to play and pause — JavaScript timers and live previews')
    field_click('animation',settle=False)
    still('animation-playing')
    for _ in range(8):
        hold(.7)
        previews=rpc('ui.thumbnails')
        assert all(row['ready'] for row in previews['rows']), 'animation preview flashed blank'
    field_click('toggle',settle=False);hold(.5);still('animation-paused');hold(1.8)

    chapter('outro','Try pdfoxium','cargo run --release -p pdfoxium-viewer')
    hold(3.)

with (args.output/'app.log').open('w') as log:
    initial=[]
    if args.start_at=='xfa': initial=[str(ASSETS/'xfa-demo.pdf')]
    app=subprocess.Popen([str(args.binary),*initial,'--control',str(endpoint_path),'--control-only-input'],
                         cwd=ROOT,stdout=log,stderr=log)
    recorder=None
    success=False
    try:
        deadline=time.monotonic()+30
        while not endpoint_path.exists():
            assert app.poll() is None and time.monotonic()<deadline
            time.sleep(.03)
        endpoint=json.loads(endpoint_path.read_text())
        wait();rpc('ui.resize',width=1280,height=800);wait()
        if initial: command('view.zoom',zoom=.85)
        # The first settled capture also brings the isolated window forward.
        rpc('ui.screenshot',path=str(args.output/'ready.png'))
        epoch=time.monotonic()
        events.append(dict(t=0,kind='pointer',position=cursor.copy()))
        if not args.rehearse:
            recorder=threading.Thread(target=capture_loop,daemon=True);recorder.start()
        if args.reel:
            record_reel()
        if include('intro'):
            chapter('intro','pdfoxium viewer','A native PDF viewer, powered by a safe Rust PDF engine')
            hold(3.5);still('launcher')
            click('demo.editing');hold(1.6)

        if include('navigation'):
            chapter('navigation','Read and navigate','Bookmarks, internal links, page labels and thumbnail navigation')
            rpc('ui.open',path=str(ASSETS/'showcase-navigation.pdf'));wait()
            command('view.zoom',zoom=.85);hold(1.4)
            click('sidebar.toggle');hold(1.2);click('view.outline');hold(1.5)
            outline=rpc('ui.outline')
            reading=next(i for i in outline['items'] if i['title']=='Reading and search')
            click(point=center(reading['rect']));hold(1.6);still('outline')
            key('ArrowLeft',['alt']);wait();hold(1.)
            click('view.thumbnails');hold(1.3)
            third=next(r for r in rpc('ui.thumbnails')['rows'] if r['page']==3)
            click(point=center(third['rect']));hold(1.4)
            click('sidebar.toggle')
            # A real pinch input at the mouse position preserves its page anchor.
            point=page_point(3,.5,.5);move(point)
            for _ in range(4): rpc('ui.pinch',factor=1.45,x=point[0],y=point[1]);hold(.3)
            wait();hold(1.5)
            rpc('ui.scroll',dx=90,dy=80);wait();hold(1.2);still('zoom-detail')
            zoom('view.fit');hold(1.)
            zoom('view.rotate');hold(1.3)
            for _ in range(3): command('view.rotate')
            zoom('view.zoom.75')
            command('view.page',page=2);hold(.8)
            rpc('ui.links',page=2);wait()
            link=next(l for l in rpc('ui.links',page=2)['links'] if l['actionable'])
            click(point=center(link['screen_rects'][0]));hold(1.2)
            assert state()['page']==1

        if include('layers'):
            choose('layers')
            record_layers()

        if include('search'):
            rpc('ui.open',path=str(ASSETS/'showcase-navigation.pdf'));wait()
            command('view.zoom',zoom=.75)
            chapter('search','Search without losing your place','Type to highlight; Enter and previous/next move between matches')
            edit('view.page','2',True);hold(.7)
            click('search.open');type_text('document');hold(2.)
            assert state()['page']==2
            click('search.next');hold(1.3);click('search.next');hold(1.1)
            click('search.previous');hold(1.2);still('search')
            click('search.clear');command('view.page',page=2)
            # Select actual page text by dragging across its advance bounds.
            phrase='Search starts on the current page.'
            a=page_point(2,44/612,(792-599)/792)
            b=page_point(2,(44+stringWidth(phrase,'Helvetica',15))/612,(792-599)/792)
            drag(a,b,1.4);hold(1.4);key('C',['command']);hold(1.2);still('selection')
            assert 'Search starts' in rpc('ui.selection')['text']
            key('Escape');wait()

        if include('annotations'):
            chapter('annotations','Annotate and edit','Text markup, notes, styled text boxes, shapes and direct manipulation')
            choose('editing');click('view.annotations');hold(1.4)
            # Existing text box: change text and size in one explicit Apply action.
            hit=next(h for p in annotations()['visible'] for h in p['items'] if h['kind']=='FreeText')
            click(point=center(hit['screen_rects'][0]));hold(1.)
            edit('annotation.comment','Edited in pdfoxium.\nText wraps inside the box.')
            edit('annotation.style.font_size','16',True)
            click('annotation.apply_comment');hold(1.5);still('text-edit')
            handle=annotations()['move_handle'];a=center(handle['rect'])
            drag(a,[a[0]-18,a[1]+16]);hold(.8)
            corner=next(h for h in annotations()['resize_handles']['corners'] if h['corner']=='BottomRight')['position']
            drag(corner,[corner[0]+20,corner[1]+12]);hold(1.)
            # Rectangle border and optional fill.
            key('Escape');wait()
            hit=next(h for p in annotations()['visible'] for h in p['items'] if h['kind']=='Square')
            click(point=center(hit['screen_rects'][0]));edit('annotation.style.border_width','3',True)
            click('annotation.style.fill_enabled');choose_border_color()
            click('annotation.apply_style');hold(1.2)
            key('Escape');wait()
            # A new free-text annotation uses the normal drawing tool.
            click('annotation.text')
            drag(page_point(1,.12,.82),page_point(1,.55,.90),1.2)
            type_text('A new text annotation.');click('annotation.apply_comment');hold(1.2)
            key('Escape');wait()
            # Note placement has its own compact composer.
            click('annotation.place_note');edit('annotation.note_text','Review this section.')
            click(point=page_point(1,.85,.37));hold(1.1)
            key('Escape');wait()
            # Create an ellipse, then delete it using the visible editor action.
            click('annotation.ellipse');drag(page_point(1,.63,.74),page_point(1,.89,.84));hold(.8)
            click('annotation.delete_selected');hold(.8)
            # Select page text, then create a continuous highlight.
            a=page_point(1,40/612,(792-603)/792);b=page_point(1,330/612,(792-603)/792)
            drag(a,b,1.2);hold(.7);click('annotation.markup.highlight');key('Escape');wait()
            hit=next(h for p in annotations()['visible'] for h in p['items'] if h['kind']=='Highlight')
            click(point=center(hit['screen_rects'][0]));edit('annotation.style.opacity','30',True)
            click('annotation.apply_style');hold(1.4);still('annotations')
            saved=save_copy('annotations')
            rpc('ui.open',path=str(saved));wait();command('view.zoom',zoom=.85)
            hold(1.2);still('saved-reopened')

        if include('pages'):
            chapter('pages','Assemble a document','Insert images, rotate and delete pages, then import another PDF')
            menu('edit.open');edit('edit.image_path','crates/pdfoxium-viewer/assets/demos/insert-chart.png')
            click('page.image');hold(1.6)
            menu('edit.open');click('page.rotate');hold(1.2)
            edit('view.page','2',True);menu('edit.open');click('page.delete');hold(1.2)
            menu('edit.open');edit('edit.import_path','crates/pdfoxium-viewer/assets/demos/import-page.pdf');click('page.import')
            hold(1.8);still('page-import')

        if include('forms'):
            chapter('forms','Fill forms with JavaScript','Interactive fields, choices, checkboxes and currency formatting')
            choose('acroform');hold(1.2)
            field_type('name','Grace Hopper');hold(.6)
            field_type('amount','42.5');hold(1.6)
            field_click('approved',True);hold(.8)
            field_click('delivery');hold(.7);key('ArrowDown');key('Enter');wait();command('form.blur')
            hold(1.4);still('acroform')
            assert field('name')['value']=='Grace Hopper' and field('approved')['checked']

        if include('xfa'):
            chapter('xfa','Interact with real XFA forms','Live controls, multiline text, data binding and JavaScript actions')
            if not (args.start_at=='xfa'): choose('xfa')
            if not rpc('ui.thumbnails')['open']: click('sidebar.toggle')
            hold(1.1)
            field_type('customer','Grace Hopper');hold(.7)
            field_type('notes','First line\nSecond line');hold(.7)
            field_click('delivery');hold(.7);key('ArrowDown');key('Enter');wait();command('form.blur')
            field_click('approved',True);hold(.6);field_click('confirm');command('form.blur')
            assert field('status')['value']=='Confirmed'
            hold(1.6);still('xfa')

        if include('dynamic'):
            chapter('dynamic','Dynamic XFA pagination','Add rows to create a page; thumbnails follow the live layout')
            choose('dynamic-xfa')
            if not rpc('ui.thumbnails')['open']: click('sidebar.toggle')
            hold(1.1)
            field_click('add');hold(.9);field_click('add');hold(1.6)
            assert state()['page_count']==2
            row=next(r for r in rpc('ui.thumbnails')['rows'] if r['page']==2)
            click(point=center(row['rect']));hold(1.7);still('dynamic-page-two')
            row=next(r for r in rpc('ui.thumbnails')['rows'] if r['page']==1)
            click(point=center(row['rect']));field_click('remove');hold(.6);field_click('remove');hold(1.2)
            assert state()['page_count']==1

        if include('animation'):
            chapter('animation','PDF animation and timers','Click the travelling wave to play; click again to pause')
            choose('animation');hold(1.1)
            field_click('animation',settle=False);hold(7.)
            field_click('toggle',settle=False);hold(1.5);still('animation-paused')

        if include('outro'):
            chapter('outro','Explore the demo','cargo run --release -p pdfoxium-viewer')
            hold(4.)
        success=True
    except FinishedEarly:
        success=True
    finally:
        stop.set()
        if recorder: recorder.join(timeout=40)
        duration=now()
        for path,digest in source_hashes.items():
            assert hashlib.sha256(path.read_bytes()).hexdigest()==digest, path
        (args.output/'timeline.json').write_text(json.dumps(dict(duration=duration,size=[1280,800],
            events=events,frames=frames,calls=calls,capture_errors=capture_errors,capture_retries=capture_retries,transport_retries=transport_retries,success=success,app_exit_before_cleanup=app.poll()),indent=2)+'\n')
        try: rpc('ui.quit')
        except Exception: pass
        try: app.wait(timeout=3)
        except subprocess.TimeoutExpired: app.terminate();app.wait(timeout=10)
    assert success and not capture_errors, capture_errors
    print(f'Walkthrough complete: {duration:.1f}s, {len(frames)} app frames, {len(calls)} control requests',flush=True)
