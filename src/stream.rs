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

use crate::hasher::{HasherSHA512, HasherCRC, NextHash};
use crate::kdf::kdf;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub enum DtStreamType {
    CRC,
    SHA512,
}

pub struct DtStreamChunk {
    pub index: u64,
    pub data: Vec<u8>,
}

struct DtStreamWorker {
    stype:          DtStreamType,
    seed:           Vec<u8>,
    thread_id:      u32,
    abort:          Arc<AtomicBool>,
    level:          Arc<AtomicIsize>,
    tx:             Sender<DtStreamChunk>,
    index:          u64,
}

impl DtStreamWorker {
    const LEVEL_THRES: isize = 8;

    fn new(stype:       DtStreamType,
           seed:        &Vec<u8>,
           thread_id:   u32,
           tx:          Sender<DtStreamChunk>,
           abort:       Arc<AtomicBool>,
           level:       Arc<AtomicIsize>) -> DtStreamWorker {

        DtStreamWorker {
            stype,
            seed: seed.to_vec(),
            thread_id,
            abort,
            level,
            tx,
            index: 0,
        }
    }

    fn worker(&mut self) {
        // Calculate the per-thread-seed from the global seed.
        let thread_seed = kdf(&self.seed, self.thread_id);

        // Construct the hashing algorithm.
        let mut hasher: Box<dyn NextHash> = match self.stype {
            DtStreamType::SHA512 => Box::new(HasherSHA512::new(&thread_seed)),
            DtStreamType::CRC    => Box::new(HasherCRC::new(&thread_seed)),
        };

        // Run the hasher work loop.
        while !self.abort.load(Ordering::Relaxed) {
            if self.level.load(Ordering::Relaxed) < DtStreamWorker::LEVEL_THRES {

                let mut chunk = DtStreamChunk {
                    data: Vec::with_capacity(hasher.get_size() * DtStream::CHUNKFACTOR),
                    index: self.index,
                };
                self.index += 1;

                // Get the next chunk from the hasher.
                hasher.next_chunk(&mut chunk.data, DtStream::CHUNKFACTOR);

                // Send the chunk to the main thread.
                if let Ok(()) = self.tx.send(chunk) {
                    self.level.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                // The chunk buffer is full. Wait... */
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

pub struct DtStream {
    stype:          DtStreamType,
    seed:           Vec<u8>,
    thread_id:      u32,
    level:          Arc<AtomicIsize>,
    rx:             Option<Receiver<DtStreamChunk>>,
    thread_join:    RefCell<Option<thread::JoinHandle<()>>>,
    abort_thread:   Arc<AtomicBool>,
    is_active:      bool,
}

impl DtStream {
    pub const CHUNKFACTOR: usize = 1024 * 10;

    pub fn new(stype: DtStreamType,
               seed: &Vec<u8>,
               thread_id: u32) -> DtStream {

        let abort_thread = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicIsize::new(0));
        DtStream {
            stype,
            seed: seed.to_vec(),
            thread_id,
            level,
            rx: None,
            thread_join: RefCell::new(None),
            abort_thread,
            is_active: false,
        }
    }

    pub fn get_chunksize(&self) -> usize {
        match self.stype {
            DtStreamType::SHA512 => HasherSHA512::OUTSIZE * DtStream::CHUNKFACTOR,
            DtStreamType::CRC => HasherCRC::OUTSIZE * DtStream::CHUNKFACTOR,
        }
    }

    fn stop(&mut self) {
        self.is_active = false;
        self.abort_thread.store(true, Ordering::Release);
        if let Some(thread_join) = self.thread_join.replace(None) {
            thread_join.join().unwrap();
        }
        self.abort_thread.store(false, Ordering::Release);
    }

    fn start(&mut self) {
        self.abort_thread.store(false, Ordering::Release);
        self.level.store(0, Ordering::Release);
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let mut w = DtStreamWorker::new(self.stype,
                                        &self.seed,
                                        self.thread_id,
                                        tx,
                                        Arc::clone(&self.abort_thread),
                                        Arc::clone(&self.level));
        let thread_join = thread::spawn(move || w.worker());
        self.thread_join.replace(Some(thread_join));
        self.is_active = true;
    }

    pub fn activate(&mut self) {
        self.stop();
        self.start();
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

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
            DtStreamType::SHA512 => {
                assert_eq!(results_first, vec![226, 143, 221, 30, 59]);
            }
            DtStreamType::CRC => {
                assert_eq!(results_first, vec![132, 133, 170, 226, 104]);
            }
        }
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
