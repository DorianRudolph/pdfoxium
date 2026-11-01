#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Check the final MP4, visible mouse and actual animation motion (Pillow, FFmpeg)."""
import argparse
from bisect import bisect_right
import json
from pathlib import Path
import subprocess
from PIL import Image, ImageChops
from showcase_timeline import playback_timeline

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--video',type=Path,required=True)
p.add_argument('--take',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--max-duration',type=float)
a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-show_chapters','-of','json',str(a.video)]))
video=next(s for s in probe['streams'] if s['codec_type']=='video')
assert video['codec_name']=='h264' and video['pix_fmt']=='yuv420p'
assert (video['width'],video['height'],video['r_frame_rate'])==(1280,880,'24/1')
assert not any(s['codec_type']=='audio' for s in probe['streams'])
assert a.video.stat().st_size<10*1024*1024
if a.max_duration is not None:
    assert float(probe['format']['duration']) < a.max_duration
subprocess.run(['ffmpeg','-v','error','-i',str(a.video),'-f','null','-'],check=True)
d=playback_timeline(json.loads((a.take/'timeline.json').read_text()))
times=[f['t'] for f in d['frames']]
chapters=[e for e in d['events'] if e['kind']=='chapter']
assert len(probe['chapters'])==len(chapters)
assert [c['tags']['title'] for c in probe['chapters']]==[c['title'] for c in chapters]
intro_time=min(2,chapters[1]['t']/2)
intro=a.output/'intro.png'
subprocess.run(['ffmpeg','-v','error','-y','-ss',str(intro_time),'-i',str(a.video),'-frames:v','1',str(intro)],check=True)
image=Image.open(intro).convert('RGB')
pointer=list(image.crop((1178,738,1212,781)).get_flattened_data())
assert sum(max(c)<80 for c in pointer)>30, 'visible mouse outline'
assert sum(min(c)>235 for c in pointer)>30, 'visible mouse interior'

snapshots=0
for call in d['calls']:
    result=call['response'].get('result',{})
    if isinstance(result,dict) and result.get('active') is not None:
        active=result['active'];snapshots+=1
        assert result.get('error') is None and not active['errors']
        assert active['forms']['error'] is None and active['forms']['save_error'] is None
        assert active['navigation']['error'] is None
animation=next(e['t'] for e in d['events'] if e['kind']=='chapter' and e['id']=='animation')
fields=next(c['response']['result']['fields'] for c in d['calls'] if c['t']>animation and c['method']=='ui.forms')
x,y,w,h=next(f['screen_rect'] for f in fields if f['name']=='animation')
box=tuple(round(v) for v in (x+2,y+2,x+w-2,y+h-2))
play=next(e['t'] for e in d['events'] if e['t']>animation and e['kind']=='click'
          and x<e['position'][0]<x+w and y<e['position'][1]<y+h)

def app_frame(t):
    f=d['frames'][max(0,bisect_right(times,t)-1)]
    return Image.open(a.take/f['path']).convert('RGB').resize(tuple(d['size']))

# Verify that the captured layer action visibly removes and restores map ink.
layer_times={e['name']:e['t'] for e in d['events']
             if e['kind']=='checkpoint' and e['name'].startswith('layers-')}
assert set(layer_times)=={'layers-all','layers-hidden','layers-restored'}
layer_state=next(c['response']['result']['active'] for c in d['calls']
                 if c['method']=='ui.state' and c['t']>layer_times['layers-all'])
x,y,w,h=layer_state['pages_on_screen'][0]['rect']
map_box=tuple(round(v) for v in (x+2,y+2,x+w-2,y+h-2))
layer_images={name:app_frame(t-.1).crop(map_box) for name,t in layer_times.items()}
layer_diff=ImageChops.difference(layer_images['layers-all'],layer_images['layers-hidden'])
layer_changed=sum(pixel!=(0,0,0) for pixel in layer_diff.get_flattened_data())
assert layer_changed>100, 'toggling roads must visibly change the map'
assert ImageChops.difference(layer_images['layers-all'],layer_images['layers-restored']).getbbox() is None, 'reset must restore the map'

def frame(t):
    return app_frame(t).crop(box)

pause=next(e['t'] for e in d['events'] if e['kind']=='checkpoint' and e['name']=='animation-paused')
first,second=frame(play+(pause-play)*.25),frame(play+(pause-play)*.65)
changed=sum(a!=b for a,b in zip(first.get_flattened_data(),second.get_flattened_data()))
assert changed>100, 'the wave must actually animate in captured app frames'
outro=next(e['t'] for e in chapters if e['id']=='outro')
paused_a,paused_b=frame(pause+.1),frame(outro-.1)
assert ImageChops.difference(paused_a,paused_b).getbbox() is None, 'pause must stop the wave'
previews=[c['response']['result'] for c in d['calls'] if c['method']=='ui.thumbnails' and play<c['t']<pause]
assert previews and all(all(row['ready'] for row in p['rows']) for p in previews), 'preview disappeared during animation'
thumbnail_frames=0
thumbnail_motion=0
if previews[0]['open']:
    x,y,w,h=previews[0]['rows'][0]['rect']
    header=(round(x+w/2-20),round(y+8),round(x+w/2+20),round(y+16))
    for f in d['frames']:
        if not play+.25 < f['t'] < pause: continue
        image=Image.open(a.take/f['path']).convert('RGB').resize(tuple(d['size'])).crop(header)
        assert sum(r<150 and g<160 and b<200 for r,g,b in image.get_flattened_data())>80, 'thumbnail flashed white in a captured frame'
        thumbnail_frames+=1
    assert thumbnail_frames>=10
    body=(round(x+w/2-60),round(y+30),round(x+w/2+60),round(y+140))
    diff=ImageChops.difference(app_frame(play+(pause-play)*.25).crop(body),
                              app_frame(play+(pause-play)*.65).crop(body))
    thumbnail_motion=sum(pixel!=(0,0,0) for pixel in diff.get_flattened_data())
    assert thumbnail_motion>10, 'the retained preview must still receive live updates'
result=dict(duration=float(probe['format']['duration']),bytes=a.video.stat().st_size,
            size=[video['width'],video['height']],fps=24,chapters=len(chapters),layers_changed_pixels=layer_changed,decoded_frames=int(video['nb_frames']),
            mouse_visible=True,animation_changed_pixels=changed,paused_animation_changed_pixels=0,
            error_free_app_snapshots=snapshots,stable_thumbnail_samples=len(previews),
            thumbnail_frames_without_flashing=thumbnail_frames,
            animated_thumbnail_changed_pixels=thumbnail_motion)
(a.output/'validation.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
