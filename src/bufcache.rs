// -*- coding: utf-8 -*-
//
// disktest - Storage tester
//
// Copyright 2020-2024 Michael Büsch <m@bues.ch>
//
// Licensed under the Apache License version 2.0
// or the MIT license, at your option.
// SPDX-License-Identifier: Apache-2.0 OR MIT
//

use crate::disktest::DisktestQuiet;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};

pub struct BufCache {
    snd: HashMap<u32, Sender<Vec<u8>>>,
    quiet_level: DisktestQuiet,
}

impl BufCache {
    pub fn new(quiet_level: DisktestQuiet) -> BufCache {
        BufCache {
            snd: HashMap::new(),
            quiet_level,
        }
    }

    pub fn new_consumer(&mut self, cons_id: u32) -> BufCacheCons {
        let (snd, rcv) = channel();
        self.snd.insert(cons_id, snd);
        BufCacheCons { rcv }
    }

    pub fn push(&mut self, cons_id: u32, buf: Vec<u8>) {
        let Some(snd) = self.snd.get(&cons_id) else {
            panic!("BufCache: Consumer {} does not exist.", cons_id);
        };
        if let Err(e) = snd.send(buf) {
            if self.quiet_level < DisktestQuiet::NoWarn {
                eprintln!("BufCache: Failed to send: {}", e);
            }
        }
    }
}

pub struct BufCacheCons {
    rcv: Receiver<Vec<u8>>,
}

impl BufCacheCons {
    pub fn pull(&mut self, buf_len: usize) -> Vec<u8> {
        let mut buf = match self.rcv.try_recv() {
            Ok(buf) => buf,
            Err(_) => Vec::with_capacity(buf_len),
        };
        if buf.len() != buf_len {
            buf.resize(buf_len, 0);
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bufcache() {
        let mut cache = BufCache::new(DisktestQuiet::Normal);
        let mut cons0 = cache.new_consumer(42);
        let mut cons1 = cache.new_consumer(43);

        let buf = cons0.pull(4);
        assert_eq!(buf.len(), 4);
        assert_eq!(buf, vec![0, 0, 0, 0]);

        cache.push(42, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let buf = cons0.pull(4);
        assert_eq!(buf.len(), 4);
        assert_eq!(buf, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        let buf = cons0.pull(4);
        assert_eq!(buf.len(), 4);
        assert_eq!(buf, vec![0, 0, 0, 0]);

        cache.push(43, vec![0xCA, 0xFE, 0xAF, 0xFE]);
        let buf = cons0.pull(4);
        assert_eq!(buf.len(), 4);
        assert_eq!(buf, vec![0, 0, 0, 0]);
        let buf = cons1.pull(4);
        assert_eq!(buf.len(), 4);
        assert_eq!(buf, vec![0xCA, 0xFE, 0xAF, 0xFE]);
    }

    #[test]
    #[should_panic(expected = "Consumer 42 does not exist")]
    fn test_bufcache_cons_invalid() {
        let mut cache = BufCache::new(DisktestQuiet::Normal);
        cache.push(42, vec![]);
    }
}

// vim: ts=4 sw=4 expandtab
