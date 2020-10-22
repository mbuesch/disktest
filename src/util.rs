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

const EIB: u64 = 1024 * 1024 * 1024 * 1024 * 1024 * 1024;
const PIB: u64 = 1024 * 1024 * 1024 * 1024 * 1024;
const TIB: u64 = 1024 * 1024 * 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
const KIB: u64 = 1024;

const EB: u64 = 1000 * 1000 * 1000 * 1000 * 1000 * 1000;
const PB: u64 = 1000 * 1000 * 1000 * 1000 * 1000;
const TB: u64 = 1000 * 1000 * 1000 * 1000;
const GB: u64 = 1000 * 1000 * 1000;
const MB: u64 = 1000 * 1000;
const KB: u64 = 1000;

pub fn prettybyte(count: u64) -> String {
    return if count >= EIB {
        format!("{:.4} EiB ({:.4} EB)",
                ((count / TIB) as f64) / (MIB as f64),
                ((count / TB) as f64) / (MB as f64))
    } else if count >= PIB {
        format!("{:.4} PiB ({:.4} PB)",
                ((count / GIB) as f64) / (MIB as f64),
                ((count / GB) as f64) / (MB as f64))
    } else if count >= TIB {
        format!("{:.4} TiB ({:.4} TB)",
                ((count / MIB) as f64) / (MIB as f64),
                ((count / MB) as f64) / (MB as f64))
    } else if count >= GIB {
        format!("{:.2} GiB ({:.2} GB)",
                ((count / MIB) as f64) / (KIB as f64),
                ((count / MB) as f64) / (KB as f64))
    } else if count >= MIB {
        format!("{:.1} MiB ({:.1} MB)",
                (count as f64) / (MIB as f64),
                (count as f64) / (MB as f64))
    } else if count >= KIB {
        format!("{:.1} kiB ({:.1} kB)",
                (count as f64) / (KIB as f64),
                (count as f64) / (KB as f64))
    } else {
        format!("{} bytes", count)
    }
}

pub fn parsebytes(s: &str) -> Result<u64, <u64 as std::str::FromStr>::Err> {
    let s = s.trim().to_lowercase();

    if let Some(s) = s.strip_suffix("eib") {
        Ok(s.trim().parse::<u64>()? * EIB)
    } else if let Some(s) = s.strip_suffix("pib") {
        Ok(s.trim().parse::<u64>()? * PIB)
    } else if let Some(s) = s.strip_suffix("tib") {
        Ok(s.trim().parse::<u64>()? * TIB)
    } else if let Some(s) = s.strip_suffix("gib") {
        Ok(s.trim().parse::<u64>()? * GIB)
    } else if let Some(s) = s.strip_suffix("mib") {
        Ok(s.trim().parse::<u64>()? * MIB)
    } else if let Some(s) = s.strip_suffix("kib") {
        Ok(s.trim().parse::<u64>()? * KIB)

    } else if let Some(s) = s.strip_suffix("e") {
        Ok(s.trim().parse::<u64>()? * EIB)
    } else if let Some(s) = s.strip_suffix("p") {
        Ok(s.trim().parse::<u64>()? * PIB)
    } else if let Some(s) = s.strip_suffix("t") {
        Ok(s.trim().parse::<u64>()? * TIB)
    } else if let Some(s) = s.strip_suffix("g") {
        Ok(s.trim().parse::<u64>()? * GIB)
    } else if let Some(s) = s.strip_suffix("m") {
        Ok(s.trim().parse::<u64>()? * MIB)
    } else if let Some(s) = s.strip_suffix("k") {
        Ok(s.trim().parse::<u64>()? * KIB)

    } else if let Some(s) = s.strip_suffix("eb") {
        Ok(s.trim().parse::<u64>()? * EB)
    } else if let Some(s) = s.strip_suffix("pb") {
        Ok(s.trim().parse::<u64>()? * PB)
    } else if let Some(s) = s.strip_suffix("tb") {
        Ok(s.trim().parse::<u64>()? * TB)
    } else if let Some(s) = s.strip_suffix("gb") {
        Ok(s.trim().parse::<u64>()? * GB)
    } else if let Some(s) = s.strip_suffix("mb") {
        Ok(s.trim().parse::<u64>()? * MB)
    } else if let Some(s) = s.strip_suffix("kb") {
        Ok(s.trim().parse::<u64>()? * KB)

    } else {
        s.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prettybyte() {
        assert_eq!(prettybyte(42),
                   "42 bytes");
        assert_eq!(prettybyte(42 * 1024),
                   "42.0 kiB (43.0 kB)");
        assert_eq!(prettybyte(42 * 1024 * 1024),
                   "42.0 MiB (44.0 MB)");
        assert_eq!(prettybyte(42 * 1024 * 1024 * 1024),
                   "42.00 GiB (45.10 GB)");
        assert_eq!(prettybyte(42 * 1024 * 1024 * 1024 * 1024),
                   "42.0000 TiB (46.1795 TB)");
        assert_eq!(prettybyte(42 * 1024 * 1024 * 1024 * 1024 * 1024),
                   "42.0000 PiB (47.2878 PB)");
        assert_eq!(prettybyte(2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024),
                   "2.0000 EiB (2.3058 EB)");
    }

    #[test]
    fn test_parsebytes() {
        assert_eq!(parsebytes("42").unwrap(),
                   42);

        assert_eq!(parsebytes("42kib").unwrap(),
                   42 * 1024);
        assert_eq!(parsebytes("42 mib").unwrap(),
                   42 * 1024 * 1024);
        assert_eq!(parsebytes(" 42 gib ").unwrap(),
                   42 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42Tib").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42PiB").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("2 EIB ").unwrap(),
                   2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024);

        assert_eq!(parsebytes("42k").unwrap(),
                   42 * 1024);
        assert_eq!(parsebytes("42 m").unwrap(),
                   42 * 1024 * 1024);
        assert_eq!(parsebytes(" 42 g ").unwrap(),
                   42 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42T").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42P").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("2 E ").unwrap(),
                   2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024);

        assert_eq!(parsebytes("42kb").unwrap(),
                   42 * 1000);
        assert_eq!(parsebytes("42 mb").unwrap(),
                   42 * 1000 * 1000);
        assert_eq!(parsebytes(" 42 gb ").unwrap(),
                   42 * 1000 * 1000 * 1000);
        assert_eq!(parsebytes("42Tb").unwrap(),
                   42 * 1000 * 1000 * 1000 * 1000);
        assert_eq!(parsebytes("42PB").unwrap(),
                   42 * 1000 * 1000 * 1000 * 1000 * 1000);
        assert_eq!(parsebytes("2 EB ").unwrap(),
                   2 * 1000 * 1000 * 1000 * 1000 * 1000 * 1000);
    }
}

// vim: ts=4 sw=4 expandtab
