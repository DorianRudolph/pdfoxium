#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Measure input-to-upload CPU/frame latency in the release viewer (Pillow)."""
import argparse,importlib.util,json,os,socket,subprocess,sys,time
from pathlib import Path
sys.dont_write_bytecode=True
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--binary',type=Path,default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--pdf',type=Path)
parser.add_argument('--roundtrip',action='store_true',help='Compare the UI acknowledgement/render path with the same optimized metadata handling')
parser.add_argument('--samples',type=int,default=20)
args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
pdf=args.pdf
if pdf is None:
 spec=importlib.util.spec_from_file_location('forms',root/'scripts/smoke-viewer-forms.py');forms=importlib.util.module_from_spec(spec);spec.loader.exec_module(forms)
 pdf=args.output/'forms.pdf';forms.form_pdf(pdf)
endpoint=args.output/'control.json';assert not endpoint.exists()
report=[]
with (args.output/'app.log').open('w') as log:
 app=subprocess.Popen([str(args.binary),str(pdf.resolve()),'--control',str(endpoint.resolve()),'--control-only-input'],env={**os.environ,'PDFOXIUM_VIEWER_TRACE_TYPING':'1',**({'PDFOXIUM_VIEWER_FORM_ROUNDTRIP':'1'} if args.roundtrip else {})},stdout=log,stderr=log)
 try:
  deadline=time.monotonic()+30
  while not endpoint.exists():
   assert app.poll() is None and time.monotonic()<deadline;time.sleep(.02)
  address=json.loads(endpoint.read_text())
  def call(method,params=None):
   with socket.create_connection(('127.0.0.1',address['port']),timeout=40) as conn:
    conn.sendall((json.dumps({'jsonrpc':'2.0','id':len(report),'token':address['token'],'method':method,'params':params or {}})+'\n').encode());response=json.loads(conn.makefile('rb').readline())
   report.append({'method':method,'params':params,'response':response});assert 'error' not in response,response
   return response['result']
  def command(id,**params):call('ui.command',{'id':id,**params});return call('ui.wait')
  call('ui.wait');command('view.zoom',zoom=1.5)
  fields=call('ui.forms');field=next(f for f in fields['fields'] if f['name']=='name')
  x,y,w,h=field['screen_rect'];state=call('ui.state')['active']
  call('ui.scroll',{'dx':max(0,x-state['viewport'][0]-50),'dy':max(0,y-state['viewport'][1]-100)});call('ui.wait')
  field=next(f for f in call('ui.forms')['fields'] if f['name']=='name');x,y,w,h=field['screen_rect']
  call('ui.click',{'position':[x+w/3,y+h/2]});call('ui.wait');call('ui.key',{'key':'A','modifiers':['command']});call('ui.wait')
  before=call('ui.forms');warm_count=len(before['typing_latency']['samples'])
  for i in range(args.samples):
   call('ui.type',{'text':chr(97+i%26)});call('ui.wait')
  after=call('ui.forms');samples=after['typing_latency']['samples'][warm_count:]
  assert len(samples)==args.samples and all(s['upload_frame'] is not None for s in samples),samples
  summary={'roundtrip':args.roundtrip,'samples':samples,'metadata_queries':after['metadata_queries']-before['metadata_queries'],'pipeline':call('ui.state')['active']['pipeline']}
  (args.output/'results.json').write_text(json.dumps(summary,indent=2)+'\n')
  times=sorted(s['input_to_upload_ms'] for s in samples);print(json.dumps({'median_ms':times[len(times)//2],'max_ms':max(times),'frame_delays':[s['upload_frame']-s['input_frame'] for s in samples],'metadata_queries':summary['metadata_queries']}))
  call('ui.quit');call('ui.wait');call('ui.command',{'id':'document.discard'});assert app.wait(timeout=15)==0
 finally:
  (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
  if app.poll() is None:app.terminate();app.wait(timeout=10)
