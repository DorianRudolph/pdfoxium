#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Encode captured app frames at real-time speed, with captions and a mouse (Pillow, FFmpeg)."""
import argparse
from bisect import bisect_right
import json
import math
from pathlib import Path
import subprocess
from PIL import Image, ImageDraw, ImageFont
from showcase_timeline import playback_timeline

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--input',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--fps',type=int,default=24)
parser.add_argument('--max-duration',type=float,help='Reject a reel at or above this many seconds')
args=parser.parse_args()
if not 1 <= args.fps <= 60: parser.error('--fps must be 1..60')
data=playback_timeline(json.loads((args.input/'timeline.json').read_text()))
assert data['success'] and data['frames'] and not data['capture_errors']
if args.max_duration is not None:
    assert math.ceil(data['duration']*args.fps)/args.fps < args.max_duration, data['duration']
(args.input/'playback-timeline.json').write_text(json.dumps(data,indent=2)+'\n')
args.output.parent.mkdir(parents=True,exist_ok=True)
width,height=data['size'];footer=80
size=(width,height+footer)
frames=data['frames'];frame_times=[f['t'] for f in frames]
pointers=[e for e in data['events'] if e['kind']=='pointer'];pointer_times=[e['t'] for e in pointers]
chapters=[e for e in data['events'] if e['kind']=='chapter'];chapter_times=[e['t'] for e in chapters]
clicks=[e for e in data['events'] if e['kind']=='click']
duration=data['duration']
assert chapters[-1]['id']=='outro', 'The take must include its ending'

def font(size,bold=False):
    candidates=[Path('/System/Library/Fonts/Supplemental')/('Arial Bold.ttf' if bold else 'Arial.ttf'),
                Path('/usr/share/fonts/truetype/dejavu')/('DejaVuSans-Bold.ttf' if bold else 'DejaVuSans.ttf')]
    for p in candidates:
        if p.exists(): return ImageFont.truetype(str(p),size)
    return ImageFont.load_default(size=size)

title_font,body_font,brand_font=font(23,True),font(16),font(16,True)
cursor=Image.new('RGBA',(66,84));draw=ImageDraw.Draw(cursor)
points=[(3,3),(3,62),(19,47),(33,77),(45,71),(31,43),(55,43)]
draw.polygon(points,fill='white',outline=(20,35,30),width=5)
cursor=cursor.resize((26,34),Image.Resampling.LANCZOS)

def compose(t,app):
    frame=Image.new('RGB',size,(26,49,42));frame.paste(app,(0,0))
    chapter=chapters[max(0,bisect_right(chapter_times,t)-1)]
    d=ImageDraw.Draw(frame)
    d.text((24,height+10),chapter['title'],font=title_font,fill=(247,245,238))
    d.text((24,height+43),chapter['subtitle'],font=body_font,fill=(198,219,205))
    d.text((width-100,height+14),'pdfoxium',font=brand_font,fill=(198,219,205))
    d.rectangle((0,size[1]-3,width*t/duration,size[1]),fill=(83,145,120))
    i=max(0,bisect_right(pointer_times,t)-1)
    p=pointers[i]['position']
    if i+1<len(pointers) and not pointers[i+1].get('jump',False):
        a,b=pointers[i],pointers[i+1]
        fraction=max(0,min(1,(t-a['t'])/max(.0001,b['t']-a['t'])))
        p=[x+(y-x)*fraction for x,y in zip(a['position'],b['position'])]
    for click in clicks:
        age=t-click['t']
        if 0<=age<.32:
            overlay=Image.new('RGBA',size)
            od=ImageDraw.Draw(overlay)
            radius=10+12*age/.32;x,y=click['position']
            od.ellipse((x-radius,y-radius,x+radius,y+radius),outline=(203,150,57,round(185*(1-age/.32))),width=2)
            frame.paste(overlay,(0,0),overlay)
    frame.paste(cursor,(round(p[0])-1,round(p[1])-1),cursor)
    return frame

command=['ffmpeg','-hide_banner','-loglevel','warning','-y','-f','rawvideo','-pixel_format','rgb24',
         '-video_size',f'{size[0]}x{size[1]}','-framerate',str(args.fps),'-i','pipe:0',
         '-an','-c:v','libx264','-preset','slow','-crf','21','-threads','4','-pix_fmt','yuv420p',
         '-movflags','+faststart',str(args.output)]
process=subprocess.Popen(command,stdin=subprocess.PIPE)
last=-1;app=None
try:
    for index in range(math.ceil(duration*args.fps)):
        t=index/args.fps
        source=max(0,bisect_right(frame_times,t)-1)
        if source!=last:
            app=Image.open(args.input/frames[source]['path']).convert('RGB')
            if app.size!=(width,height):app=app.resize((width,height),Image.Resampling.LANCZOS)
            last=source
        frame=compose(t,app)
        process.stdin.write(frame.tobytes())
        if index%(args.fps*20)==0: print(f'Encoded {t:.0f}/{duration:.0f} seconds',flush=True)
finally:
    process.stdin.close()
assert process.wait()==0

# Keep chapter markers in the MP4 too, so players can jump between feature groups.
metadata=[';FFMETADATA1']
for i,chapter in enumerate(chapters):
    end=chapters[i+1]['t'] if i+1<len(chapters) else duration
    metadata.extend(['[CHAPTER]','TIMEBASE=1/1000',f'START={round(chapter["t"]*1000)}',
                     f'END={round(end*1000)}',f'title={chapter["title"]}'])
metadata_path=args.input/'chapters.ffmetadata'
metadata_path.write_text('\n'.join(metadata)+'\n')
chaptered=args.output.with_name(args.output.stem+'-chaptered.mp4')
subprocess.run(['ffmpeg','-v','error','-y','-i',str(args.output),'-i',str(metadata_path),
                '-map','0:v:0','-map_metadata','1','-map_chapters','1','-c','copy',
                '-movflags','+faststart',str(chaptered)],check=True)
chaptered.replace(args.output)

# A real app frame with a quiet pointer and caption serves as the README poster.
checkpoint=next(e for e in data['events'] if e['kind']=='checkpoint' and e['name']=='outline')
t=checkpoint['t']
source=max(0,bisect_right(frame_times,t)-1)
app=Image.open(args.input/frames[source]['path']).convert('RGB').resize((width,height),Image.Resampling.LANCZOS)
poster=compose(t,app)
overlay=Image.new('RGBA',size);d=ImageDraw.Draw(overlay)
cx,cy=width/2,height/2
d.ellipse((cx-34,cy-34,cx+34,cy+34),fill=(26,49,42,230),outline=(247,245,238,230),width=2)
d.polygon([(cx-8,cy-15),(cx-8,cy+15),(cx+17,cy)],fill=(247,245,238,255))
poster.paste(overlay,(0,0),overlay)
poster.save(args.output.with_suffix('.png'),optimize=True)

def stamp(seconds):
    milliseconds=round(seconds*1000);s,ms=divmod(milliseconds,1000);m,s=divmod(s,60);h,m=divmod(m,60)
    return f'{h:02}:{m:02}:{s:02}.{ms:03}'
vtt=['WEBVTT','']
for i,chapter in enumerate(chapters):
    end=chapters[i+1]['t'] if i+1<len(chapters) else duration
    vtt.extend([f'{stamp(chapter["t"])} --> {stamp(end)}',chapter['title'],chapter['subtitle'],''])
args.output.with_suffix('.vtt').write_text('\n'.join(vtt))
summary=dict(duration_seconds=duration,size=size,fps=args.fps,source_frames=len(frames),
             actual_capture_fps=len(frames)/duration,bytes=args.output.stat().st_size,
             setup_cut_seconds=data['setup_cut_seconds'],
             chapters=[dict(time=c['t'],title=c['title']) for c in chapters])
args.output.with_suffix('.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
