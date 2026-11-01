// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Opt-in CPU/frame latency diagnostics; never retains typed text.
use serde_json::{json, Value};
use std::collections::VecDeque;
use web_time::Instant;
struct Sample {
    id: u64,
    kind: &'static str,
    start: Instant,
    frame: u64,
    ack: Option<(Instant, u64, u64)>,
    queued: Option<u64>,
    uploaded: Option<(Instant, u64)>,
    work_ms: Option<(f64, f64)>,
}
pub struct Trace {
    enabled: bool,
    pub roundtrip: bool,
    next: u64,
    pub frame: u64,
    samples: VecDeque<Sample>,
}
impl Default for Trace {
    fn default() -> Self {
        Self {
            enabled: std::env::var_os("PDFOXIUM_VIEWER_TRACE_TYPING").is_some(),
            roundtrip: std::env::var_os("PDFOXIUM_VIEWER_FORM_ROUNDTRIP").is_some(),
            next: 0,
            frame: 0,
            samples: VecDeque::new(),
        }
    }
}
impl Trace {
    pub fn begin(&mut self, kind: &'static str) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        self.next += 1;
        if self.samples.len() == 128 {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample {
            id: self.next,
            kind,
            start: Instant::now(),
            frame: self.frame,
            ack: None,
            queued: None,
            uploaded: None,
            work_ms: None,
        });
        Some(self.next)
    }
    pub fn queued_input(&mut self, id: u64) {
        if let Some(s) = self.samples.iter_mut().find(|s| s.id == id) {
            s.queued = Some(self.frame);
        }
    }
    pub fn ack(&mut self, id: u64, generation: u64, work_ms: Option<(f64, f64)>) {
        if let Some(s) = self.samples.iter_mut().find(|s| s.id == id) {
            s.ack = Some((Instant::now(), self.frame, generation));
            s.work_ms = work_ms;
        }
    }
    pub fn queued(&mut self, generation: u64) {
        for s in &mut self.samples {
            if s.uploaded.is_none()
                && s.queued.is_none()
                && s.ack.is_some_and(|(_, _, g)| g <= generation)
            {
                s.queued = Some(self.frame);
            }
        }
    }
    pub fn uploaded(&mut self, generation: u64) {
        for s in &mut self.samples {
            if s.uploaded.is_none() && s.ack.is_some_and(|(_, _, g)| g <= generation) {
                s.uploaded = Some((Instant::now(), self.frame));
            }
        }
    }
    pub fn snapshot(&self) -> Value {
        json!({"enabled":self.enabled,"roundtrip":self.roundtrip,"samples":self.samples.iter().map(|s|json!({"id":s.id,"kind":s.kind,"dispatch_and_input_ms":s.work_ms.map(|w|w.0),"paint_ms":s.work_ms.map(|w|w.1),"input_frame":s.frame,"ack_frame":s.ack.map(|(_,f,_)|f),"paint_queued_frame":s.queued,"upload_frame":s.uploaded.map(|(_,f)|f),"input_ack_ms":s.ack.map(|(t,_,_)|t.duration_since(s.start).as_secs_f64()*1000.),"input_to_upload_ms":s.uploaded.map(|(t,_)|t.duration_since(s.start).as_secs_f64()*1000.)})).collect::<Vec<_>>()})
    }
}
