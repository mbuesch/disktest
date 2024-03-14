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

use anyhow as ah;
use std::fmt::Write;
use std::time::Duration;

const EIB: u64 = 1024 * 1024 * 1024 * 1024 * 1024 * 1024;
const PIB: u64 = 1024 * 1024 * 1024 * 1024 * 1024;
const TIB: u64 = 1024 * 1024 * 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
const KIB: u64 = 1024;

const EIBM1: u64 = EIB - 1;
const PIBM1: u64 = PIB - 1;
const TIBM1: u64 = TIB - 1;
const GIBM1: u64 = GIB - 1;
const MIBM1: u64 = MIB - 1;
//const KIBM1: u64 = KIB - 1;

const EB: u64 = 1000 * 1000 * 1000 * 1000 * 1000 * 1000;
const PB: u64 = 1000 * 1000 * 1000 * 1000 * 1000;
const TB: u64 = 1000 * 1000 * 1000 * 1000;
const GB: u64 = 1000 * 1000 * 1000;
const MB: u64 = 1000 * 1000;
const KB: u64 = 1000;

const EBM1: u64 = EB - 1;
const PBM1: u64 = PB - 1;
const TBM1: u64 = TB - 1;
const GBM1: u64 = GB - 1;
const MBM1: u64 = MB - 1;

pub fn prettybytes(count: u64, binary: bool, decimal: bool, bytes: bool) -> String {
    let mut ret = String::new();

    if !binary && !decimal {
        return ret;
    }

    if count < KIB {
        let _ = write!(ret, "{} bytes", count);
        return ret;
    }

    if binary {
        let _ = match count {
            EIB..=u64::MAX => write!(ret, "{:.4} EiB", ((count / TIB) as f64) / (MIB as f64)),
            PIB..=EIBM1 => write!(ret, "{:.4} PiB", ((count / GIB) as f64) / (MIB as f64)),
            TIB..=PIBM1 => write!(ret, "{:.4} TiB", ((count / MIB) as f64) / (MIB as f64)),
            GIB..=TIBM1 => write!(ret, "{:.2} GiB", ((count / MIB) as f64) / (KIB as f64)),
            MIB..=GIBM1 => write!(ret, "{:.1} MiB", (count as f64) / (MIB as f64)),
            0..=MIBM1 => write!(ret, "{:.1} kiB", (count as f64) / (KIB as f64)),
        };
    }

    let paren = if !ret.is_empty() && (decimal || bytes) {
        ret.push_str(" (");
        true
    } else {
        false
    };

    if decimal {
        let _ = match count {
            EB..=u64::MAX => write!(ret, "{:.4} EB", ((count / TB) as f64) / (MB as f64)),
            PB..=EBM1 => write!(ret, "{:.4} PB", ((count / GB) as f64) / (MB as f64)),
            TB..=PBM1 => write!(ret, "{:.4} TB", ((count / MB) as f64) / (MB as f64)),
            GB..=TBM1 => write!(ret, "{:.2} GB", ((count / MB) as f64) / (KB as f64)),
            MB..=GBM1 => write!(ret, "{:.1} MB", (count as f64) / (MB as f64)),
            0..=MBM1 => write!(ret, "{:.1} kB", (count as f64) / (KB as f64)),
        };
    }

    if bytes {
        if decimal {
            ret.push_str(", ");
        }
        let _ = write!(ret, "{} bytes", count);
    }

    if paren {
        ret.push(')');
    }

    ret
}

fn try_one_parsebytes(s: &str, suffix: &str, factor: u64) -> ah::Result<u64> {
    let Some(s) = s.strip_suffix(suffix) else {
        return Err(ah::format_err!("Value suffix does not match."));
    };
    let s = s.trim();
    if let Ok(value) = s.parse::<u64>() {
        // Integer value.
        let Some(prod) = value.checked_mul(factor) else {
            return Err(ah::format_err!("Value integer overflow."));
        };
        Ok(prod)
    } else if let Ok(value) = s.parse::<f64>() {
        // Floating point value.
        let factor = factor as f64;
        if value.log2() + factor.log2() >= 61.0 {
            return Err(ah::format_err!("Value float overflow."));
        }
        Ok((value * factor).round() as u64)
    } else {
        Err(ah::format_err!("Value is neither integer nor float."))
    }
}

pub fn parsebytes(s: &str) -> ah::Result<u64> {
    let s = s.trim().to_lowercase();

    if let Ok(v) = try_one_parsebytes(&s, "eib", EIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "pib", PIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "tib", TIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "gib", GIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "mib", MIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "kib", KIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "e", EIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "p", PIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "t", TIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "g", GIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "m", MIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "k", KIB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "eb", EB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "pb", PB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "tb", TB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "gb", GB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "mb", MB) {
        Ok(v)
    } else if let Ok(v) = try_one_parsebytes(&s, "kb", KB) {
        Ok(v)
    } else if let Ok(v) = s.parse::<u64>() {
        // byte count w/o suffix.
        Ok(v)
    } else {
        Err(ah::format_err!("Cannot parse byte count: {}", s))
    }
}

pub trait Hhmmss {
    fn hhmmss(&self) -> String;
}

impl Hhmmss for Duration {
    fn hhmmss(&self) -> String {
        let secs = self.as_secs();
        let secs_lim = (99 * 60 * 60) + (59 * 60) + 59;
        let lim = if secs > secs_lim { ">" } else { "" };
        let secs = secs.min(secs_lim);
        let h = secs / (60 * 60);
        let rem = secs % (60 * 60);
        let m = rem / 60;
        let s = rem % 60;
        format!("{}{:02}h:{:02}m:{:02}s", lim, h, m, s)
    }
}

/// Fold a byte vector into a smaller byte vector using XOR operation.
/// If output_size is bigger than input.len(), the trailing bytes
/// will be filled with zeros.
pub fn fold(input: &[u8], output_size: usize) -> Vec<u8> {
    let mut output = vec![0; output_size];

    if output_size > 0 {
        for (i, data) in input.iter().enumerate() {
            output[i % output_size] ^= data;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prettybytes() {
        assert_eq!(prettybytes(42, true, true, false), "42 bytes");
        assert_eq!(
            prettybytes(42 * 1024, true, true, false),
            "42.0 kiB (43.0 kB)"
        );
        assert_eq!(
            prettybytes(42 * 1024 * 1024, true, true, false),
            "42.0 MiB (44.0 MB)"
        );
        assert_eq!(
            prettybytes(42 * 1024 * 1024 * 1024, true, true, false),
            "42.00 GiB (45.10 GB)"
        );
        assert_eq!(
            prettybytes(42 * 1024 * 1024 * 1024 * 1024, true, true, false),
            "42.0000 TiB (46.1795 TB)"
        );
        assert_eq!(
            prettybytes(42 * 1024 * 1024 * 1024 * 1024 * 1024, true, true, false),
            "42.0000 PiB (47.2878 PB)"
        );
        assert_eq!(
            prettybytes(
                2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024,
                true,
                true,
                false
            ),
            "2.0000 EiB (2.3058 EB)"
        );

        assert_eq!(prettybytes(42, true, false, false), "42 bytes");
        assert_eq!(prettybytes(42, false, true, false), "42 bytes");
        assert_eq!(prettybytes(42, false, false, false), "");

        assert_eq!(prettybytes(42 * 1024, true, false, false), "42.0 kiB");
        assert_eq!(prettybytes(42 * 1024, false, true, false), "43.0 kB");
        assert_eq!(prettybytes(42 * 1024, false, false, false), "");

        assert_eq!(
            prettybytes(42 * 1024, true, true, true),
            "42.0 kiB (43.0 kB, 43008 bytes)"
        );
        assert_eq!(
            prettybytes(42 * 1024, true, false, true),
            "42.0 kiB (43008 bytes)"
        );
    }

    #[test]
    fn test_parsebytes() {
        // No suffix.
        assert_eq!(parsebytes("42").unwrap(), 42);

        // Binary suffix, integer.
        assert_eq!(parsebytes("42kib").unwrap(), 42 * 1024);
        assert_eq!(parsebytes("42 mib").unwrap(), 42 * 1024 * 1024);
        assert_eq!(parsebytes(" 42 gib ").unwrap(), 42 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42Tib").unwrap(), 42 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(
            parsebytes("42PiB").unwrap(),
            42 * 1024 * 1024 * 1024 * 1024 * 1024
        );
        assert_eq!(
            parsebytes("2 EIB ").unwrap(),
            2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024
        );

        // Binary suffix, fractional.
        assert_eq!(parsebytes("42.5kib").unwrap(), 42 * 1024 + 1024 / 2);
        assert_eq!(
            parsebytes("42.5 mib").unwrap(),
            42 * 1024 * 1024 + 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes(" 42.5 gib ").unwrap(),
            42 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes("42.5Tib").unwrap(),
            42 * 1024 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes("42.5PiB").unwrap(),
            42 * 1024 * 1024 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 * 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes("1.5 EIB ").unwrap(),
            1024 * 1024 * 1024 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 * 1024 * 1024 * 1024 / 2
        );

        // Binary suffix, integer.
        assert_eq!(parsebytes("42k").unwrap(), 42 * 1024);
        assert_eq!(parsebytes("42 m").unwrap(), 42 * 1024 * 1024);
        assert_eq!(parsebytes(" 42 g ").unwrap(), 42 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42T").unwrap(), 42 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(
            parsebytes("42P").unwrap(),
            42 * 1024 * 1024 * 1024 * 1024 * 1024
        );
        assert_eq!(
            parsebytes("2 E ").unwrap(),
            2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024
        );

        // Binary suffix, fractional.
        assert_eq!(parsebytes("42.5k").unwrap(), 42 * 1024 + 1024 / 2);
        assert_eq!(
            parsebytes("42.5 m").unwrap(),
            42 * 1024 * 1024 + 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes(" 42.5 g ").unwrap(),
            42 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes("42.5T").unwrap(),
            42 * 1024 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes("42.5P").unwrap(),
            42 * 1024 * 1024 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 * 1024 * 1024 / 2
        );
        assert_eq!(
            parsebytes("1.5 E ").unwrap(),
            1024 * 1024 * 1024 * 1024 * 1024 * 1024 + 1024 * 1024 * 1024 * 1024 * 1024 * 1024 / 2
        );

        // Decimal suffix, integer.
        assert_eq!(parsebytes("42kb").unwrap(), 42 * 1000);
        assert_eq!(parsebytes("42 mb").unwrap(), 42 * 1000 * 1000);
        assert_eq!(parsebytes(" 42 gb ").unwrap(), 42 * 1000 * 1000 * 1000);
        assert_eq!(parsebytes("42Tb").unwrap(), 42 * 1000 * 1000 * 1000 * 1000);
        assert_eq!(
            parsebytes("42PB").unwrap(),
            42 * 1000 * 1000 * 1000 * 1000 * 1000
        );
        assert_eq!(
            parsebytes("2 EB ").unwrap(),
            2 * 1000 * 1000 * 1000 * 1000 * 1000 * 1000
        );

        // Decimal suffix, fractional.
        assert_eq!(parsebytes("42.5kb").unwrap(), 42 * 1000 + 1000 / 2);
        assert_eq!(
            parsebytes("42.5 mb").unwrap(),
            42 * 1000 * 1000 + 1000 * 1000 / 2
        );
        assert_eq!(
            parsebytes(" 42.5 gb ").unwrap(),
            42 * 1000 * 1000 * 1000 + 1000 * 1000 * 1000 / 2
        );
        assert_eq!(
            parsebytes("42.5Tb").unwrap(),
            42 * 1000 * 1000 * 1000 * 1000 + 1000 * 1000 * 1000 * 1000 / 2
        );
        assert_eq!(
            parsebytes("42.5PB").unwrap(),
            42 * 1000 * 1000 * 1000 * 1000 * 1000 + 1000 * 1000 * 1000 * 1000 * 1000 / 2
        );
        assert_eq!(
            parsebytes("1.5 EB ").unwrap(),
            1000 * 1000 * 1000 * 1000 * 1000 * 1000 + 1000 * 1000 * 1000 * 1000 * 1000 * 1000 / 2
        );
    }

    #[test]
    fn test_hhmmss() {
        assert_eq!(Duration::from_secs(0).hhmmss(), "00h:00m:00s");
        assert_eq!(
            Duration::from_secs((2 * 60 * 60) + (3 * 60) + 4).hhmmss(),
            "02h:03m:04s"
        );
        assert_eq!(
            Duration::from_secs((23 * 60 * 60) + (59 * 60) + 59).hhmmss(),
            "23h:59m:59s"
        );
        assert_eq!(
            Duration::from_secs((99 * 60 * 60) + (59 * 60) + 59).hhmmss(),
            "99h:59m:59s"
        );
        assert_eq!(
            Duration::from_secs((99 * 60 * 60) + (59 * 60) + 59 + 1).hhmmss(),
            ">99h:59m:59s"
        );
    }

    #[test]
    fn test_fold() {
        assert_eq!(fold(&[0x55, 0x55, 0xAA, 0xAA], 2), [0xFF, 0xFF]);
        assert_eq!(fold(&[0x55, 0x55, 0x55, 0x55], 2), [0x00, 0x00]);
        assert_eq!(fold(&[0x55, 0x55, 0xAA, 0x55], 2), [0xFF, 0x00]);
        assert_eq!(fold(&[0x55, 0x55, 0x55, 0xAA], 2), [0x00, 0xFF]);
        assert_eq!(
            fold(&[0x98, 0xB1, 0x5B, 0x47, 0x8F, 0xF7, 0x9C, 0x6F], 3),
            [0x43, 0x51, 0xAC]
        );
        assert_eq!(fold(&[0x12, 0x34, 0x56, 0x78], 4), [0x12, 0x34, 0x56, 0x78]);
        assert_eq!(
            fold(&[0x12, 0x34, 0x56, 0x78], 6),
            [0x12, 0x34, 0x56, 0x78, 0x00, 0x00]
        );
        assert_eq!(fold(&[0x12, 0x34, 0x56, 0x78], 0), []);
    }
}

// vim: ts=4 sw=4 expandtab
