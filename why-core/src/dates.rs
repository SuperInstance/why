//! Minimal YYYY-MM-DD date arithmetic. No external dependencies: the
//! format's clock semantics must stay deterministic and portable.
//!
//! Fixed-width ISO dates compare correctly as plain strings, so expiry
//! checks are string comparisons; only grace-period arithmetic needs the
//! civil-date algorithms below (Howard Hinnant's days_from_civil /
//! civil_from_days).

pub fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: &[u8]| r.iter().all(|c| c.is_ascii_digit());
    if !digits(&b[0..4]) || !digits(&b[5..7]) || !digits(&b[8..10]) {
        return false;
    }
    let y: i64 = s[0..4].parse().unwrap();
    let m: u32 = s[5..7].parse().unwrap();
    let d: u32 = s[8..10].parse().unwrap();
    if !(1..=12).contains(&m) {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    (1..=days_in_month[(m - 1) as usize]).contains(&d)
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m as i64 + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Inverse of `days_from_civil`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Add `n` days to a YYYY-MM-DD string. Returns None for invalid input.
pub fn add_days(date: &str, n: i64) -> Option<String> {
    if !is_valid_date(date) {
        return None;
    }
    let y: i64 = date[0..4].parse().ok()?;
    let m: u32 = date[5..7].parse().ok()?;
    let d: u32 = date[8..10].parse().ok()?;
    let (yy, mm, dd) = civil_from_days(days_from_civil(y, m, d) + n);
    Some(format!("{yy:04}-{mm:02}-{dd:02}"))
}

/// Today as YYYY-MM-DD from the system clock (UTC). None if the clock is
/// before the Unix epoch (should not happen).
pub fn today() -> Option<String> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity() {
        assert!(is_valid_date("2026-10-31"));
        assert!(!is_valid_date("2026-1-31"));
        assert!(!is_valid_date("2026-13-01"));
        assert!(!is_valid_date("not-a-date"));
        assert!(!is_valid_date("2026/10/31"));
    }

    #[test]
    fn add_days_basic() {
        assert_eq!(add_days("2026-01-01", 30).unwrap(), "2026-01-31");
        assert_eq!(add_days("2026-01-31", 1).unwrap(), "2026-02-01");
        assert_eq!(add_days("2026-10-31", 1).unwrap(), "2026-11-01");
        assert_eq!(add_days("2026-12-31", 1).unwrap(), "2027-01-01");
    }

    #[test]
    fn add_days_leap() {
        assert_eq!(add_days("2024-02-28", 1).unwrap(), "2024-02-29");
        assert_eq!(add_days("2026-02-28", 1).unwrap(), "2026-03-01");
    }

    #[test]
    fn add_days_invalid() {
        assert!(add_days("2026-02-30", 1).is_none());
        assert!(add_days("bad", 1).is_none());
    }
}
