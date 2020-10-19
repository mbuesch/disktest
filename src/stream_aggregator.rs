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

use crate::stream::{DtStream, DtStreamChunk};
use std::thread;
use std::time::Duration;

pub struct DtStreamAggChunk {
    pub data:       Vec<u8>,
}

pub struct DtStreamAgg {
    streams:        Vec<DtStream>,
    stream_chunks:  Vec<Option<DtStreamChunk>>,
    is_active:      bool,
}

impl DtStreamAgg {
    pub fn new(seed: &Vec<u8>,
               num_threads: usize) -> DtStreamAgg {

        assert!(num_threads > 0);
        assert!(num_threads <= std::u16::MAX as usize + 1);

        let mut streams = Vec::with_capacity(num_threads);
        let mut stream_chunks = Vec::with_capacity(num_threads);
        for i in 0..num_threads {
            streams.push(DtStream::new(seed, i as u16));
            stream_chunks.push(None);
        }

        DtStreamAgg {
            streams,
            stream_chunks,
            is_active: false,
        }
    }

    #[inline]
    pub fn get_chunksize(&self) -> usize {
        DtStream::CHUNKSIZE * self.streams.len()
    }

    pub fn activate(&mut self) {
        for stream in &mut self.streams {
            stream.activate();
        }
        self.is_active = true;
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn get_chunk(&mut self) -> Option<DtStreamAggChunk> {
        if self.is_active() {
            let nr_streams = self.streams.len();
            let mut count = 0;

            // Get the stream chunks, that we don't already have.
            for i in 0..nr_streams {
                match self.stream_chunks[i] {
                    None => {
                        if let Some(chunk) = self.streams[i].get_chunk() {
                            self.stream_chunks[i] = Some(chunk);
                            count += 1;
                        }
                    },
                    Some(_) => {
                        count += 1;
                    },
                }
            }

            // Do we have all stream chunks?
            assert!(count <= nr_streams);
            if count == nr_streams {
                // Build the aggregated chunk by chaining all stream chunks.
                let mut agg_chunk = DtStreamAggChunk {
                    data:   Vec::with_capacity(self.get_chunksize()),
                };
                for i in 0..nr_streams {
                    if let Some(chunk) = &self.stream_chunks[i] {
                        agg_chunk.data.extend(&chunk.data);
                    } else {
                        panic!("Internal error: No stream chunk.");
                    }
                    // Clear the stream chunk.
                    // It has been integrated into the aggregate chunk.
                    self.stream_chunks[i] = None;
                }
                return Some(agg_chunk);
            }
        }
        None
    }

    pub fn wait_chunk(&mut self) -> DtStreamAggChunk {
        if self.is_active() {
            loop {
                if let Some(chunk) = self.get_chunk() {
                    break chunk;
                }
                thread::sleep(Duration::from_millis(10));
            }
        } else {
            panic!("wait_chunk() called, but stream aggregator is stopped.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let num_threads = 2;
        let mut agg = DtStreamAgg::new(&vec![1,2,3], num_threads);
        agg.activate();
        assert_eq!(agg.is_active(), true);

        let mut prev_chunk: Option<DtStreamAggChunk> = None;

        for _ in 0..4 {
            // Generate the next chunk.
            let chunk = agg.wait_chunk();
            assert_eq!(chunk.data.len(), DtStream::CHUNKSIZE * num_threads);
            assert_eq!(agg.get_chunksize(), chunk.data.len());

            // Check if we have an even distribution.
            let mut avg = vec![0; 256];
            for i in 0..chunk.data.len() {
                let index = chunk.data[i] as usize;
                avg[index] += 1;
            }
            let expected_avg = agg.get_chunksize() / 256;
            let thres = (expected_avg as f32 * 0.95) as usize;
            for acount in &avg {
                assert!(*acount >= thres);
            }

            // Check if the streams are different.
            let mut equal = 0;
            let nr_check = DtStream::CHUNKSIZE;
            for i in 0..nr_check {
                // first stream
                let offs0 = i;
                // second stream
                let offs1 = DtStream::CHUNKSIZE + i;

                if chunk.data[offs0] == chunk.data[offs1] {
                    equal += 1;
                }
            }
            assert_ne!(equal, 0);
            let thres = (nr_check as f32 * 0.01) as usize;
            assert!(equal < thres);

            // Check if current chunk is different from previous chunk.
            if let Some(pchunk) = prev_chunk {
                let mut equal = 0;
                let nr_check = agg.get_chunksize();
                for i in 0..nr_check {
                    if chunk.data[i] == pchunk.data[i] {
                        equal += 1;
                    }
                }
                assert_ne!(equal, 0);
                let thres = (nr_check as f32 * 0.01) as usize;
                assert!(equal < thres);
            }

            prev_chunk = Some(chunk);
        }
    }
}

// vim: ts=4 sw=4 expandtab
