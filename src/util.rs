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

pub fn prettybyte(count: u64) -> String {
    if count >= 1024 * 1024 * 1024 * 1024 {
        return format!("{:.4} TiB", ((count / (1024 * 1024)) as f64) / (1024.0 * 1024.0));
    } else if count >= 1024 * 1024 * 1024 {
        return format!("{:.2} GiB", ((count / (1024 * 1024)) as f64) / 1024.0);
    } else if count >= 1024 * 1024 {
        return format!("{:.1} MiB", (count as f64) / (1024.0 * 1024.0));
    } else if count >= 1024 {
        return format!("{:.1} kiB", (count as f64) / 1024.0);
    } else {
        return format!("{} Bytes", count);
    }
}

// vim: ts=4 sw=4 expandtab
