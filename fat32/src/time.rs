//! FAT date/time encoding.
//!
//! Date: bits 15..9 year since 1980, 8..5 month (1..=12), 4..0 day (1..=31).
//! Time: bits 15..11 hours, 10..5 minutes, 4..0 seconds / 2. The creation time additionally
//! has a "tenths" byte counting 10 ms units (0..=199) for the odd second.

use crate::Timestamp;

/// The clock used until `set_time_source` is called.
pub(crate) fn default_now() -> Timestamp {
    Timestamp { year: 2026, month: 1, day: 1, hour: 0, minute: 0, second: 0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FatTime {
    pub date: u16,
    pub time: u16,
    pub tenths: u8,
}

fn is_leap(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

pub(crate) fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 31,
    }
}

/// Encodes a timestamp, clamping every field into the range FAT can represent.
pub(crate) fn encode(ts: Timestamp) -> FatTime {
    let year = ts.year.clamp(1980, 2107);
    let month = ts.month.clamp(1, 12);
    let day = ts.day.clamp(1, days_in_month(year, month));
    let hour = ts.hour.min(23);
    let minute = ts.minute.min(59);
    let second = ts.second.min(59);
    FatTime {
        date: ((year - 1980) << 9) | ((month as u16) << 5) | day as u16,
        time: ((hour as u16) << 11) | ((minute as u16) << 5) | (second as u16 / 2),
        tenths: (second % 2) * 100,
    }
}

/// Decodes an on-disk date/time. A zero date means "not set" and yields `Timestamp::default()`.
pub(crate) fn decode(date: u16, time: u16, tenths: u8) -> Timestamp {
    if date == 0 {
        return Timestamp::default();
    }
    let extra = if (100..200).contains(&tenths) { 1 } else { 0 };
    Timestamp {
        year: 1980 + (date >> 9),
        month: ((date >> 5) & 0x0F) as u8,
        day: (date & 0x1F) as u8,
        hour: ((time >> 11) & 0x1F) as u8,
        minute: ((time >> 5) & 0x3F) as u8,
        second: (((time & 0x1F) * 2) as u8 + extra).min(59),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Timestamp {
        Timestamp { year, month, day, hour, minute, second }
    }

    #[test]
    fn round_trip() {
        for t in [
            ts(1980, 1, 1, 0, 0, 0),
            ts(2026, 1, 1, 0, 0, 0),
            ts(2024, 2, 29, 23, 59, 58),
            ts(2107, 12, 31, 23, 59, 59),
            ts(2000, 6, 15, 12, 30, 45),
        ] {
            let f = encode(t);
            assert_eq!(decode(f.date, f.time, f.tenths), t, "{:?}", t);
        }
    }

    #[test]
    fn known_encoding() {
        // 2026-01-01 00:00:00
        let f = encode(default_now());
        assert_eq!(f.date, (46 << 9) | (1 << 5) | 1);
        assert_eq!(f.time, 0);
        assert_eq!(f.tenths, 0);
        let f = encode(ts(2021, 7, 4, 13, 14, 15));
        assert_eq!(f.time, (13 << 11) | (14 << 5) | 7);
        assert_eq!(f.tenths, 100);
    }

    #[test]
    fn clamping() {
        let f = encode(ts(1970, 0, 0, 30, 70, 90));
        assert_eq!(decode(f.date, f.time, f.tenths), ts(1980, 1, 1, 23, 59, 59));
        let f = encode(ts(3000, 13, 40, 0, 0, 0));
        assert_eq!(decode(f.date, f.time, f.tenths), ts(2107, 12, 31, 0, 0, 0));
        let f = encode(ts(2023, 2, 31, 0, 0, 0));
        assert_eq!(decode(f.date, f.time, f.tenths).day, 28);
        assert_eq!(decode(0, 1234, 0), Timestamp::default());
    }
}
