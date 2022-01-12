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

use std::sync::mpsc::{channel, Sender, Receiver};
use std::collections::HashMap;

pub struct BufCache {
    snd:    HashMap<usize, Sender<Vec<u8>>>,
}

impl BufCache {
    pub fn new() -> BufCache {
        BufCache {
            snd: HashMap::new(),
        }
    }

    pub fn new_consumer(&mut self, cons_id: usize) -> BufCacheCons {
        let (snd, rcv) = channel();
        self.snd.insert(cons_id, snd);
        BufCacheCons {
            rcv,
        }
    }

    pub fn push(&mut self, cons_id: usize, buf: Vec<u8>) {
        if let Some(snd) = self.snd.get(&cons_id) {
            snd.send(buf).ok();
        } else {
            panic!("BufCache: Consumer {} does not exist.", cons_id);
        }
    }
}

pub struct BufCacheCons {
    rcv:    Receiver<Vec<u8>>,
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

// vim: ts=4 sw=4 expandtab
