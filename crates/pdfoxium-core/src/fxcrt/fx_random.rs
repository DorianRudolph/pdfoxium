// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxcrt/fx_random.{h,cpp}`. Each call to `fill()` creates a new
//! PDFium MT19937 generator from a monotonically incremented process seed.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
use web_time::{SystemTime, UNIX_EPOCH};

const STATE_SIZE: usize = 624;
const TWIST_OFFSET: usize = 397;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;
const MATRIX_A: [u32; 2] = [0, 0x9908_b0df];

struct FxRandom {
    next_index: usize,
    state: [u32; STATE_SIZE],
}

impl FxRandom {
    fn new(seed: u32) -> Self {
        let mut state = [0; STATE_SIZE];
        state[0] = seed;
        for i in 1..STATE_SIZE {
            let previous = state[i - 1];
            state[i] = 1_812_433_253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(i as u32);
        }
        Self {
            next_index: STATE_SIZE,
            state,
        }
    }

    fn generate(&mut self) -> u32 {
        if self.next_index >= STATE_SIZE {
            for i in 0..STATE_SIZE - TWIST_OFFSET {
                let result = (self.state[i] & UPPER_MASK) | (self.state[i + 1] & LOWER_MASK);
                self.state[i] =
                    self.state[i + TWIST_OFFSET] ^ (result >> 1) ^ MATRIX_A[(result & 1) as usize];
            }
            for i in STATE_SIZE - TWIST_OFFSET..STATE_SIZE - 1 {
                let result = (self.state[i] & UPPER_MASK) | (self.state[i + 1] & LOWER_MASK);
                self.state[i] = self.state[i + TWIST_OFFSET - STATE_SIZE]
                    ^ (result >> 1)
                    ^ MATRIX_A[(result & 1) as usize];
            }
            let result = (self.state[STATE_SIZE - 1] & UPPER_MASK) | (self.state[0] & LOWER_MASK);
            self.state[STATE_SIZE - 1] =
                self.state[TWIST_OFFSET - 1] ^ (result >> 1) ^ MATRIX_A[(result & 1) as usize];
            self.next_index = 0;
        }
        let mut result = self.state[self.next_index];
        self.next_index += 1;
        result ^= result >> 11;
        result ^= (result << 7) & 0x9d2c_5680;
        result ^= (result << 15) & 0xefc6_0000;
        result ^= result >> 18;
        result
    }
}

fn seed_from_environment() -> u32 {
    let stack_marker = 0u8;
    let stack_address = &stack_marker as *const u8 as usize;
    let mut seed = !((stack_address >> 3) as u32);
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    seed ^= (elapsed.as_secs() as u32).wrapping_mul(1_000_000);
    seed ^= elapsed.subsec_micros();
    #[cfg(not(target_arch = "wasm32"))]
    {
        seed ^ std::process::id()
    }
    #[cfg(target_arch = "wasm32")]
    {
        seed
    }
}

fn next_global_seed() -> u32 {
    static GLOBAL_SEED: OnceLock<AtomicU32> = OnceLock::new();
    GLOBAL_SEED
        .get_or_init(|| AtomicU32::new(seed_from_environment()))
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
}

pub fn fill(buffer: &mut [u32]) {
    let mut generator = FxRandom::new(next_global_seed());
    for value in buffer {
        *value = generator.generate();
    }
}

#[cfg(test)]
mod tests {
    use super::FxRandom;

    #[test]
    fn reference_mt19937_sequence_crosses_twist_boundary() {
        let mut generator = FxRandom::new(5489);
        assert_eq!(generator.generate(), 3_499_211_612);
        assert_eq!(generator.generate(), 581_869_302);
        for _ in 2..624 {
            generator.generate();
        }
        assert_eq!(generator.generate(), 4_178_893_912);
    }
}

#[cfg(test)]
#[path = "fx_random_unittest.rs"]
mod upstream_tests;
