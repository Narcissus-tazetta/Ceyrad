/// Formats seconds as `0:53` / `12:04` / `1:02:30`.
pub fn format_time(seconds: f64) -> String {
    let total = seconds.round().max(0.0) as i64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_under_an_hour() {
        assert_eq!(format_time(53.0), "0:53");
        assert_eq!(format_time(168.0), "2:48");
        assert_eq!(format_time(724.0), "12:04");
    }

    #[test]
    fn formats_over_an_hour() {
        assert_eq!(format_time(3725.0), "1:02:05");
        assert_eq!(format_time(3750.0), "1:02:30");
    }

    #[test]
    fn clamps_negative_to_zero() {
        assert_eq!(format_time(-5.0), "0:00");
    }

    #[test]
    fn rounds_to_nearest_second() {
        assert_eq!(format_time(59.6), "1:00");
    }
}
