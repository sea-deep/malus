//! Tabular-friendly time formatting helpers.

/// Formats milliseconds into `M:SS` or `MM:SS`.
pub fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}

/// Formats remaining time in milliseconds into `-M:SS` or `-MM:SS`.
pub fn format_remaining_time(pos_ms: u64, dur_ms: u64) -> String {
    if dur_ms <= pos_ms {
        "-0:00".to_string()
    } else {
        let remaining_ms = dur_ms - pos_ms;
        let total_secs = remaining_ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("-{mins}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(16_000), "0:16");
        assert_eq!(format_time(71_000), "1:11");
        assert_eq!(format_time(337_600), "5:37");
    }

    #[test]
    fn test_format_remaining_time() {
        assert_eq!(format_remaining_time(16_000, 71_000), "-0:55");
        assert_eq!(format_remaining_time(0, 180_000), "-3:00");
        assert_eq!(format_remaining_time(180_000, 180_000), "-0:00");
        assert_eq!(format_remaining_time(200_000, 180_000), "-0:00");
    }
}
