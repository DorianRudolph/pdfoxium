// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::runtime::{self as thread, JoinHandle};
use super::ticket::{lock, Cancellation, Completion, RunState};
use super::{CancelGroup, Ticket};
use crate::{
    Bitmap, Config, Document, Error, Progress, RenderConfig, RenderJob, RenderOutput, Result,
};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Condvar, Mutex};
use std::{
    cmp::Reverse,
    collections::VecDeque,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
};
#[cfg(target_arch = "wasm32")]
use wasm_sync::{Condvar, Mutex};

/// Immutable saved content. A path is read once before starting workers, so
/// every worker sees the same snapshot even if the file is subsequently edited.
#[derive(Clone)]
pub enum Source {
    Bytes(Arc<[u8]>),
    Path(PathBuf),
}
impl Source {
    fn bytes(self) -> Result<Arc<[u8]>> {
        match self {
            Self::Bytes(bytes) => Ok(bytes),
            Self::Path(path) => Ok(std::fs::read(path)?.into()),
        }
    }
}

/// Higher values preempt lower ones at the renderer's existing pause points.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Priority {
    Background,
    Thumbnail,
    Prefetch,
    Visible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PoolSource {
    pub generation: u64,
    pub page_index: usize,
}

#[derive(Debug)]
pub struct PoolBitmap {
    pub bitmap: Bitmap,
    pub source: PoolSource,
}

/// Resource bounds for each worker, excluding delivered bitmaps held by callers.
#[derive(Clone)]
pub struct PoolOptions {
    pub threads: usize,
    pub password: Option<String>,
    /// A full queue rejects new requests with `Error::Busy`.
    pub max_pending_jobs: usize,
    /// Overflow drops the lowest-priority paused context and restarts its request
    /// later. If its queue is also full, that request is cancelled.
    pub max_paused_jobs: usize,
    /// Parsed pages/cache residency. Must exceed `max_paused_jobs` so a new page
    /// can render while other contexts are paused. Idle pages are evicted by LRU.
    pub max_cached_pages: usize,
}
impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            threads: 1,
            password: None,
            max_pending_jobs: 256,
            max_paused_jobs: 2,
            max_cached_pages: 8,
        }
    }
}

struct Request {
    page: usize,
    config: RenderConfig,
    priority: Priority,
    sequence: u64,
    completion: Completion<PoolBitmap>,
    cancellation: Arc<Cancellation>,
}
impl Request {
    fn key(&self) -> (Priority, Reverse<u64>) {
        (self.priority, Reverse(self.sequence))
    }
    fn supersedes(&self, old: &Self) -> bool {
        self.sequence > old.sequence
            && self.page == old.page
            && self.priority >= old.priority
            && self.config.covers(&old.config)
    }
}

#[derive(Default)]
struct QueueState {
    pending: Vec<Request>,
    closed: bool,
    next_sequence: u64,
}
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Started(usize, u64),
    Resumed(usize, u64),
    Paused(usize, u64),
    Finished(usize, u64),
    Evicted(usize),
}
struct Queue {
    state: Mutex<QueueState>,
    available: Condvar,
    #[cfg(test)]
    observer: Mutex<Option<Arc<dyn Fn(Event) + Send + Sync>>>,
}
impl Queue {
    fn new() -> Self {
        Self {
            state: Mutex::new(QueueState::default()),
            available: Condvar::new(),
            #[cfg(test)]
            observer: Mutex::new(None),
        }
    }
    #[cfg(test)]
    fn event(&self, event: Event) {
        let observer = lock(&self.observer).clone();
        if let Some(observer) = observer {
            observer(event);
        }
    }
    fn close(&self) {
        let pending = {
            let mut state = lock(&self.state);
            state.closed = true;
            std::mem::take(&mut state.pending)
        };
        self.available.notify_all();
        // Completion (and user wakers) must always happen outside queue locks.
        drop(pending);
    }
    fn higher_than(&self, priority: Priority) -> bool {
        lock(&self.state)
            .pending
            .iter()
            .any(|request| request.priority > priority && !request.cancellation.is_cancelled())
    }
    fn requeue(&self, request: Request, capacity: usize) {
        let mut state = lock(&self.state);
        if !state.closed && state.pending.len() < capacity && !request.cancellation.is_cancelled() {
            state.pending.push(request);
        } else {
            drop(state);
            request.completion.finish(Err(Error::Cancelled));
        }
    }
    fn discard_where(&self, mut discard: impl FnMut(&Request) -> bool) {
        let removed = {
            let mut state = lock(&self.state);
            let mut removed = Vec::new();
            let mut i = 0;
            while i < state.pending.len() {
                if discard(&state.pending[i]) {
                    removed.push(state.pending.swap_remove(i));
                } else {
                    i += 1;
                }
            }
            removed
        };
        for request in removed {
            request.completion.finish(Err(Error::Cancelled));
        }
    }
    fn discard_covered(&self, request: &Request) {
        self.discard_where(|old| request.supersedes(old));
    }
}

struct WorkerSet {
    run: Arc<RunState>,
    queues: Vec<Arc<Queue>>,
    handles: Vec<JoinHandle<()>>,
    page_count: usize,
    optional_content: Option<crate::OptionalContentState>,
}
impl Drop for WorkerSet {
    fn drop(&mut self) {
        self.run.shutdown.store(true, Ordering::Release);
        for queue in &self.queues {
            queue.close();
        }
        for handle in self.handles.drain(..) {
            // Custom wakers can drop their pool from a worker. Never self-join.
            thread::join(handle);
        }
    }
}

impl WorkerSet {
    fn start(
        bytes: Arc<[u8]>,
        config: &Config,
        options: &PoolOptions,
        generation: Arc<AtomicU64>,
        source_generation: u64,
    ) -> Result<Self> {
        let run = Arc::new(RunState {
            shutdown: AtomicBool::new(false),
            generation,
            source_generation,
        });
        let mut workers = Self {
            run,
            queues: Vec::new(),
            handles: Vec::new(),
            page_count: 0,
            optional_content: None,
        };
        let mut startup_results = Vec::new();
        for index in 0..options.threads {
            let queue = Arc::new(Queue::new());
            let worker_queue = queue.clone();
            let run = workers.run.clone();
            let bytes = bytes.clone();
            let config = config.clone();
            let options = options.clone();
            let (startup, ready) =
                mpsc::sync_channel::<Result<(usize, crate::OptionalContentState)>>(1);
            let handle = thread::spawn_named(format!("pdfoxium-render-{index}"), move || {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    let document = (|| {
                        config.install()?;
                        let document = Document::from_bytes_with_password(
                            bytes,
                            options.password.as_deref().unwrap_or(""),
                        )?;
                        let count = document.page_count()?;
                        let layers = document.optional_content_state()?;
                        Ok((document, count, layers))
                    })();
                    match document {
                        Ok((document, count, layers)) => {
                            if startup.send(Ok((count, layers))).is_ok() {
                                worker(document, &worker_queue, &run, &options);
                            }
                        }
                        Err(error) => {
                            let _ = startup.send(Err(error));
                        }
                    }
                }));
                // Unexpected panics fail queued tickets rather than stranding
                // a waiter. Active/paused completion guards unwind locally.
                worker_queue.close();
            })?;
            workers.queues.push(queue);
            workers.handles.push(handle);
            startup_results.push(ready);
        }
        for (index, ready) in startup_results.into_iter().enumerate() {
            let (count, layers) = ready.recv().map_err(|_| Error::Poisoned)??;
            if index == 0 {
                workers.optional_content = Some(layers);
            }
            if index != 0 && count != workers.page_count {
                return Err(Error::Internal("workers disagree on page count"));
            }
            workers.page_count = count;
        }
        Ok(workers)
    }
}

/// A pool for static PDF snapshots. `Document`, `Page` and progressive jobs
/// never cross worker threads. Page index modulo worker count determines affinity;
/// there is no work stealing in this initial implementation.
///
/// Higher priorities preempt at cooperative pause points, not at a guaranteed
/// deadline. Same-page preemption drops the old context and restarts it later;
/// a valid newer request covering it cancels the older queued request instead.
/// Dropping the pool cancels all requests and joins its workers (except the
/// current thread if a custom waker drops the pool from inside that worker).
pub struct RenderPool {
    workers: WorkerSet,
    config: Config,
    options: PoolOptions,
    optional_content: Mutex<Option<crate::OptionalContentSnapshot>>,
}
impl RenderPool {
    pub fn new(source: Source, config: Config, threads: usize) -> Result<Self> {
        Self::with_options(
            source,
            config,
            PoolOptions {
                threads,
                ..PoolOptions::default()
            },
        )
    }
    /// Starts and validates every worker before returning. No library settings
    /// are installed on the calling thread.
    pub fn with_options(source: Source, config: Config, options: PoolOptions) -> Result<Self> {
        if options.threads == 0
            || options.max_pending_jobs == 0
            || options.max_cached_pages <= options.max_paused_jobs
        {
            return Err(Error::InvalidArgument(
                "pool needs workers, a nonempty queue, and more cached pages than paused jobs",
            ));
        }
        let workers = WorkerSet::start(
            source.bytes()?,
            &config,
            &options,
            Arc::new(AtomicU64::new(0)),
            0,
        )?;
        Ok(Self {
            workers,
            config,
            options,
            optional_content: Mutex::new(None),
        })
    }
    pub fn has_optional_content(&self) -> bool {
        self.workers
            .optional_content
            .as_ref()
            .is_some_and(|s| !s.is_empty())
    }
    pub fn optional_content_state(&self) -> crate::OptionalContentState {
        self.workers
            .optional_content
            .as_ref()
            .expect("validated first worker")
            .clone()
    }
    /// Change the default for subsequent requests, without reparsing pages or
    /// flushing font/image caches. Existing tickets retain their captured state.
    pub fn set_optional_content(&self, state: crate::OptionalContentSnapshot) -> Result<()> {
        let original = self
            .workers
            .optional_content
            .as_ref()
            .expect("validated first worker");
        if !state.same_source(&original.snapshot()) {
            return Err(Error::InvalidArgument(
                "optional content snapshot belongs to another pool source",
            ));
        }
        *lock(&self.optional_content) = Some(state);
        Ok(())
    }
    pub fn page_count(&self) -> usize {
        self.workers.page_count
    }
    pub fn worker_count(&self) -> usize {
        self.workers.queues.len()
    }
    pub fn generation(&self) -> u64 {
        self.workers.run.source_generation
    }

    /// Prepare a fresh worker set, then invalidate old tickets and swap sources.
    /// Failure leaves the old pool usable. This blocks for startup and for old
    /// workers to reach pause points; call away from the UI thread if necessary.
    /// Old completed-but-unclaimed tickets are cancelled too. Already claimed
    /// results retain their old `PoolSource`; callers must reject stale images.
    pub fn reload(&mut self, source: Source) -> Result<u64> {
        let next = self
            .generation()
            .checked_add(1)
            .ok_or(Error::InvalidArgument("pool generation exhausted"))?;
        let generation = self.workers.run.generation.clone();
        let prepared = WorkerSet::start(
            source.bytes()?,
            &self.config,
            &self.options,
            generation.clone(),
            next,
        )?;
        *lock(&self.optional_content) = None;
        generation.store(next, Ordering::Release);
        let old = std::mem::replace(&mut self.workers, prepared);
        drop(old);
        Ok(next)
    }
    pub fn render(
        &self,
        page_index: usize,
        config: RenderConfig,
        priority: Priority,
    ) -> Ticket<PoolBitmap> {
        self.submit(page_index, config, priority, None)
    }
    /// Attach the group before the worker can start the request.
    pub fn render_in_group(
        &self,
        page_index: usize,
        config: RenderConfig,
        priority: Priority,
        group: &CancelGroup,
    ) -> Ticket<PoolBitmap> {
        self.submit(page_index, config, priority, Some(group))
    }
    fn submit(
        &self,
        page: usize,
        mut config: RenderConfig,
        priority: Priority,
        group: Option<&CancelGroup>,
    ) -> Ticket<PoolBitmap> {
        if config.optional_content.is_none() {
            config.optional_content = lock(&self.optional_content).clone();
        }
        let (ticket, completion) = Completion::pair(self.workers.run.clone(), group);
        if page >= self.page_count() {
            completion.finish(Err(Error::PageOutOfRange {
                index: page,
                count: self.page_count(),
            }));
            return ticket;
        }
        let queue = &self.workers.queues[page % self.worker_count()];
        let mut state = lock(&queue.state);
        let error = if state.closed {
            Some(Error::Poisoned)
        } else if state.pending.len() >= self.options.max_pending_jobs {
            Some(Error::Busy)
        } else if state.next_sequence == u64::MAX {
            Some(Error::InvalidArgument("request sequence exhausted"))
        } else {
            None
        };
        if let Some(error) = error {
            drop(state);
            completion.finish(Err(error));
            return ticket;
        }
        let sequence = state.next_sequence;
        state.next_sequence += 1;
        let cancellation = completion.cancellation();
        state.pending.push(Request {
            page,
            config,
            priority,
            sequence,
            completion,
            cancellation,
        });
        drop(state);
        queue.available.notify_one();
        ticket
    }
}

struct Active {
    render: RenderJob<'static>,
    request: Request,
}

enum Next {
    Pending(Request),
    Paused(Active),
}

fn next(queue: &Queue, paused: &mut Vec<Active>) -> Option<Next> {
    loop {
        // Cancellation must not starve behind an endless higher-priority stream.
        queue.discard_where(|request| request.cancellation.is_cancelled());
        let mut i = 0;
        while i < paused.len() {
            if paused[i].request.cancellation.is_cancelled() {
                let Active { render, request } = paused.swap_remove(i);
                drop(render);
                request.completion.finish(Err(Error::Cancelled));
            } else {
                i += 1;
            }
        }
        let mut state = lock(&queue.state);
        if state.closed {
            return None;
        }
        let queued = state
            .pending
            .iter()
            .enumerate()
            .max_by_key(|(_, r)| r.key())
            .map(|(i, _)| i);
        let saved = paused
            .iter()
            .enumerate()
            .max_by_key(|(_, a)| a.request.key())
            .map(|(i, _)| i);
        if let Some(index) = queued {
            if saved.is_none_or(|i| state.pending[index].key() > paused[i].request.key()) {
                // Prefer a newer covering request at the same priority, but only
                // discard its predecessors after successful render validation.
                let index = state
                    .pending
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        !r.cancellation.is_cancelled() && r.supersedes(&state.pending[index])
                    })
                    .max_by_key(|(_, r)| r.sequence)
                    .map_or(index, |(i, _)| i);
                return Some(Next::Pending(state.pending.swap_remove(index)));
            }
        }
        if let Some(index) = saved {
            return Some(Next::Paused(paused.swap_remove(index)));
        }
        state = queue
            .available
            .wait(state)
            .unwrap_or_else(|error| error.into_inner());
        drop(state);
    }
}

fn restart(active: Active, queue: &Queue, capacity: usize) {
    let Active { render, request } = active;
    drop(render); // Release the page lease before another job is constructed.
    queue.requeue(request, capacity);
}

fn trim_pages(
    document: &Document,
    _queue: &Queue,
    resident: &mut VecDeque<usize>,
    paused: &[Active],
    limit: usize,
) -> Result<()> {
    while resident.len() > limit {
        let Some(index) = resident
            .iter()
            .position(|page| !paused.iter().any(|a| a.request.page == *page))
        else {
            break;
        };
        let page = resident.remove(index).unwrap();
        document.page(page)?.unload()?;
        #[cfg(test)]
        _queue.event(Event::Evicted(page));
    }
    Ok(())
}

fn worker(document: Document, queue: &Queue, run: &RunState, options: &PoolOptions) {
    let mut paused: Vec<Active> = Vec::new();
    let mut resident = VecDeque::new();
    while let Some(next) = next(queue, &mut paused) {
        let active = match next {
            Next::Paused(active) => {
                resident.retain(|page| *page != active.request.page);
                resident.push_back(active.request.page);
                #[cfg(test)]
                queue.event(Event::Resumed(active.request.page, active.request.sequence));
                active
            }
            Next::Pending(request) => {
                if request.cancellation.is_cancelled() {
                    request.completion.finish(Err(Error::Cancelled));
                    continue;
                }
                if let Some(i) = paused.iter().position(|a| a.request.page == request.page) {
                    restart(paused.swap_remove(i), queue, options.max_pending_jobs);
                }
                resident.retain(|page| *page != request.page);
                resident.push_back(request.page);
                if let Err(error) = trim_pages(
                    &document,
                    queue,
                    &mut resident,
                    &paused,
                    options.max_cached_pages,
                ) {
                    request.completion.finish(Err(error));
                    break;
                }
                let render = match document
                    .page(request.page)
                    .and_then(|p| p.render_job(&request.config))
                {
                    Ok(render) => render,
                    Err(error) => {
                        request.completion.finish(Err(error));
                        continue;
                    }
                };
                // Loading/parsing can be lengthy and is not interruptible. A
                // request cancelled during it must not supersede valid work.
                if request.cancellation.is_cancelled() {
                    drop(render);
                    request.completion.finish(Err(Error::Cancelled));
                    continue;
                }
                queue.discard_covered(&request);
                #[cfg(test)]
                queue.event(Event::Started(request.page, request.sequence));
                Active { render, request }
            }
        };
        let Active {
            mut render,
            request,
        } = active;
        if request.cancellation.is_cancelled() {
            drop(render);
            request.completion.finish(Err(Error::Cancelled));
            continue;
        }
        let outcome = loop {
            let outcome = render.resume(&mut || {
                request.cancellation.is_cancelled() || queue.higher_than(request.priority)
            });
            // Mask breaks also return Paused. Without a higher-priority request,
            // immediately continue; a zero paused-context budget must not restart
            // the same mask forever.
            if matches!(outcome, Ok(Progress::Paused))
                && !request.cancellation.is_cancelled()
                && !queue.higher_than(request.priority)
            {
                continue;
            }
            break outcome;
        };
        if request.cancellation.is_cancelled() {
            drop(render);
            request.completion.finish(Err(Error::Cancelled));
        } else {
            match outcome {
                Ok(Progress::Done) => {
                    let result = render.finish().and_then(|output| match output {
                        RenderOutput::Owned(bitmap) => Ok(PoolBitmap {
                            bitmap,
                            source: PoolSource {
                                generation: run.source_generation,
                                page_index: request.page,
                            },
                        }),
                        RenderOutput::Borrowed(_) => {
                            Err(Error::Internal("pool render returned borrowed output"))
                        }
                    });
                    #[cfg(test)]
                    queue.event(Event::Finished(request.page, request.sequence));
                    request.completion.finish(result);
                }
                Ok(Progress::Paused) => {
                    #[cfg(test)]
                    queue.event(Event::Paused(request.page, request.sequence));
                    paused.push(Active { render, request });
                    if paused.len() > options.max_paused_jobs {
                        let index = paused
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, a)| a.request.key())
                            .unwrap()
                            .0;
                        restart(paused.swap_remove(index), queue, options.max_pending_jobs);
                    }
                }
                Err(error) => {
                    drop(render);
                    request.completion.finish(Err(error));
                }
            }
        }
        // Unexpected unload failures poison this worker rather than silently
        // violating its residency bound or continuing with a damaged document.
        if trim_pages(
            &document,
            queue,
            &mut resident,
            &paused,
            options.max_cached_pages,
        )
        .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests;

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(bytes) => f
                .debug_struct("Source::Bytes")
                .field("length", &bytes.len())
                .finish(),
            Self::Path(path) => f.debug_tuple("Source::Path").field(path).finish(),
        }
    }
}
impl std::fmt::Debug for PoolOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolOptions")
            .field("threads", &self.threads)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("max_pending_jobs", &self.max_pending_jobs)
            .field("max_paused_jobs", &self.max_paused_jobs)
            .field("max_cached_pages", &self.max_cached_pages)
            .finish()
    }
}
