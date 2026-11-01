// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Playwright CLI: run-code --filename scripts/smoke-viewer-wasm.js
// Requires a viewer built with --features web-test, served on localhost:8080,
// in a fresh headed Chrome tab without viewport emulation.
async (page) => {
  if (page.viewportSize() !== null) {
    throw new Error('Run in a fresh headed Chrome tab without viewport emulation');
  }
  const cdp = await page.context().newCDPSession(page);
  const {windowId, bounds} = await cdp.send('Browser.getWindowForTarget');
  const frame = await page.evaluate(() => ({width:outerWidth-innerWidth,height:outerHeight-innerHeight}));
  const resizeWindow = async (width,height) => {
    await cdp.send('Browser.setWindowBounds', {windowId,bounds:{width:width+frame.width,height:height+frame.height}});
    await page.waitForFunction(([w,h]) => innerWidth===w && innerHeight===h, [width,height]);
    await page.waitForFunction(() => {
      const canvas=document.querySelector('#viewer');
      return !canvas || (canvas.width===Math.round(innerWidth*devicePixelRatio) && canvas.height===Math.round(innerHeight*devicePixelRatio));
    });
  };
  try {
  const output = 'output/playwright/wasm';
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const errors = [];
  page.context().on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', error => errors.push(String(error)));
  await resizeWindow(1100,850);
  await page.goto('http://127.0.0.1:8080/');
  await page.waitForFunction(() => document.querySelector('#viewer')?.dataset.ready === 'true');
  await page.evaluate(async () => {
    const api = await import('./pdfoxium_viewer.js');
    window.viewerSmoke = {api, call: async (method, params={}) => {
      const id = api.test_request(method, JSON.stringify(params));
      const deadline = performance.now() + 45000;
      while (performance.now() < deadline) {
        const raw = api.test_reply(id);
        if (raw) {
          const reply = JSON.parse(raw);
          if (reply.error) throw new Error(`${method}: ${reply.error}`);
          return reply.result;
        }
        await new Promise(resolve => setTimeout(resolve, 15));
      }
      throw new Error(`Timeout: ${method}`);
    }};
  });
  const call = (method, params={}) => page.evaluate(([m,p]) => window.viewerSmoke.call(m,p), [method,params]);
  const wait = () => call('ui.wait');
  const command = async (id, params={}) => { await call('ui.command',{id,...params}); await wait(); };
  const state = () => call('ui.state');
  const active = async () => (await state()).active;
  const widget = async id => {
    const widgets = (await call('ui.inspect')).widgets;
    const found = widgets.find(w => w.id === id);
    assert(found?.enabled, `Missing/disabled widget: ${id}`);
    return found.rect;
  };
  const clickWidget = async id => {
    const [x,y,w,h] = await widget(id);
    await page.mouse.click(x+w/2,y+h/2);
    await wait();
  };
  const choose = async sample => {
    await command('demo.open');
    const [x,y,w,h]=await widget(`demo.${sample}`);
    await page.mouse.click(x+w/2,y+h/2);
    // Dirty sample switches use the same departure confirmation as uploads.
    await page.waitForTimeout(75);
    if ((await state()).departure) await call('ui.command',{id:'document.discard'});
    await wait();
    assert((await active()).errors.length === 0, `Render error in ${sample}`);
    await command('view.zoom',{zoom:0.8});
  };
  const field = async name => {
    const found=(await call('ui.forms')).fields.find(f=>f.name===name);
    assert(found, `Missing form field ${name}`);
    return found;
  };
  const clickField = async name => {
    const target = await field(name);
    const [x,y,w,h] = target.screen_rect;
    await page.mouse.click(x+w/2,y+h/2);
    await wait();
    if (target.kind === "PushButton") await page.waitForTimeout(350);
  };
  const typeField = async (name,text) => {
    await clickField(name);
    await page.keyboard.press('Meta+A');
    await page.keyboard.insertText(text);
    await wait();
    await command('form.blur');
    assert((await field(name)).value === text, `${name} edit did not commit`);
  };
  // CLI modal interception is verified separately with File → Open + upload.
  // Direct file input selection keeps the full automated walkthrough running.
  const upload = async (path, purpose='open') => {
    await page.locator('#open-pdf').evaluate((input,purpose) => input.setAttribute('data-purpose',purpose), purpose);
    await page.locator('#open-pdf').setInputFiles(path);
    await page.waitForTimeout(75);
    await wait();
  };
  const drop = async path => {
    // Populate a real File via the browser file input, then dispatch the same
    // DataTransfer events the canvas receives from a desktop file drop.
    // Avoid its change listener: use a separate temporary input for the fixture.
    await page.evaluate(() => {
      const input=document.createElement('input'); input.type='file'; input.id='drop-fixture'; input.hidden=true; document.body.append(input);
    });
    await page.locator('#drop-fixture').setInputFiles(path);
    await page.evaluate(() => {
      const input=document.querySelector('#drop-fixture');
      const transfer=new DataTransfer(); transfer.items.add(input.files[0]);
      const canvas=document.querySelector('#viewer');
      canvas.dispatchEvent(new DragEvent('dragover',{dataTransfer:transfer,bubbles:true,cancelable:true}));
      canvas.dispatchEvent(new DragEvent('drop',{dataTransfer:transfer,bubbles:true,cancelable:true}));
      input.remove();
    });
    await page.waitForTimeout(75);
    await wait();
  };
  const save = async name => {
    const downloadEvent=page.waitForEvent('download');
    await call('ui.command',{id:'form.save',path:name});
    const download=await downloadEvent;
    assert(download.suggestedFilename()===name, 'Download filename changed');
    const path=`${output}/${name}`;
    await download.saveAs(path);
    await wait();
    assert(!(await active()).forms.dirty, 'Save left dirty state');
    return path;
  };
  const report = {samples:[], checks:[]};
  assert(await page.evaluate(() => crossOriginIsolated && typeof SharedArrayBuffer==='function'), 'SharedArrayBuffer unavailable');
  assert(await page.evaluate(() => window.viewerSmoke.api.test_threads()) === 42, 'Thread probe failed');
  report.checks.push('nested workers, condition variable, channels, TLS and repeated joins');
  for (const sample of ['layers','editing','acroform','xfa','dynamic-xfa','animation']) {
    await choose(sample);
    const view=await active();
    assert(view.cached_tiles>0, `${sample} did not render`);
    report.samples.push({sample,pages:view.page_count,fields:view.forms.fields.length});
    await page.screenshot({path:`${output}/${sample}.png`});
  }
  await choose('layers');
  assert((await active()).layers.groups.length===4, 'Layer sample did not load groups');
  const mapClip = async () => {
    const [x,y,w,h] = (await active()).pages_on_screen.find(p=>p.page===1).rect;
    return {x:x+w*.07,y:y+h*.28,width:w*.86,height:h*.48};
  };
  await command('view.thumbnails');
  const thumbRow=(await active()).thumbnails.rows.find(r=>r.page===1);
  assert(thumbRow?.ready && thumbRow.source==='rendered', 'Layered document used a stale embedded thumbnail');
  const [tx,ty,tw,th]=thumbRow.rect;
  const thumbnailClip={x:tx,y:ty,width:tw,height:th};
  const originalThumbnail=await page.screenshot({clip:thumbnailClip});
  const layerMapClip = await mapClip();
  const allLayers = await page.screenshot({clip:layerMapClip,path:`${output}/layers-all.png`});
  await clickWidget('menu.layers');
  const locked=(await call('ui.inspect')).widgets.find(w=>w.id==='layers.3');
  assert(locked && !locked.enabled, 'Locked layer is interactive');
  await page.screenshot({path:`${output}/layers-menu.png`});
  await clickWidget('layers.0');
  await page.keyboard.press('Escape'); await wait();
  assert(!(await active()).layers.groups[0].visible, 'Mouse toggle did not hide roads');
  const noRoads=await page.screenshot({clip:layerMapClip,path:`${output}/layers-no-roads.png`});
  assert(!allLayers.equals(noRoads), 'Layer toggle did not change visible map pixels');
  assert(!originalThumbnail.equals(await page.screenshot({clip:thumbnailClip})), 'Layer toggle did not refresh thumbnail pixels');
  await command('page.next');
  assert(!(await active()).layers.groups[0].visible, 'Layer state was lost on next page');
  await command('page.previous');
  await clickWidget('menu.layers'); await clickWidget('layers.configuration.1');
  await page.keyboard.press('Escape'); await wait();
  assert(JSON.stringify((await active()).layers.groups.map(g=>g.visible))==='[false,true,false,true]', 'Configuration did not apply');
  await page.screenshot({path:`${output}/layers-buildings.png`});
  await clickWidget('menu.layers'); await clickWidget('layers.reset');
  await page.keyboard.press('Escape'); await wait();
  assert((await active()).layers.groups.every(g=>g.visible), 'Layer reset failed');
  const reset=await page.screenshot({clip:await mapClip()});
  assert(reset.equals(allLayers), 'Reset did not restore the original map pixels');
  await command('navigation.link',{page:1,index:0});
  assert(!(await active()).layers.groups[0].visible, 'SetOCGState link did not toggle roads');
  assert(!(await active()).forms.dirty, 'Transient layer change dirtied the PDF');
  await command('edit.open');
  await command('layers.set',{index:1,visible:false});
  assert(!(await active()).layers.groups[1].visible, 'Editor worker lost layer state');
  await command('layers.reset');
  assert((await active()).layers.groups.every(g=>g.visible), 'Editor worker reset failed');
  await command('view.thumbnails');
  report.checks.push('layers: mouse visibility, map and thumbnail pixels, locks, both pages, configurations, reset, PDF actions and editor worker');
  await choose('acroform');
  report.acroformFields=(await call('ui.forms')).fields.map(f=>f.name);
  await typeField('name','WASM customer');
  const saved=await save('wasm-acroform.pdf');
  await upload(saved);
  assert((await field('name')).value==='WASM customer', 'AcroForm save/reopen lost the value');
  report.checks.push('AcroForm typing, download, browser upload, saved value reopened');
  await choose('xfa');
  await typeField('customer','WASM XFA customer');
  const xfaSaved=await save('wasm-xfa.pdf');
  await upload(xfaSaved);
  assert((await field('customer')).value==='WASM XFA customer', 'XFA save/reopen lost the value');
  report.checks.push('XFA typing, download and save/reopen');
  await choose('dynamic-xfa');
  const before=await page.screenshot();
  await clickField('add'); await clickField('add');
  assert((await active()).page_count===2, 'Dynamic XFA did not grow');
  const grown=await page.screenshot({path:`output/playwright/wasm/dynamic-grown.png`});
  assert(!before.equals(grown), 'Dynamic growth produced no visible change');
  await clickField('remove'); await clickField('remove');
  assert((await active()).page_count===1, 'Dynamic XFA did not shrink');
  report.checks.push('dynamic XFA visible growth and shrink');
  await choose('animation');
  const [ax,ay,aw,ah]=(await field('animation')).screen_rect;
  const clip={x:Math.ceil(ax+3),y:Math.ceil(ay+3),width:Math.floor(aw-6),height:Math.floor(ah-6)};
  const initial=await page.screenshot({clip});
  await clickField('animation');
  await page.waitForTimeout(350);
  await clickField('toggle');
  const moving=await page.screenshot({clip,path:`${output}/animation-paused.png`});
  assert(!initial.equals(moving), 'Animation produced no visible change');
  await page.waitForTimeout(300);
  assert(moving.equals(await page.screenshot({clip})), 'Paused animation kept changing');
  report.checks.push('animation timers, visible frame change and pause');
  await choose('editing');
  await clickWidget('view.annotations');
  assert((await call('ui.annotations')).items.length===7, 'Seeded annotations missing');
  await command('view.annotations');
  await upload('crates/pdfoxium-viewer/assets/demos/import-page.pdf','page.import');
  assert((await active()).page_count===4, 'Uploaded PDF pages were not imported');
  const importedBefore=await page.screenshot();
  await upload('crates/pdfoxium-viewer/assets/demos/insert-chart.png','page.image');
  const inserted=await page.screenshot({path:`${output}/inserted-image.png`});
  assert(!importedBefore.equals(inserted), 'PNG insertion produced no visible change');
  report.checks.push('seeded annotations, byte-backed PDF import and visible PNG insertion');
  await command('page.rotate');
  // A drop must preserve the old document until the user chooses Discard.
  await drop('crates/pdfoxium-viewer/assets/demos/import-page.pdf');
  assert((await state()).departure?.kind==='open', 'Dirty drop bypassed unsaved confirmation');
  await command('document.cancel');
  assert((await active()).path==='editing-demo.pdf', 'Cancel replaced the old document');
  await drop('crates/pdfoxium-viewer/assets/demos/import-page.pdf');
  await command('document.discard');
  assert((await active()).path==='import-page.pdf', 'Drop did not open the uploaded PDF');
  report.checks.push('drag/drop, unsaved Cancel and Discard');
  await upload('crates/pdfoxium-viewer/assets/demos/showcase-navigation.pdf');
  assert((await active()).page_count>1, 'File dialog did not open the navigation PDF');
  assert((await active()).workers===2, 'Static document did not use the two-worker render pool');
  await command('view.page',{page:2});
  await command('search.find',{query:'PDF'});
  assert((await active()).search.scanned_pages>0, 'Reader worker did not search');
  report.checks.push('browser upload, two-worker static rendering, navigation and reader search');
  const oldPath=(await active()).path;
  await page.locator('#open-pdf').setInputFiles({name:'broken.pdf',mimeType:'application/pdf',buffer:Buffer.from('not a PDF')});
  let failed;
  for(let i=0;i<100;i++) {
    failed=await state();
    if (failed.error) break;
    await page.waitForTimeout(20);
  }
  assert(failed.error && failed.active.path===oldPath, 'Invalid PDF replaced the working document or hid its error');
  await upload('crates/pdfoxium-viewer/assets/demos/showcase-navigation.pdf');
  report.checks.push('invalid PDF error preserves the previous document and permits recovery');
  await resizeWindow(640,480); await wait();
  await page.screenshot({path:`${output}/narrow.png`});
  const layout=await page.evaluate(()=>({canvas:document.querySelector('#viewer').getBoundingClientRect().toJSON(),width:innerWidth,height:innerHeight,scrollWidth:document.documentElement.scrollWidth}));
  assert(layout.canvas.width===layout.width && layout.canvas.height===layout.height && layout.scrollWidth===layout.width, 'Viewer does not fill viewport');
  assert(errors.length===0, `Browser errors: ${errors.join('\n')}`);
  report.checks.push('viewer-only full viewport, narrow resize, no console errors');
  await page.waitForTimeout(100);
  report.liveWorkers=page.workers().length;
  assert(report.liveWorkers===4, `Worker leak after document switches: ${report.liveWorkers}`);
  return report;
  } finally {
    // Resize the actual window throughout: setViewportSize leaves page-level
    // emulation behind that CDP cleanup cannot reliably clear for later tools.
    try {
      await cdp.send('Browser.setWindowBounds', {windowId,bounds});
    } finally {
      await cdp.detach();
    }
  }
}
