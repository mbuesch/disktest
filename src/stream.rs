// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020 Michael Buesch <m@bues.ch>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
//

use crate::generator::{GeneratorChaCha20, GeneratorSHA512, GeneratorCRC, NextRandom};
use crate::kdf::kdf;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::Duration;

/// Stream algorithm type.
#[derive(Copy, Clone, Debug)]
pub enum DtStreamType {
    CHACHA20,
    SHA512,
    CRC,
}

/// Data chunk that contains the computed PRNG data.
pub struct DtStreamChunk {
    pub index: u64,
    pub data: Vec<u8>,
}

/// Thread worker function, that computes the chunks.
fn thread_worker(stype:         DtStreamType,
                 chunk_factor:  usize,
                 seed:          Vec<u8>,
                 thread_id:     u32,
                 abort:         Arc<AtomicBool>,
                 level:         Arc<AtomicIsize>,
                 tx:            Sender<DtStreamChunk>) {
    // Calculate the per-thread-seed from the global seed.
    let thread_seed = kdf(&seed, thread_id);
    drop(seed);

    // Construct the generator algorithm.
    let mut generator: Box<dyn NextRandom> = match stype {
        DtStreamType::CHACHA20 => Box::new(GeneratorChaCha20::new(&thread_seed)),
        DtStreamType::SHA512   => Box::new(GeneratorSHA512::new(&thread_seed)),
        DtStreamType::CRC      => Box::new(GeneratorCRC::new(&thread_seed)),
    };

    // Run the generator work loop.
    let mut index = 0;
    while !abort.load(Ordering::Relaxed) {
        if level.load(Ordering::Relaxed) < DtStream::LEVEL_THRES {

            // Get the next chunk from the generator.
            let data = generator.next(chunk_factor);
            debug_assert_eq!(data.len(), generator.get_size() * chunk_factor);

            let chunk = DtStreamChunk {
                index,
                data,
            };
            index += 1;

            // Send the chunk to the main thread.
            tx.send(chunk).expect("Worker thread: Send failed.");
            level.fetch_add(1, Ordering::Relaxed);
        } else {
            // The chunk buffer is full. Wait...
            thread::sleep(Duration::from_millis(10));
        }
    }
}

/// PRNG stream.
pub struct DtStream {
    stype:          DtStreamType,
    seed:           Vec<u8>,
    thread_id:      u32,
    rx:             Option<Receiver<DtStreamChunk>>,
    is_active:      bool,
    thread_join:    Option<thread::JoinHandle<()>>,
    abort:          Arc<AtomicBool>,
    level:          Arc<AtomicIsize>,
}

impl DtStream {
    /// Maximum number of chunks that the thread will compute in advance.
    const LEVEL_THRES: isize        = 8;

    pub fn new(stype: DtStreamType,
               seed: &Vec<u8>,
               thread_id: u32) -> DtStream {

        let abort = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicIsize::new(0));

        DtStream {
            stype,
            seed: seed.to_vec(),
            thread_id,
            rx: None,
            is_active: false,
            thread_join: None,
            abort,
            level,
        }
    }

    /// Stop the worker thread.
    /// Does nothing, if the thread is not running.
    fn stop(&mut self) {
        self.is_active = false;
        self.abort.store(true, Ordering::Release);
        if let Some(thread_join) = self.thread_join.take() {
            thread_join.join().unwrap();
        }
        self.abort.store(false, Ordering::Release);
    }

    /// Spawn the worker thread.
    /// Panics, if the thread is already running.
    fn start(&mut self) {
        assert!(!self.is_active);
        assert!(self.thread_join.is_none());

        // Initialize thread communication
        self.abort.store(false, Ordering::Release);
        self.level.store(0, Ordering::Release);
        let (tx, rx) = channel();
        self.rx = Some(rx);

        // Spawn the worker thread.
        let thread_stype = self.stype;
        let thread_chunk_factor = self.get_chunk_factor();
        let thread_seed = self.seed.to_vec();
        let thread_id = self.thread_id;
        let thread_abort = Arc::clone(&self.abort);
        let thread_level = Arc::clone(&self.level);
        self.thread_join = Some(thread::spawn(move || {
            thread_worker(thread_stype,
                          thread_chunk_factor,
                          thread_seed,
                          thread_id,
                          thread_abort,
                          thread_level,
                          tx);
        }));
        self.is_active = true;
    }

    /// Activate the worker thread.
    pub fn activate(&mut self) {
        self.stop();
        self.start();
    }

    /// Check if the worker thread is currently running.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Get the size of the selected generator output, in bytes.
    fn get_generator_outsize(&self) -> usize {
        match self.stype {
            DtStreamType::CHACHA20 => GeneratorChaCha20::OUTSIZE,
            DtStreamType::SHA512   => GeneratorSHA512::OUTSIZE,
            DtStreamType::CRC      => GeneratorCRC::OUTSIZE,
        }
    }

    fn get_chunk_factor(&self) -> usize {
        match self.stype {
            DtStreamType::CHACHA20 => GeneratorChaCha20::CHUNKFACTOR,
            DtStreamType::SHA512   => GeneratorSHA512::CHUNKFACTOR,
            DtStreamType::CRC      => GeneratorCRC::CHUNKFACTOR,
        }
    }

    /// Get the size of the chunk returned by get_chunk(), in bytes.
    pub fn get_chunk_size(&self) -> usize {
        self.get_generator_outsize() * self.get_chunk_factor()
    }

    /// Get the next chunk from the thread.
    /// Returns None, if no chunk is available, yet.
    #[inline]
    pub fn get_chunk(&mut self) -> Option<DtStreamChunk> {
        if self.is_active() {
            if let Some(rx) = &self.rx {
                match rx.try_recv() {
                    Ok(chunk) => {
                        self.level.fetch_sub(1, Ordering::Relaxed);
                        Some(chunk)
                    },
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Drop for DtStream {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(algorithm: DtStreamType) {
        let mut s = DtStream::new(algorithm, &vec![1,2,3], 0);
        s.activate();
        assert_eq!(s.is_active(), true);

        assert_eq!(s.get_chunk_size(), s.get_generator_outsize() * s.get_chunk_factor());
        assert!(s.get_chunk_size() > 0);
        assert!(s.get_generator_outsize() > 0);
        assert!(s.get_chunk_factor() > 0);

        let mut results_first = vec![];
        let mut count = 0;
        while count < 5 {
            if let Some(chunk) = s.get_chunk() {
                println!("{}: index={} data[0]={} (current level = {})",
                         count, chunk.index, chunk.data[0], s.level.load(Ordering::Relaxed));
                results_first.push(chunk.data[0]);
                assert_eq!(chunk.index, count);
                count += 1;
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
        match algorithm {
            DtStreamType::CHACHA20 => {
                assert_eq!(results_first, vec![206, 122, 60, 217, 224]);
            }
            DtStreamType::SHA512 => {
                assert_eq!(results_first, vec![226, 143, 221, 30, 59]);
            }
            DtStreamType::CRC => {
                assert_eq!(results_first, vec![132, 133, 170, 226, 104]);
            }
        }
    }

    #[test]
    fn test_chacha20() {
        run_test(DtStreamType::CHACHA20);
    }

    #[test]
    fn test_sha512() {
        run_test(DtStreamType::SHA512);
    }

    #[test]
    fn test_crc() {
        run_test(DtStreamType::CRC);
    }
}

// vim: ts=4 sw=4 expandtab
