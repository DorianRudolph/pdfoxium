// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::*;
use crate::{Color, DeviceRect, PathSpec, Point, Rotation, Size};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

fn fixture() -> Arc<[u8]> {
    let document = Document::create().unwrap();
    for index in 0..3 {
        let page = document
            .insert_blank_page(
                index,
                Size {
                    width: 40.,
                    height: 30.,
                },
            )
            .unwrap();
        for _ in 0..220 {
            page.add_path(&PathSpec {
                commands: vec![
                    crate::PathCommand::MoveTo(Point { x: 0., y: 0. }),
                    crate::PathCommand::LineTo(Point { x: 40., y: 0. }),
                    crate::PathCommand::LineTo(Point { x: 40., y: 30. }),
                    crate::PathCommand::LineTo(Point { x: 0., y: 30. }),
                    crate::PathCommand::Close,
                ],
                fill: Some(if index == 1 {
                    Color::rgb(0, 0, 255)
                } else {
                    Color::rgb(255, 0, 0)
                }),
                ..PathSpec::default()
            })
            .unwrap();
        }
    }
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes.into()
}

fn run_state() -> Arc<RunState> {
    Arc::new(RunState {
        shutdown: AtomicBool::new(false),
        generation: Arc::new(AtomicU64::new(0)),
        source_generation: 0,
    })
}
fn queued_pool(options: PoolOptions) -> RenderPool {
    RenderPool {
        workers: WorkerSet {
            run: run_state(),
            queues: (0..options.threads)
                .map(|_| Arc::new(Queue::new()))
                .collect(),
            handles: Vec::new(),
            page_count: 3,
            optional_content: None,
        },
        optional_content: Mutex::new(None),
        config: Config::default(),
        options,
    }
}
fn launch(pool: &mut RenderPool, bytes: Arc<[u8]>) {
    for queue in &pool.workers.queues {
        let queue = queue.clone();
        let run = pool.workers.run.clone();
        let options = pool.options.clone();
        let bytes = bytes.clone();
        pool.workers.handles.push(thread::spawn(move || {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let document = Document::from_bytes(bytes).unwrap();
                worker(document, &queue, &run, &options);
            }));
            queue.close();
        }));
    }
}
fn wait<T: Send + 'static>(ticket: Ticket<T>) -> Result<T> {
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let _ = send.send(ticket.wait());
    });
    receive
        .recv_timeout(Duration::from_secs(30))
        .expect("ticket stranded")
}
fn pixels(bytes: &Arc<[u8]>, page: usize, config: &RenderConfig) -> Vec<u8> {
    Document::from_bytes(bytes.clone())
        .unwrap()
        .page(page)
        .unwrap()
        .render(config)
        .unwrap()
        .pixels()
        .to_vec()
}

#[test]
fn public_types_cross_threads_but_documents_stay_local() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<RenderPool>();
    send_sync::<Ticket<PoolBitmap>>();
    send_sync::<Source>();
    send_sync::<CancelGroup>();
}

#[test]
fn preemption_resumes_other_pages_and_restarts_the_same_page() {
    let bytes = fixture();
    for (urgent_page, budget) in [(1, 2), (0, 2), (1, 0)] {
        let mut pool = queued_pool(PoolOptions {
            max_paused_jobs: budget,
            ..PoolOptions::default()
        });
        let low_cfg = RenderConfig::default().device_clip(DeviceRect::new(0, 0, 20, 30));
        let high_cfg = RenderConfig::default().device_clip(DeviceRect::new(20, 0, 20, 30));
        let low = pool.render(0, low_cfg.clone(), Priority::Background);
        let high = pool.render(urgent_page, high_cfg.clone(), Priority::Visible);
        // Hold the urgent request until background rendering actually starts.
        let urgent = lock(&pool.workers.queues[0].state).pending.pop().unwrap();
        let urgent = Mutex::new(Some(urgent));
        let queue = Arc::downgrade(&pool.workers.queues[0]);
        let events = Arc::new(Mutex::new(Vec::new()));
        let record = events.clone();
        *lock(&pool.workers.queues[0].observer) = Some(Arc::new(move |event| {
            lock(&record).push(event.clone());
            if event == Event::Started(0, 0) {
                if let Some(urgent) = lock(&urgent).take() {
                    let queue = queue.upgrade().unwrap();
                    lock(&queue.state).pending.push(urgent);
                    queue.available.notify_one();
                }
            }
        }));
        launch(&mut pool, bytes.clone());
        let high = wait(high).unwrap();
        let low = wait(low).unwrap();
        assert_eq!(high.bitmap.pixels(), pixels(&bytes, urgent_page, &high_cfg));
        assert_eq!(low.bitmap.pixels(), pixels(&bytes, 0, &low_cfg));
        let events = lock(&events);
        assert!(events.contains(&Event::Paused(0, 0)), "{events:?}");
        let high_done = events
            .iter()
            .position(|e| *e == Event::Finished(urgent_page, 1))
            .unwrap();
        let low_done = events
            .iter()
            .position(|e| *e == Event::Finished(0, 0))
            .unwrap();
        assert!(high_done < low_done, "{events:?}");
        if urgent_page == 0 || budget == 0 {
            assert_eq!(
                events
                    .iter()
                    .filter(|e| **e == Event::Started(0, 0))
                    .count(),
                2
            );
            assert!(!events.contains(&Event::Resumed(0, 0)));
        } else {
            assert!(events.contains(&Event::Resumed(0, 0)));
        }
    }
}

#[test]
fn queued_replacement_waits_for_validation_and_preserves_different_settings() {
    let bytes = fixture();
    for invalid in [false, true] {
        let mut pool = queued_pool(PoolOptions::default());
        let old = pool.render(
            0,
            RenderConfig::default().device_clip(DeviceRect::new(0, 0, 10, 10)),
            Priority::Visible,
        );
        let new = pool.render(
            0,
            if invalid {
                RenderConfig::default().device_clip(DeviceRect::new(0, 0, 900, 900))
            } else {
                RenderConfig::default()
            },
            Priority::Visible,
        );
        let rotated = pool.render(
            0,
            RenderConfig::default().rotation(Rotation::Cw90),
            Priority::Visible,
        );
        launch(&mut pool, bytes.clone());
        if invalid {
            assert!(matches!(wait(new), Err(Error::InvalidArgument(_))));
            assert!(wait(old).is_ok());
        } else {
            assert!(wait(new).is_ok());
            assert!(matches!(wait(old), Err(Error::Cancelled)));
        }
        assert!(wait(rotated).is_ok());
    }
}

#[test]
fn cancelled_queued_active_grouped_and_dropped_requests_release_work() {
    let bytes = fixture();
    let mut pool = queued_pool(PoolOptions::default());
    let cancelled = pool.render(0, RenderConfig::default(), Priority::Visible);
    cancelled.cancel();
    assert!(matches!(wait(cancelled), Err(Error::Cancelled)));
    let abandoned = pool.render(1, RenderConfig::default(), Priority::Visible);
    let token = lock(&pool.workers.queues[0].state)
        .pending
        .last()
        .unwrap()
        .cancellation
        .clone();
    drop(abandoned);
    assert!(token.is_cancelled());
    let group = CancelGroup::new();
    let grouped = pool.render_in_group(2, RenderConfig::default(), Priority::Visible, &group);
    group.cancel_all();
    let fresh = pool.render_in_group(2, RenderConfig::default(), Priority::Thumbnail, &group);
    // Cancel while a job owns a page lease, before its first actual render step.
    let active = pool.render_in_group(0, RenderConfig::default(), Priority::Visible, &group);
    let sequence = lock(&pool.workers.queues[0].state)
        .pending
        .last()
        .unwrap()
        .sequence;
    let cancel = group.clone();
    let records = Arc::new(Mutex::new(Vec::new()));
    let record = records.clone();
    *lock(&pool.workers.queues[0].observer) = Some(Arc::new(move |event| {
        lock(&record).push(event.clone());
        if event == Event::Started(0, sequence) {
            cancel.cancel_all();
        }
    }));
    launch(&mut pool, bytes.clone());
    assert!(matches!(wait(grouped), Err(Error::Cancelled)));
    assert!(matches!(wait(active), Err(Error::Cancelled)));
    assert!(matches!(wait(fresh), Err(Error::Cancelled)));
    // A new group generation remains usable, including the previously leased page.
    assert!(
        wait(pool.render_in_group(0, RenderConfig::default(), Priority::Visible, &group)).is_ok()
    );
    assert!(!lock(&records)
        .iter()
        .any(|e| matches!(e, Event::Started(1, _))));
}

#[test]
fn priority_fifo_queue_bounds_and_shutdown_resolve_tickets() {
    let pool = queued_pool(PoolOptions {
        max_pending_jobs: 2,
        ..PoolOptions::default()
    });
    let first = pool.render(0, RenderConfig::default(), Priority::Background);
    let second = pool.render(1, RenderConfig::default(), Priority::Visible);
    let busy = pool.render(2, RenderConfig::default(), Priority::Visible);
    assert!(matches!(wait(busy), Err(Error::Busy)));
    let mut paused = Vec::new();
    let Some(Next::Pending(request)) = next(&pool.workers.queues[0], &mut paused) else {
        panic!()
    };
    assert_eq!(request.page, 1);
    request.completion.finish(Err(Error::Cancelled));
    assert!(matches!(wait(second), Err(Error::Cancelled)));
    drop(pool);
    assert!(matches!(wait(first), Err(Error::Cancelled)));
}

#[test]
fn worker_panic_fails_active_queued_and_future_requests() {
    let mut pool = queued_pool(PoolOptions::default());
    let active = pool.render(0, RenderConfig::default(), Priority::Visible);
    let queued = pool.render(1, RenderConfig::default(), Priority::Visible);
    *lock(&pool.workers.queues[0].observer) = Some(Arc::new(|_| panic!("injected worker panic")));
    launch(&mut pool, fixture());
    assert!(matches!(wait(active), Err(Error::Poisoned)));
    assert!(matches!(wait(queued), Err(Error::Poisoned)));
    assert!(matches!(
        wait(pool.render(2, RenderConfig::default(), Priority::Visible)),
        Err(Error::Poisoned)
    ));
}

#[test]
fn reload_is_transactional_and_invalidates_unclaimed_results() {
    let bytes = fixture();
    let mut pool = RenderPool::new(Source::Bytes(bytes.clone()), Config::default(), 2).unwrap();
    assert_eq!(pool.page_count(), 3);
    let first = wait(pool.render(1, RenderConfig::default(), Priority::Visible)).unwrap();
    assert_eq!(
        first.source,
        PoolSource {
            generation: 0,
            page_index: 1
        }
    );
    assert!(matches!(
        pool.reload(Source::Bytes(Arc::from(&b"not pdf"[..]))),
        Err(Error::Format)
    ));
    assert_eq!(pool.generation(), 0);
    let pending = pool.render(0, RenderConfig::default(), Priority::Visible);
    // Page 2 uses the same worker: completing it proves page 0's result is
    // ready but still unclaimed when reload invalidates the old generation.
    wait(pool.render(2, RenderConfig::default(), Priority::Visible)).unwrap();
    let replacement: Arc<[u8]> = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/render_corpus/pdfs/rectangles.pdf"),
    )
    .unwrap()
    .into();
    assert_eq!(pool.reload(Source::Bytes(replacement.clone())).unwrap(), 1);
    assert_eq!(pool.page_count(), 1);
    assert!(matches!(wait(pending), Err(Error::Cancelled)));
    let next = wait(pool.render(0, RenderConfig::default(), Priority::Visible)).unwrap();
    assert_eq!(next.source.generation, 1);
    assert_eq!(
        next.bitmap.pixels(),
        pixels(&replacement, 0, &RenderConfig::default())
    );
    assert_ne!(next.bitmap.pixels(), first.bitmap.pixels());
    assert!(matches!(
        wait(pool.render(3, RenderConfig::default(), Priority::Visible)),
        Err(Error::PageOutOfRange { .. })
    ));
}

#[test]
fn affinity_and_residency_eviction_are_observable_without_moving_documents() {
    let bytes = fixture();
    let mut pool = queued_pool(PoolOptions {
        threads: 2,
        max_cached_pages: 1,
        max_paused_jobs: 0,
        ..PoolOptions::default()
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    for (index, queue) in pool.workers.queues.iter().enumerate() {
        let record = events.clone();
        *lock(&queue.observer) = Some(Arc::new(move |event| {
            lock(&record).push((index, event));
        }));
    }
    launch(&mut pool, bytes.clone());
    for page in [0, 1, 2, 0, 1] {
        let result = wait(pool.render(page, RenderConfig::default(), Priority::Visible)).unwrap();
        assert_eq!(
            result.bitmap.pixels(),
            pixels(&bytes, page, &RenderConfig::default())
        );
    }
    let events = lock(&events);
    for (worker, event) in events.iter() {
        if let Event::Started(page, _) = event {
            assert_eq!(*worker, page % 2);
        }
    }
    assert!(events.contains(&(0, Event::Evicted(0))));
    assert!(events.contains(&(0, Event::Evicted(2))));
}

#[test]
fn futures_register_wakers_and_explicit_cancel_wakes_waiters() {
    struct Counter(std::sync::atomic::AtomicUsize);
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let counter = Arc::new(Counter(std::sync::atomic::AtomicUsize::new(0)));
    let waker = Waker::from(counter.clone());
    let mut context = Context::from_waker(&waker);
    let (mut ticket, completion) = Completion::<u32>::pair(run_state(), None);
    assert!(Pin::new(&mut ticket).poll(&mut context).is_pending());
    completion.finish(Ok(42));
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    assert!(matches!(
        Pin::new(&mut ticket).poll(&mut context),
        Poll::Ready(Ok(42))
    ));
    assert!(ticket.try_take().is_none());
    let (mut ticket, _completion) = Completion::<u32>::pair(run_state(), None);
    assert!(Pin::new(&mut ticket).poll(&mut context).is_pending());
    ticket.cancel();
    assert_eq!(counter.0.load(Ordering::SeqCst), 2);
    assert!(matches!(
        Pin::new(&mut ticket).poll(&mut context),
        Poll::Ready(Err(Error::Cancelled))
    ));
}

#[test]
fn input_and_configuration_errors_are_reported_before_returning_a_pool() {
    assert!(matches!(
        RenderPool::new(Source::Bytes(Arc::from(&b"bad"[..])), Config::default(), 1),
        Err(Error::Format)
    ));
    assert!(matches!(
        RenderPool::new(Source::Bytes(fixture()), Config::default(), 0),
        Err(Error::InvalidArgument(_))
    ));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/render_corpus/pdfs");
    assert!(matches!(
        RenderPool::new(
            Source::Path(root.join("encrypted_r6.pdf")),
            Config::default(),
            2
        ),
        Err(Error::Password)
    ));
    let pool = RenderPool::with_options(
        Source::Path(root.join("encrypted_r6.pdf")),
        Config::default(),
        PoolOptions {
            threads: 2,
            password: Some("âge".into()),
            ..PoolOptions::default()
        },
    )
    .unwrap();
    assert!(wait(pool.render(0, RenderConfig::default(), Priority::Visible)).is_ok());
}

#[test]
fn dropping_pool_joins_active_work_and_cancels_pending_results() {
    let mut pool = queued_pool(PoolOptions::default());
    let ticket = pool.render(0, RenderConfig::default(), Priority::Visible);
    let queued = pool.render(1, RenderConfig::default(), Priority::Thumbnail);
    let queue = Arc::downgrade(&pool.workers.queues[0]);
    let (started, ready) = mpsc::channel();
    *lock(&pool.workers.queues[0].observer) = Some(Arc::new(move |event| {
        if event == Event::Started(0, 0) {
            started.send(()).unwrap();
            let queue = queue.upgrade().unwrap();
            let mut state = lock(&queue.state);
            while !state.closed {
                state = queue.available.wait(state).unwrap();
            }
        }
    }));
    launch(&mut pool, fixture());
    ready.recv_timeout(Duration::from_secs(30)).unwrap();
    drop(pool);
    assert!(matches!(wait(ticket), Err(Error::Cancelled)));
    assert!(matches!(wait(queued), Err(Error::Cancelled)));
}

#[test]
fn attaching_another_group_cannot_revive_a_cancelled_ticket() {
    let first = CancelGroup::new();
    let second = CancelGroup::new();
    let (ticket, completion) = Completion::<u32>::pair(run_state(), Some(&first));
    first.cancel_all();
    let ticket = ticket.group(&second);
    assert!(ticket.is_cancelled());
    completion.finish(Ok(1));
    assert!(matches!(wait(ticket), Err(Error::Cancelled)));
}

#[test]
fn paused_capacity_restarts_the_least_urgent_context() {
    let bytes = fixture();
    let mut pool = queued_pool(PoolOptions {
        max_paused_jobs: 1,
        ..PoolOptions::default()
    });
    let low = pool.render(0, RenderConfig::default(), Priority::Background);
    let middle = pool.render(1, RenderConfig::default(), Priority::Thumbnail);
    let high = pool.render(2, RenderConfig::default(), Priority::Visible);
    let held = Mutex::new(VecDeque::from(
        lock(&pool.workers.queues[0].state).pending.split_off(1),
    ));
    let queue = Arc::downgrade(&pool.workers.queues[0]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let record = events.clone();
    *lock(&pool.workers.queues[0].observer) = Some(Arc::new(move |event| {
        lock(&record).push(event.clone());
        if let Event::Started(_, sequence) = event {
            let mut held = lock(&held);
            if held.front().is_some_and(|r| r.sequence == sequence + 1) {
                lock(&queue.upgrade().unwrap().state)
                    .pending
                    .push(held.pop_front().unwrap());
            }
        }
    }));
    launch(&mut pool, bytes);
    wait(high).unwrap();
    wait(middle).unwrap();
    wait(low).unwrap();
    let events = lock(&events);
    assert_eq!(
        events
            .iter()
            .filter(|e| **e == Event::Started(0, 0))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| **e == Event::Started(1, 1))
            .count(),
        1
    );
    assert!(events.contains(&Event::Resumed(1, 1)));
}

#[test]
fn file_sources_are_snapshots_and_failed_reload_keeps_the_snapshot() {
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let name = format!(
        "pdfoxium-pool-{}-{}.pdf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = Temp(std::env::temp_dir().join(name));
    let bytes = fixture();
    std::fs::write(&path.0, &bytes).unwrap();
    let mut pool = RenderPool::new(Source::Path(path.0.clone()), Config::default(), 2).unwrap();
    std::fs::write(&path.0, b"replaced on disk").unwrap();
    assert!(matches!(
        pool.reload(Source::Path(path.0.clone())),
        Err(Error::Format)
    ));
    let actual = wait(pool.render(0, RenderConfig::default(), Priority::Visible)).unwrap();
    assert_eq!(actual.source.generation, 0);
    assert_eq!(
        actual.bitmap.pixels(),
        pixels(&bytes, 0, &RenderConfig::default())
    );
}

#[test]
fn cancellation_after_a_real_pause_and_at_completion_never_delivers_pixels() {
    let bytes = fixture();
    for at_completion in [false, true] {
        let mut pool = queued_pool(PoolOptions::default());
        let group = CancelGroup::new();
        let low = pool.render_in_group(0, RenderConfig::default(), Priority::Background, &group);
        let high = if at_completion {
            None
        } else {
            Some(pool.render(1, RenderConfig::default(), Priority::Visible))
        };
        let held = Mutex::new(if at_completion {
            None
        } else {
            Some(lock(&pool.workers.queues[0].state).pending.pop().unwrap())
        });
        let queue = Arc::downgrade(&pool.workers.queues[0]);
        let events = Arc::new(Mutex::new(Vec::new()));
        let record = events.clone();
        *lock(&pool.workers.queues[0].observer) = Some(Arc::new(move |event| {
            lock(&record).push(event.clone());
            if event == Event::Started(0, 0) {
                if let Some(request) = lock(&held).take() {
                    lock(&queue.upgrade().unwrap().state).pending.push(request);
                }
            }
            if event
                == (if at_completion {
                    Event::Finished(0, 0)
                } else {
                    Event::Paused(0, 0)
                })
            {
                group.cancel_all();
            }
        }));
        launch(&mut pool, bytes.clone());
        assert!(matches!(wait(low), Err(Error::Cancelled)));
        if let Some(high) = high {
            wait(high).unwrap();
        }
        assert!(lock(&events).contains(&if at_completion {
            Event::Finished(0, 0)
        } else {
            Event::Paused(0, 0)
        }));
    }
}
