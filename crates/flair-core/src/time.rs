//! Minimal UTC date formatting (backup names, export manifests), without a
//! date library.

use std::time::{SystemTime, UNIX_EPOCH};

/// A UTC date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UtcDateTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl UtcDateTime {
    pub fn now() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self::from_unix(i64::try_from(secs).unwrap_or(0))
    }

    /// Converts Unix seconds (days-from-civil algorithm by H. Hinnant).
    pub fn from_unix(secs: i64) -> Self {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        // All the values below are small and non-negative.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Self {
            year,
            month: month as u32,
            day: day as u32,
            hour: (rem / 3600) as u32,
            minute: (rem / 60 % 60) as u32,
            second: (rem % 60) as u32,
        }
    }

    /// `2026-11-26T14:12:00Z`
    pub fn iso8601(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `2026-11-26`
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `20261126-141200`, sortable, safe in file names.
    pub fn compact(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(UtcDateTime::from_unix(0).iso8601(), "1970-01-01T00:00:00Z");
        assert_eq!(
            UtcDateTime::from_unix(1_700_000_000).iso8601(),
            "2023-11-14T22:13:20Z"
        );
        assert_eq!(UtcDateTime::from_unix(951_782_400).date(), "2000-02-29");
        assert_eq!(
            UtcDateTime::from_unix(1_798_761_599).compact(),
            "20261231-235959"
        );
    }
}
