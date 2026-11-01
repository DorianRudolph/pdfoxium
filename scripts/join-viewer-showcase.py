#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Join captured takes at a chapter boundary without changing playback speed."""
import argparse
import json
from pathlib import Path

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--prefix',type=Path,required=True)
parser.add_argument('--tail',type=Path,required=True)
parser.add_argument('--at',default='xfa')
parser.add_argument('--output',type=Path,required=True)
a=parser.parse_args()
prefix=json.loads((a.prefix/'timeline.json').read_text())
tail=json.loads((a.tail/'timeline.json').read_text())
cut=next(e['t'] for e in prefix['events'] if e['kind']=='chapter' and e['id']==a.at)
assert prefix['size']==tail['size'] and tail['success'] and not tail['capture_errors']
assert tail['events'] and any(e['kind']=='chapter' and e['id']==a.at for e in tail['events'])
assert max(f['t'] for f in prefix['frames'])>=cut
assert all('error' not in c['response'] for c in prefix['calls'] if c['t']<cut)
events=[e for e in prefix['events'] if e['t']<cut]+[dict(e,t=e['t']+cut) for e in tail['events']]
frames=[dict(f,path=str((a.prefix/f['path']).resolve())) for f in prefix['frames'] if f['t']<cut]
frames.extend(dict(f,t=f['t']+cut,path=str((a.tail/f['path']).resolve())) for f in tail['frames'])
calls=[c for c in prefix['calls'] if c['t']<cut]+[dict(c,t=c['t']+cut) for c in tail['calls']]
data=dict(duration=cut+tail['duration'],size=prefix['size'],events=events,frames=frames,calls=calls,
          success=True,capture_errors=[],capture_retries=tail.get('capture_retries',[]),cuts=[cut])
a.output.mkdir(parents=True,exist_ok=True)
(a.output/'timeline.json').write_text(json.dumps(data,indent=2)+'\n')
print(f'Joined at {cut:.2f}s; total {data["duration"]:.2f}s, {len(frames)} real app frames')
