#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Measure search first-result and completion latency through the viewer control channel."""
import argparse,json,socket,statistics,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--binary',type=Path,default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--pdf',type=Path,default=root/'tests/render_corpus/books/hefferon-toc.pdf')
parser.add_argument('--page',type=int,default=200)
parser.add_argument('--query',default='function')
parser.add_argument('--samples',type=int,default=3)
args=parser.parse_args()
assert 1 <= args.samples <= 60
args.output.mkdir(parents=True,exist_ok=True)
pdf=args.pdf
endpoint=args.output/'control.json';assert not endpoint.exists()
report=[]
with (args.output/'app.log').open('w') as log:
 app=subprocess.Popen([str(args.binary),str(pdf.resolve()),'--control',str(endpoint.resolve()),'--control-only-input'],stdout=log,stderr=log)
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
  call('ui.wait')
  samples=[]
  for _ in range(args.samples):
   command('search.clear');command('view.page',page=args.page)
   page_count=call('ui.state')['active']['page_count']
   start=time.monotonic();call('ui.command',{'id':'search.find','query':args.query})
   first=None;local=None;any_page=None;movement=[]
   while True:
    result=call('ui.search',{'limit':100});state=result['state']
    elapsed=(time.monotonic()-start)*1000
    if result['hits'] and first is None:first=elapsed;any_page=result['hits'][0]['page']
    local_scan=(args.page-state.get('start_page',1))%page_count+1
    if local is None and state['scanned_pages']>=local_scan:local=elapsed
    page=call('ui.state')['active']['page']
    if not movement or movement[-1]!=page:movement.append(page)
    if not state['pending']:break
    assert time.monotonic()-start<30;time.sleep(.005)
   samples.append({'first_result_ms':first,'current_page_scanned_ms':local,'first_hit_page':any_page,'pages_visited':movement,'total_ms':elapsed,'count':state['count'],'scanned_pages':state['scanned_pages']})
  out={'pdf':str(pdf),'page':args.page,'query':args.query,'samples':samples,
       'median_first_result_ms':statistics.median(s['first_result_ms'] for s in samples),
       'median_current_page_scanned_ms':statistics.median(s['current_page_scanned_ms'] for s in samples),
       'median_total_ms':statistics.median(s['total_ms'] for s in samples)}
  (args.output/'results.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out))
  dirty=call('ui.forms')['dirty'];call('ui.quit')
  if dirty:call('ui.wait');call('ui.command',{'id':'document.discard'})
  assert app.wait(timeout=15)==0
 finally:
  (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
  if app.poll() is None:app.terminate();app.wait(timeout=10)
