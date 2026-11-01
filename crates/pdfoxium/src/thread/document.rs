// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! One owner for mutable form state, timers and progressive rendering.
use super::runtime::{self as thread, JoinHandle};
use super::{
    ticket::{lock, Completion, RunState},
    PoolBitmap, PoolSource, Priority, Source, Ticket,
};
use crate::{
    Config, Document, Error, FormEvent, FormInput, FormInputResult, FormOptions, FormSession,
    FormState, Progress, RenderConfig, RenderJob, RenderOutput, Result,
};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Condvar, Mutex};
use std::{
    collections::VecDeque,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc,
    },
};
#[cfg(target_arch = "wasm32")]
use wasm_sync::{Condvar, Mutex};
use web_time::Instant;
#[derive(Clone, Default)]
pub struct DocOptions {
    pub password: Option<String>,
    pub forms: FormOptions,
}
#[derive(Clone, Debug)]
pub enum DocEvent {
    /// Transient viewing changes, including SetOCGState actions; no PDF edit.
    OptionalContent(crate::OptionalContentState),
    Dialog {
        id: u64,
        dialog: crate::FormDialog,
    },
    Form {
        generation: u64,
        /// Committed document revision; uncommitted typing only changes generation.
        revision: u64,
        /// The input-with-paint callback owns these invalidations, including
        /// recovery if painting fails. Other event consumers can skip them.
        damage_handled: bool,
        visual: bool,
        events: Vec<FormEvent>,
        state: FormState,
    },
    Failed(String),
}
type InputCompletion =
    Box<dyn FnOnce(Result<(&Document, &FormSession, FormInputResult, u64)>) + Send>;
type Call = Box<dyn FnOnce(Result<(&Document, &FormSession)>) + Send>;
type Wake = Arc<dyn Fn() + Send + Sync>;
struct Render {
    page: usize,
    config: RenderConfig,
    priority: Priority,
    sequence: u64,
    completion: Completion<PoolBitmap>,
}
enum Message {
    Call(Call),
    Input {
        page: usize,
        input: FormInput,
        completion: InputCompletion,
        damage_handled: bool,
    },
    Render(Render),
}
struct Inbox {
    queue: Mutex<VecDeque<Message>>,
    wake: Condvar,
    pending: AtomicUsize,
    stop: AtomicBool,
}
impl Inbox {
    fn push(&self, message: Message) -> std::result::Result<(), Message> {
        let mut q = lock(&self.queue);
        if self.stop.load(Ordering::Acquire) || q.len() >= 1024 {
            return Err(message);
        }
        q.push_back(message);
        self.pending.fetch_add(1, Ordering::Release);
        self.wake.notify_one();
        Ok(())
    }
    fn drain(&self) -> Vec<Message> {
        let mut q = lock(&self.queue);
        let messages = q.drain(..).collect();
        self.pending.store(0, Ordering::Release);
        messages
    }
    fn close(&self) {
        // The stop predicate and the transition into Condvar::wait must use
        // the same mutex; otherwise shutdown can notify just before sleep.
        let _queue = lock(&self.queue);
        self.stop.store(true, Ordering::Release);
        self.wake.notify_one();
    }
    fn wait(&self, deadline: Option<Instant>) {
        let q = lock(&self.queue);
        if !q.is_empty() || self.stop.load(Ordering::Acquire) {
            return;
        }
        if let Some(deadline) = deadline {
            let _unused = self
                .wake
                .wait_timeout(q, deadline.saturating_duration_since(Instant::now()));
        } else {
            drop(self.wake.wait(q));
        }
    }
}
struct Inner {
    javascript: bool,
    xfa: Arc<AtomicBool>,
    dialogs: Arc<super::dialog::Dialogs>,
    inbox: Arc<Inbox>,
    run: Arc<RunState>,
    generation: Arc<AtomicU64>,
    preview: Arc<AtomicU64>,
    sequence: AtomicU64,
    events: Mutex<mpsc::Receiver<DocEvent>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    observer: Arc<Mutex<Option<Wake>>>,
    count: Arc<AtomicUsize>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.dialogs.close();
        self.run.shutdown.store(true, Ordering::Release);
        self.inbox.close();
        if let Some(thread) = lock(&self.thread).take() {
            thread::join(thread);
        }
    }
}
/// Cloneable thread-safe endpoint. Input/calls/saves execute once accepted even
/// if their tickets are dropped; render tickets cancel queued or paused work.
#[derive(Clone)]
pub struct DocumentThread(Arc<Inner>);
impl DocumentThread {
    pub fn spawn(source: Source, config: Config, options: DocOptions) -> Result<Self> {
        let javascript = options.forms.javascript;
        let bytes = match source {
            Source::Bytes(b) => b,
            Source::Path(path) => std::fs::read(path)?.into(),
        };
        let inbox = Arc::new(Inbox {
            queue: Default::default(),
            wake: Condvar::new(),
            pending: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
        });
        let run = Arc::new(RunState {
            shutdown: AtomicBool::new(false),
            generation: Arc::new(AtomicU64::new(1)),
            source_generation: 1,
        });
        let generation = Arc::new(AtomicU64::new(1));
        let observer = Arc::new(Mutex::new(None));
        let preview = Arc::new(AtomicU64::new(1));
        let (tx, rx) = mpsc::channel();
        let dialogs = Arc::new(super::dialog::Dialogs::default());
        let worker_dialogs = dialogs.clone();
        let (startup, ready) = mpsc::sync_channel(1);
        let state = inbox.clone();
        let epoch = generation.clone();
        let callback = observer.clone();
        let preview_epoch = preview.clone();
        let live_count = Arc::new(AtomicUsize::new(0));
        let worker_count = live_count.clone();
        let xfa = Arc::new(AtomicBool::new(false));
        let worker_xfa = xfa.clone();
        let handle = thread::spawn_named("pdfoxium-document".into(), move || {
            let start = catch_unwind(AssertUnwindSafe(|| -> Result<_> {
                config.install()?;
                let document = Document::from_bytes_with_password(
                    bytes,
                    options.password.as_deref().unwrap_or(""),
                )?;
                let count = document.page_count()?;
                let form = document.forms(options.forms)?;
                let dialog_tx = tx.clone();
                let wake = callback.clone();
                form.set_dialog_handler(move |dialog| {
                    worker_dialogs.request(dialog, |id, dialog| {
                        let _ = dialog_tx.send(DocEvent::Dialog { id, dialog });
                        notify(&wake);
                    })
                });
                Ok((document, form, count))
            }));
            match start {
                Ok(Ok((document, form, count))) => {
                    worker_xfa.store(form.is_xfa(), Ordering::Release);
                    worker_count.store(count, Ordering::Release);
                    let _ = startup.send(Ok(count));
                    run_worker(
                        document,
                        form,
                        state,
                        epoch,
                        preview_epoch,
                        tx,
                        callback,
                        worker_count,
                    );
                }
                Ok(Err(error)) => {
                    let _ = startup.send(Err(error));
                }
                Err(_) => {
                    let _ = startup.send(Err(Error::Poisoned));
                }
            }
        })?;
        let _count = match ready.recv() {
            Ok(Ok(count)) => count,
            Ok(Err(error)) => {
                let _ = handle.join();
                return Err(error);
            }
            Err(_) => {
                let _ = handle.join();
                return Err(Error::Poisoned);
            }
        };
        Ok(Self(Arc::new(Inner {
            javascript,
            xfa,
            dialogs,
            inbox,
            run,
            generation,
            preview,
            sequence: AtomicU64::new(0),
            events: Mutex::new(rx),
            thread: Mutex::new(Some(handle)),
            observer,
            count: live_count,
        })))
    }
    pub fn page_count(&self) -> usize {
        self.0.count.load(Ordering::Acquire)
    }
    pub fn is_xfa(&self) -> bool {
        self.0.xfa.load(Ordering::Acquire)
    }
    pub fn javascript_enabled(&self) -> bool {
        self.0.javascript
    }
    /// Reply directly to the host bridge; queuing a normal call would deadlock
    /// because the owning worker is suspended inside the script's dialog call.
    pub fn answer_dialog(&self, id: u64, response: crate::FormDialogResponse) -> Result<()> {
        self.0.dialogs.answer(id, response)
    }
    pub fn generation(&self) -> u64 {
        self.0.generation.load(Ordering::Acquire)
    }
    pub fn preview_generation(&self) -> u64 {
        self.0.preview.load(Ordering::Acquire)
    }
    pub fn set_observer(&self, observer: impl Fn() + Send + Sync + 'static) {
        *lock(&self.0.observer) = Some(Arc::new(observer));
    }
    pub fn take_events(&self) -> Vec<DocEvent> {
        lock(&self.0.events).try_iter().collect()
    }
    pub fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Document, &FormSession) -> Result<T> + Send + 'static,
    ) -> Ticket<T> {
        let (ticket, completion) = Completion::pair(self.0.run.clone(), None);
        let call = Message::Call(Box::new(move |context| {
            completion.finish(context.and_then(|(doc, form)| operation(doc, form)));
        }));
        if let Err(Message::Call(call)) = self.0.inbox.push(call) {
            call(Err(Error::Busy));
            notify(&self.0.observer);
        }
        ticket
    }
    pub fn input(&self, page: usize, input: FormInput) -> Ticket<FormInputResult> {
        let (ticket, completion) = Completion::pair(self.0.run.clone(), None);
        self.enqueue_input(
            page,
            input,
            false,
            Box::new(move |result| completion.finish(result.map(|(_, _, input, _)| input))),
        );
        ticket
    }
    /// Process input and its paint work in one worker turn, avoiding an owner/UI
    /// round trip. The callback owns the input's invalidations: the Form event
    /// sets `damage_handled`, and the ticket returns its render output. No page
    /// render lease survives input. Accepted operations run even if tickets drop.
    /// Keep the callback bounded: other input and caret timers wait for it.
    /// A callback error does not roll back accepted input; callers must recover
    /// its damage (for example by requesting a fresh visible render).
    pub fn input_with_paint<T: Send + 'static>(
        &self,
        page: usize,
        input: FormInput,
        paint: impl FnOnce(&Document, &FormSession, &FormInputResult, u64) -> Result<T> + Send + 'static,
    ) -> Ticket<(FormInputResult, T)> {
        let (ticket, completion) = Completion::pair(self.0.run.clone(), None);
        self.enqueue_input(
            page,
            input,
            true,
            Box::new(move |context| {
                completion.finish(context.and_then(|(doc, form, input, generation)| {
                    paint(doc, form, &input, generation).map(|output| (input, output))
                }))
            }),
        );
        ticket
    }
    fn enqueue_input(
        &self,
        page: usize,
        input: FormInput,
        damage_handled: bool,
        completion: InputCompletion,
    ) {
        if let Err(Message::Input { completion, .. }) = self.0.inbox.push(Message::Input {
            page,
            input,
            completion,
            damage_handled,
        }) {
            completion(Err(Error::Busy));
            notify(&self.0.observer);
        }
    }
    pub fn render(
        &self,
        page: usize,
        config: RenderConfig,
        priority: Priority,
    ) -> Ticket<PoolBitmap> {
        let (ticket, completion) = Completion::pair(self.0.run.clone(), None);
        let request = Render {
            page,
            config,
            priority,
            sequence: self.0.sequence.fetch_add(1, Ordering::Relaxed),
            completion,
        };
        if let Err(Message::Render(request)) = self.0.inbox.push(Message::Render(request)) {
            request.completion.finish(Err(Error::Busy));
            notify(&self.0.observer);
        }
        ticket
    }
    /// Save to a new path; never truncate an existing file, including the source.
    pub fn save(&self, path: PathBuf) -> Ticket<()> {
        self.call(move |doc, form| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            let result = form
                .document_action(crate::FormDocumentAction::WillSave)
                .and_then(|()| doc.save_to(&mut file))
                .and_then(|()| file.sync_all().map_err(Error::Io));
            drop(file);
            if result.is_err() {
                let _ = std::fs::remove_file(&path);
            }
            result?;
            form.document_action(crate::FormDocumentAction::DidSave)
        })
    }
}
fn notify(observer: &Mutex<Option<Wake>>) {
    let callback = lock(observer).clone();
    if let Some(callback) = callback {
        let _ = catch_unwind(AssertUnwindSafe(|| callback()));
    }
}
struct Publisher<'a> {
    generation: &'a AtomicU64,
    preview: &'a AtomicU64,
    tx: &'a mpsc::Sender<DocEvent>,
    last: FormState,
    layers: Option<crate::OptionalContentSnapshot>,
    revision: u64,
}
impl Publisher<'_> {
    fn publish(
        &mut self,
        doc: &Document,
        events: Vec<FormEvent>,
        state: FormState,
        force: bool,
        damage_handled: bool,
    ) -> u64 {
        let Self {
            generation,
            preview,
            tx,
            last,
            layers,
            revision: last_revision,
        } = self;
        let layer_snapshot = doc.optional_content_snapshot().ok();
        let layers_changed = *layers != layer_snapshot;
        *layers = layer_snapshot;
        if layers_changed {
            if let Ok(state) = doc.optional_content_state() {
                let _ = tx.send(DocEvent::OptionalContent(state));
            }
        }
        let state_changed = state != *last;
        let content_changed = doc.revision() != *last_revision;
        let same_field = state
            .focus
            .as_ref()
            .zip(last.focus.as_ref())
            .is_some_and(|(a, b)| a.page == b.page && a.index == b.index);
        if content_changed
            || events.iter().any(|e| matches!(e, FormEvent::Changed))
            || (same_field && state.text != last.text)
        {
            preview.fetch_add(1, Ordering::AcqRel);
        }
        let visual = layers_changed
            || content_changed
            || state.focus != last.focus
            || state.text != last.text
            || state.selected_text != last.selected_text
            || events.iter().any(|e| {
                matches!(
                    e,
                    FormEvent::Invalidate { .. }
                        | FormEvent::PagesChanged
                        | FormEvent::Changed
                        | FormEvent::FocusChanged
                        | FormEvent::TextFocus { .. }
                )
            });
        let epoch = if visual {
            generation.fetch_add(1, Ordering::AcqRel) + 1
        } else {
            generation.load(Ordering::Acquire)
        };
        *last_revision = doc.revision();
        *last = state.clone();
        if force || !events.is_empty() || state_changed || content_changed || visual {
            let _ = tx.send(DocEvent::Form {
                generation: epoch,
                revision: doc.revision(),
                damage_handled,
                visual,
                events,
                state,
            });
        }
        epoch
    }
}
fn run_worker(
    doc: Document,
    form: FormSession,
    inbox: Arc<Inbox>,
    generation: Arc<AtomicU64>,
    preview: Arc<AtomicU64>,
    tx: mpsc::Sender<DocEvent>,
    observer: Arc<Mutex<Option<Wake>>>,
    live_count: Arc<AtomicUsize>,
) {
    let mut queue: Vec<Render> = Vec::new();
    let mut resident = VecDeque::new();
    let mut active: Option<(Render, RenderJob<'static>)> = None;
    let mut publisher = Publisher {
        generation: &generation,
        preview: &preview,
        tx: &tx,
        last: FormState::default(),
        layers: doc.optional_content_snapshot().ok(),
        revision: doc.revision(),
    };
    let mut poisoned = false;
    match catch_unwind(AssertUnwindSafe(|| form.initialize())) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            poisoned = true;
            let _ = tx.send(DocEvent::Failed(error.to_string()));
        }
        Err(_) => {
            poisoned = true;
            let _ = tx.send(DocEvent::Failed(
                "document initialization panicked; reopen the PDF".into(),
            ));
        }
    }
    if !poisoned {
        if let Ok(state) = form.state() {
            publisher.publish(&doc, form.take_events(), state, false, false);
        }
    }
    notify(&observer);
    loop {
        let messages = inbox.drain();
        let timer_due = form
            .next_timer_deadline()
            .is_some_and(|d| d <= Instant::now());
        if timer_due || messages.iter().any(|m| !matches!(m, Message::Render(_))) {
            if let Some((request, job)) = active.take() {
                drop(job);
                queue.push(request);
            }
        }
        for message in messages {
            let result = catch_unwind(AssertUnwindSafe(|| match message {
                Message::Render(request) => queue.push(request),
                Message::Call(call) => {
                    if poisoned {
                        call(Err(Error::Poisoned));
                        notify(&observer);
                    } else {
                        call(Ok((&doc, &form)));
                        if let Ok(state) = form.state() {
                            publisher.publish(&doc, form.take_events(), state, false, false);
                        }
                        notify(&observer);
                    }
                }
                Message::Input {
                    page,
                    input,
                    completion,
                    damage_handled,
                } => {
                    if poisoned {
                        completion(Err(Error::Poisoned));
                        notify(&observer);
                        return;
                    }
                    let result = form.input(page, input);
                    let epoch = if let Ok(result) = &result {
                        publisher.publish(
                            &doc,
                            result.events.clone(),
                            result.state.clone(),
                            true,
                            damage_handled,
                        )
                    } else {
                        generation.load(Ordering::Acquire)
                    };
                    completion(result.map(|result| (&doc, &form, result, epoch)));
                    notify(&observer);
                }
            }));
            if result.is_err() {
                poisoned = true;
                let _ = tx.send(DocEvent::Failed(
                    "document operation panicked; reopen the PDF".into(),
                ));
                notify(&observer);
            }
        }
        if inbox.stop.load(Ordering::Acquire) {
            break;
        }
        if poisoned {
            if let Some((request, job)) = active.take() {
                drop(job);
                request.completion.finish(Err(Error::Poisoned));
            }
            for request in queue.drain(..) {
                request.completion.finish(Err(Error::Poisoned));
            }
            inbox.wait(None);
            continue;
        }
        if timer_due {
            let result = catch_unwind(AssertUnwindSafe(|| form.run_due_timers(Instant::now())));
            match result {
                Ok(Ok(events)) => {
                    if let Ok(state) = form.state() {
                        publisher.publish(&doc, events, state, false, false);
                        notify(&observer);
                    }
                }
                _ => {
                    poisoned = true;
                    let _ = tx.send(DocEvent::Failed("form timer failed; reopen the PDF".into()));
                    notify(&observer);
                }
            }
        }
        let mut cancelled = false;
        queue.retain(|request| {
            let keep = !request.completion.cancellation().is_cancelled();
            cancelled |= !keep;
            keep
        });
        if cancelled {
            notify(&observer);
        }
        if let Some((request, job)) = active.take() {
            if request.completion.cancellation().is_cancelled() {
                drop(job);
                notify(&observer);
            } else if queue.iter().any(|other| other.priority > request.priority) {
                drop(job);
                queue.push(request);
            } else {
                active = Some((request, job));
            }
        }
        if let Ok(count) = doc.page_count() {
            live_count.store(count, Ordering::Release);
        }
        if active.is_none() {
            if let Some(index) = queue
                .iter()
                .enumerate()
                .max_by_key(|(_, r)| (r.priority, std::cmp::Reverse(r.sequence)))
                .map(|(i, _)| i)
            {
                let request = queue.swap_remove(index);
                if form.is_full_xfa() {
                    let output = catch_unwind(AssertUnwindSafe(|| {
                        form.render(request.page, &request.config)
                            .map(|bitmap| PoolBitmap {
                                bitmap,
                                source: PoolSource {
                                    page_index: request.page,
                                    generation: generation.load(Ordering::Acquire),
                                },
                            })
                    }));
                    let output = output.unwrap_or_else(|_| {
                        poisoned = true;
                        let _ = tx.send(DocEvent::Failed(
                            "XFA drawing panicked; reopen the PDF".into(),
                        ));
                        Err(Error::Poisoned)
                    });
                    request.completion.finish(output);
                    notify(&observer);
                    continue;
                }
                resident.retain(|page| *page != request.page);
                resident.push_back(request.page);
                let result = (|| {
                    // SDK form views retain field state separately from parsed page graphs.
                    while resident.len() > 8 {
                        doc.page(resident.pop_front().unwrap())?.unload()?;
                    }
                    doc.page(request.page)?.render_job(&request.config)
                })();
                match result {
                    Ok(job) => active = Some((request, job)),
                    Err(error) => {
                        request.completion.finish(Err(error));
                        notify(&observer);
                    }
                }
            }
        }
        if let Some((request, mut job)) = active.take() {
            let cancellation = request.completion.cancellation();
            let deadline = form.next_timer_deadline();
            let result = catch_unwind(AssertUnwindSafe(|| {
                job.resume(&mut || {
                    inbox.pending.load(Ordering::Acquire) > 0
                        || cancellation.is_cancelled()
                        || deadline.is_some_and(|d| d <= Instant::now())
                })
            }));
            match result {
                Ok(Ok(Progress::Paused)) => active = Some((request, job)),
                Ok(Ok(Progress::Done)) => {
                    let output = catch_unwind(AssertUnwindSafe(|| {
                        job.finish().and_then(|output| match output {
                            RenderOutput::Owned(mut bitmap) => {
                                form.draw(request.page, &mut bitmap, &request.config)?;
                                Ok(PoolBitmap {
                                    bitmap,
                                    source: PoolSource {
                                        page_index: request.page,
                                        generation: generation.load(Ordering::Acquire),
                                    },
                                })
                            }
                            _ => Err(Error::Internal(
                                "owned form render returned borrowed output",
                            )),
                        })
                    }));
                    let output = match output {
                        Ok(output) => output,
                        Err(_) => {
                            poisoned = true;
                            let _ = tx.send(DocEvent::Failed(
                                "form drawing panicked; reopen the PDF".into(),
                            ));
                            Err(Error::Poisoned)
                        }
                    };
                    request.completion.finish(output);
                    notify(&observer);
                }
                Ok(Err(error)) => {
                    drop(job);
                    request.completion.finish(Err(error));
                    notify(&observer);
                }
                Err(_) => {
                    drop(job);
                    request.completion.finish(Err(Error::Poisoned));
                    poisoned = true;
                    let _ = tx.send(DocEvent::Failed(
                        "document rendering panicked; reopen the PDF".into(),
                    ));
                    notify(&observer);
                }
            }
        } else if queue.is_empty() {
            inbox.wait(form.next_timer_deadline());
        }
    }
    // Accepted calls/input were drained above; only read-only work is discarded.
    drop(active);
    drop(queue);
    if !poisoned {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            form.document_action(crate::FormDocumentAction::WillClose)
        }));
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    #[test]
    fn shutdown_cannot_notify_between_the_predicate_check_and_sleep() {
        let inbox = Arc::new(Inbox {
            queue: Default::default(),
            wake: Condvar::new(),
            pending: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
        });
        let guard = lock(&inbox.queue);
        assert!(!inbox.stop.load(Ordering::Acquire));
        let (started, ready) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let closer = inbox.clone();
        let thread = thread::spawn(move || {
            started.send(()).unwrap();
            closer.close();
            done.send(()).unwrap();
        });
        ready.recv().unwrap();
        assert!(finished
            .recv_timeout(std::time::Duration::from_millis(20))
            .is_err());
        let (_guard, timeout) = inbox
            .wake
            .wait_timeout(guard, std::time::Duration::from_secs(2))
            .unwrap();
        assert!(!timeout.timed_out());
        assert!(inbox.stop.load(Ordering::Acquire));
        finished
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        thread.join().unwrap();
    }
}
