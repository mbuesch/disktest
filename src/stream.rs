// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020-2022 Michael Buesch <m@bues.ch>
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

use anyhow as ah;
use crate::bufcache::{BufCache, BufCacheCons};
use crate::generator::{GeneratorChaCha8, GeneratorChaCha12, GeneratorChaCha20, GeneratorCrc, NextRandom};
use crate::kdf::kdf;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::Duration;

/// Stream algorithm type.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DtStreamType {
    ChaCha8,
    ChaCha12,
    ChaCha20,
    Crc,
}

/// Data chunk that contains the computed PRNG data.
pub struct DtStreamChunk {
    pub index:  u64,
    pub data:   Option<Vec<u8>>,
}

/// Thread worker function, that computes the chunks.
#[allow(clippy::too_many_arguments)]
fn thread_worker(stype:             DtStreamType,
                 chunk_factor:      usize,
                 seed:              Vec<u8>,
                 thread_id:         u32,
                 mut cache_cons:    BufCacheCons,
                 byte_offset:       u64,
                 invert_pattern:    bool,
                 abort:             Arc<AtomicBool>,
                 error:             Arc<AtomicBool>,
                 level:             Arc<AtomicIsize>,
                 tx:                Sender<DtStreamChunk>) {
    // Calculate the per-thread-seed from the global seed.
    let thread_seed = kdf(&seed, thread_id);
    drop(seed);

    // Construct the generator algorithm.
    let mut generator: Box<dyn NextRandom> = match stype {
        DtStreamType::ChaCha8 => Box::new(GeneratorChaCha8::new(&thread_seed)),
        DtStreamType::ChaCha12 => Box::new(GeneratorChaCha12::new(&thread_seed)),
        DtStreamType::ChaCha20 => Box::new(GeneratorChaCha20::new(&thread_seed)),
        DtStreamType::Crc => Box::new(GeneratorCrc::new(&thread_seed)),
    };

    // Seek the generator to the specified byte offset.
    if let Err(e) = generator.seek(byte_offset) {
        eprintln!("ERROR in generator thread {}: {}", thread_id, e);
        error.store(true, Ordering::Release);
        return;
    }

    // Run the generator work loop.
    let mut index = 0;
    while !abort.load(Ordering::Relaxed) {
        if level.load(Ordering::Relaxed) < DtStream::LEVEL_THRES {

            // Get the next chunk from the generator.
            let size = generator.get_base_size() * chunk_factor;
            let mut data = cache_cons.pull(size);
            generator.next(&mut data, chunk_factor);
            debug_assert_eq!(data.len(), size);

            // Invert the bit pattern, if requested.
            if invert_pattern {
                for x in &mut data {
                    *x ^= 0xFFu8;
                }
            };

            let chunk = DtStreamChunk {
                index,
                data: Some(data),
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
    invert_pattern: bool,
    thread_id:      u32,
    rx:             Option<Receiver<DtStreamChunk>>,
    cache:          Rc<RefCell<BufCache>>,
    is_active:      bool,
    thread_join:    Option<thread::JoinHandle<()>>,
    abort:          Arc<AtomicBool>,
    error:          Arc<AtomicBool>,
    level:          Arc<AtomicIsize>,
}

impl DtStream {
    /// Maximum number of chunks that the thread will compute in advance.
    const LEVEL_THRES: isize        = 8;

    pub fn new(stype:           DtStreamType,
               seed:            Vec<u8>,
               invert_pattern:  bool,
               thread_id:       u32,
               cache:           Rc<RefCell<BufCache>>) -> DtStream {

        let abort = Arc::new(AtomicBool::new(false));
        let error = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicIsize::new(0));

        DtStream {
            stype,
            seed,
            invert_pattern,
            thread_id,
            rx: None,
            cache,
            is_active: false,
            thread_join: None,
            abort,
            error,
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
    fn start(&mut self, byte_offset: u64) {
        assert!(!self.is_active);
        assert!(self.thread_join.is_none());

        // Initialize thread communication
        self.abort.store(false, Ordering::Release);
        self.error.store(false, Ordering::Release);
        self.level.store(0, Ordering::Release);
        let (tx, rx) = channel();
        self.rx = Some(rx);

        // Spawn the worker thread.
        let thread_stype = self.stype;
        let thread_chunk_factor = self.get_chunk_factor();
        let thread_seed = self.seed.to_vec();
        let thread_id = self.thread_id;
        let thread_cache_cons = self.cache.borrow_mut().new_consumer(self.thread_id);
        let thread_byte_offset = byte_offset;
        let thread_invert_pattern = self.invert_pattern;
        let thread_abort = Arc::clone(&self.abort);
        let thread_error = Arc::clone(&self.error);
        let thread_level = Arc::clone(&self.level);
        self.thread_join = Some(thread::spawn(move || {
            thread_worker(thread_stype,
                          thread_chunk_factor,
                          thread_seed,
                          thread_id,
                          thread_cache_cons,
                          thread_byte_offset,
                          thread_invert_pattern,
                          thread_abort,
                          thread_error,
                          thread_level,
                          tx);
        }));
        self.is_active = true;
    }

    /// Check if the thread exited due to an error.
    #[inline]
    fn is_thread_error(&self) -> bool {
        self.error.load(Ordering::Relaxed)
    }

    /// Activate the worker thread.
    pub fn activate(&mut self, byte_offset: u64) -> ah::Result<()> {
        self.stop();
        self.start(byte_offset);

        Ok(())
    }

    /// Check if the worker thread is currently running.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Get the size of the selected generator output, in bytes.
    fn get_generator_outsize(&self) -> usize {
        match self.stype {
            DtStreamType::ChaCha8 => GeneratorChaCha8::BASE_SIZE,
            DtStreamType::ChaCha12 => GeneratorChaCha12::BASE_SIZE,
            DtStreamType::ChaCha20 => GeneratorChaCha20::BASE_SIZE,
            DtStreamType::Crc => GeneratorCrc::BASE_SIZE,
        }
    }

    /// Get the chunk factor of the selected generator.
    fn get_chunk_factor(&self) -> usize {
        match self.stype {
            DtStreamType::ChaCha8 => GeneratorChaCha8::CHUNK_FACTOR,
            DtStreamType::ChaCha12 => GeneratorChaCha12::CHUNK_FACTOR,
            DtStreamType::ChaCha20 => GeneratorChaCha20::CHUNK_FACTOR,
            DtStreamType::Crc => GeneratorCrc::CHUNK_FACTOR,
        }
    }

    /// Get the size of the chunk returned by get_chunk(), in bytes.
    pub fn get_chunk_size(&self) -> usize {
        self.get_generator_outsize() * self.get_chunk_factor()
    }

    /// Get the next chunk from the thread.
    /// Returns None, if no chunk is available, yet.
    #[inline]
    pub fn get_chunk(&mut self) -> ah::Result<Option<DtStreamChunk>> {
        if self.is_active() {
            if self.is_thread_error() {
                Err(ah::format_err!("Generator stream thread aborted with an error."))
            } else if let Some(rx) = &self.rx {
                match rx.try_recv() {
                    Ok(chunk) => {
                        self.level.fetch_sub(1, Ordering::Relaxed);
                        Ok(Some(chunk))
                    },
                    Err(_) => Ok(None),
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
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

    impl DtStream {
        pub fn wait_chunk(&mut self) -> DtStreamChunk {
            loop {
                if let Some(chunk) = self.get_chunk().unwrap() {
                    break chunk;
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    fn run_base_test(algorithm: DtStreamType) {
        println!("stream base test");
        let cache = Rc::new(RefCell::new(BufCache::new()));
        let mut s = DtStream::new(algorithm, vec![1,2,3], false, 0, cache);
        s.activate(0).unwrap();
        assert!(s.is_active());

        assert_eq!(s.get_chunk_size(), s.get_generator_outsize() * s.get_chunk_factor());
        assert!(s.get_chunk_size() > 0);
        assert!(s.get_generator_outsize() > 0);
        assert!(s.get_chunk_factor() > 0);

        let mut results_first = vec![];
        for count in 0..5 {
            let chunk = s.wait_chunk();
            println!("{}: index={} data[0]={} (current level = {})",
                     count, chunk.index, chunk.data.as_ref().unwrap()[0], s.level.load(Ordering::Relaxed));
            results_first.push(chunk.data.as_ref().unwrap()[0]);
            assert_eq!(chunk.index, count);
        }
        match algorithm {
            DtStreamType::ChaCha8 => {
                assert_eq!(results_first, vec![66, 209, 254, 224, 203]);
            }
            DtStreamType::ChaCha12 => {
                assert_eq!(results_first, vec![200, 202, 12, 60, 234]);
            }
            DtStreamType::ChaCha20 => {
                assert_eq!(results_first, vec![206, 236, 87, 55, 170]);
            }
            DtStreamType::Crc => {
                assert_eq!(results_first, vec![108, 99, 114, 196, 213]);
            }
        }
    }

    fn run_offset_test(algorithm: DtStreamType) {
        println!("stream offset test");
        // a: start at chunk offset 0
        let cache = Rc::new(RefCell::new(BufCache::new()));
        let mut a = DtStream::new(algorithm, vec![1,2,3], false, 0, cache);
        a.activate(0).unwrap();

        // b: start at chunk offset 1
        let cache = Rc::new(RefCell::new(BufCache::new()));
        let mut b = DtStream::new(algorithm, vec![1,2,3], false, 0, cache);
        b.activate(a.get_chunk_size() as u64).unwrap();

        let achunk = a.wait_chunk();
        let bchunk = b.wait_chunk();
        assert!(achunk.data.as_ref().unwrap() != bchunk.data.as_ref().unwrap());
        let achunk = a.wait_chunk();
        assert!(achunk.data.as_ref().unwrap() == bchunk.data.as_ref().unwrap());
    }

    fn run_invert_test(algorithm: DtStreamType) {
        println!("stream invert test");
        let cache = Rc::new(RefCell::new(BufCache::new()));
        let mut a = DtStream::new(algorithm, vec![1,2,3], false, 0, cache);
        a.activate(0).unwrap();
        let cache = Rc::new(RefCell::new(BufCache::new()));
        let mut b = DtStream::new(algorithm, vec![1,2,3], true, 0, cache);
        b.activate(0).unwrap();

        let achunk = a.wait_chunk();
        let bchunk = b.wait_chunk();
        let inv_bchunk: Vec<u8> = bchunk.data.as_ref().unwrap().iter().map(|x| x ^ 0xFF).collect();
        assert!(achunk.data.as_ref().unwrap() != bchunk.data.as_ref().unwrap());
        assert!(achunk.data.as_ref().unwrap() == &inv_bchunk);
    }

    #[test]
    fn test_chacha8() {
        let alg = DtStreamType::ChaCha8;
        run_base_test(alg);
        run_offset_test(alg);
        run_invert_test(alg);
    }

    #[test]
    fn test_chacha12() {
        let alg = DtStreamType::ChaCha12;
        run_base_test(alg);
        run_offset_test(alg);
        run_invert_test(alg);
    }

    #[test]
    fn test_chacha20() {
        let alg = DtStreamType::ChaCha20;
        run_base_test(alg);
        run_offset_test(alg);
        run_invert_test(alg);
    }

    #[test]
    fn test_crc() {
        let alg = DtStreamType::Crc;
        run_base_test(alg);
        run_offset_test(alg);
        run_invert_test(alg);
    }
}

// vim: ts=4 sw=4 expandtab
