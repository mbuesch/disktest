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

use std::thread;
use std::time::Duration;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicBool, Ordering};
use std::cell::RefCell;

use crate::hasher::Hasher;

pub struct DtStreamChunk {
    pub index: u64,
    pub data: Vec<u8>,
}

struct DtStreamWorker {
    hasher:         Hasher,
    run:            Arc<AtomicBool>,
    abort:          Arc<AtomicBool>,
    level:          Arc<AtomicIsize>,
    tx:             Sender<DtStreamChunk>,
    index:          u64,
}

impl DtStreamWorker {
    const LEVEL_THRES: isize = 8;

    fn new(seed: &Vec<u8>,
           serial:  u16,
           tx:      Sender<DtStreamChunk>,
           run:     Arc<AtomicBool>,
           abort:   Arc<AtomicBool>,
           level:   Arc<AtomicIsize>) -> DtStreamWorker {

        DtStreamWorker {
            hasher: Hasher::new(seed, serial),
            run,
            abort,
            level,
            tx,
            index: 0,
        }
    }

    fn worker(&mut self) {
        while !self.abort.load(Ordering::Relaxed) {
            if (self.level.load(Ordering::Relaxed) < DtStreamWorker::LEVEL_THRES) &&
               self.run.load(Ordering::Relaxed) {

                let mut chunk = DtStreamChunk {
                    data: Vec::with_capacity(DtStream::CHUNKSIZE),
                    index: self.index,
                };
                self.index += 1;

                for _ in 0..(DtStream::CHUNKSIZE / Hasher::OUTSIZE) {
                    let next_hash = self.hasher.next();
                    chunk.data.extend(next_hash);
                }
                if let Ok(()) = self.tx.send(chunk) {
                    self.level.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

pub struct DtStream {
    level:          Arc<AtomicIsize>,
    rx:             Receiver<DtStreamChunk>,
    thread_join:    RefCell<Option<thread::JoinHandle<()>>>,
    run_thread:     Arc<AtomicBool>,
    abort_thread:   Arc<AtomicBool>,
}

impl DtStream {
    pub const CHUNKSIZE: usize = Hasher::OUTSIZE * 1024 * 10;

    pub fn new(seed: &Vec<u8>,
               serial: u16) -> DtStream {

        let abort_thread = Arc::new(AtomicBool::new(false));
        let run_thread = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicIsize::new(0));
        let (tx, rx) = channel();
        let mut w = DtStreamWorker::new(seed,
                                        serial,
                                        tx,
                                        Arc::clone(&run_thread),
                                        Arc::clone(&abort_thread),
                                        Arc::clone(&level));
        let thread_join = thread::spawn(move || w.worker());
        DtStream {
            level,
            rx,
            thread_join: RefCell::new(Some(thread_join)),
            run_thread,
            abort_thread,
        }
    }

    pub fn activate(&mut self) {
        self.run_thread.store(true, Ordering::Release);
    }

    pub fn is_active(&self) -> bool {
        self.run_thread.load(Ordering::Relaxed) &&
        !self.abort_thread.load(Ordering::Relaxed)
    }

    pub fn get_chunk(&mut self) -> Option<DtStreamChunk> {
        if self.is_active() {
            match self.rx.try_recv() {
                Ok(chunk) => {
                    self.level.fetch_sub(1, Ordering::Relaxed);
                    Some(chunk)
                },
                Err(_) => None,
            }
        } else {
            None
        }
    }

    pub fn wait_chunk(&mut self) -> DtStreamChunk {
        if self.is_active() {
            loop {
                if let Some(chunk) = self.get_chunk() {
                    break chunk;
                }
                thread::sleep(Duration::from_millis(1));
            }
        } else {
            panic!("wait_chunk() called, but stream is stopped.");
        }
    }

    #[cfg(test)]
    pub fn get_level(&self) -> isize {
        self.level.load(Ordering::Relaxed)
    }
}

impl Drop for DtStream {
    fn drop(&mut self) {
        self.abort_thread.store(true, Ordering::Release);
        if let Some(thread_join) = self.thread_join.replace(None) {
            thread_join.join().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let mut s = DtStream::new(&vec![1,2,3], 0);
        s.activate();
        assert_eq!(s.is_active(), true);

        let mut count = 0;
        while count < 5 {
            if let Some(chunk) = s.get_chunk() {
                println!("{}: index={} data[0]={} (current level = {})",
                         count, chunk.index, chunk.data[0], s.get_level());
                assert_eq!(chunk.index, count);
                assert_eq!(chunk.data[0], [84, 31, 194, 246, 107][chunk.index as usize]);
                count += 1;
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

// vim: ts=4 sw=4 expandtab
