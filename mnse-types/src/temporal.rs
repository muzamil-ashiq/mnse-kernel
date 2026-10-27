//! Natural time parser for temporal state reconstruction and history replays.
//!
//! Parses relative and human-friendly time expressions into `DateTime<Utc>`:
//! - Relative offsets: "30m ago", "2 hours ago", "3 days ago", "1 week ago", "now"
//! - Times of day: "3pm", "3:30pm", "10am", "15:00"
//! - Combined dates: "today at 3pm", "yesterday at 4pm", "2 days ago at 10am"
//! - Standard ISO / RFC3339 formats: "2026-10-23T15:00:00Z", "2026-10-23 15:00:00"

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime, TimeZone, Utc};

/// Parse a human-friendly natural time expression into a UTC `DateTime`.
pub fn parse_natural_time(input: &str) -> Result<DateTime<Utc>, String> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return Err("Empty time string".into());
    }

    if s == "now" {
        return Ok(Utc::now());
    }

    // 1. Try parsing relative expressions (e.g., "30m ago", "2 hours ago", "3 days ago")
    if s.ends_with(" ago") || s.ends_with("ago") {
        let trimmed = s.trim_end_matches("ago").trim();
        return parse_relative_offset(trimmed);
    }

    // 2. Try parsing standard RFC3339 / ISO formats
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        if let Some(naive_dt) = dt.and_hms_opt(0, 0, 0) {
            return Ok(Utc.from_utc_datetime(&naive_dt));
        }
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&dt));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&dt));
    }

    // 3. Try parsing day-relative expressions: "today ...", "yesterday ...", "X days ago at ..."
    if s.starts_with("today") {
        let rem = s.trim_start_matches("today").trim();
        let rem = rem.trim_start_matches("at").trim();
        if rem.is_empty() {
            let today = Local::now().date_naive();
            return Ok(Local.from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap()).unwrap().with_timezone(&Utc));
        }
        let time = parse_time_of_day(rem)?;
        let local_dt = Local::now().date_naive().and_time(time);
        return match Local.from_local_datetime(&local_dt) {
            chrono::LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
            chrono::LocalResult::Ambiguous(dt, _) => Ok(dt.with_timezone(&Utc)),
            chrono::LocalResult::None => Err(format!("Invalid local time: {}", local_dt)),
        };
    }

    if s.starts_with("yesterday") {
        let rem = s.trim_start_matches("yesterday").trim();
        let rem = rem.trim_start_matches("at").trim();
        let yesterday_date = (Local::now() - Duration::days(1)).date_naive();
        let time = if rem.is_empty() {
            Local::now().time()
        } else {
            parse_time_of_day(rem)?
        };
        let local_dt = yesterday_date.and_time(time);
        return match Local.from_local_datetime(&local_dt) {
            chrono::LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
            chrono::LocalResult::Ambiguous(dt, _) => Ok(dt.with_timezone(&Utc)),
            chrono::LocalResult::None => Err(format!("Invalid local time: {}", local_dt)),
        };
    }

    // Check for "X days ago at <time>"
    if let Some((days_part, time_part)) = s.split_once(" at ") {
        if let Ok(days_dt) = parse_natural_time(days_part.trim()) {
            let time = parse_time_of_day(time_part.trim())?;
            let target_date = days_dt.with_timezone(&Local).date_naive();
            let local_dt = target_date.and_time(time);
            return match Local.from_local_datetime(&local_dt) {
                chrono::LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
                chrono::LocalResult::Ambiguous(dt, _) => Ok(dt.with_timezone(&Utc)),
                chrono::LocalResult::None => Err(format!("Invalid local time: {}", local_dt)),
            };
        }
    }

    // 4. Try parsing standalone time of day: "3pm", "3:30pm", "10am", "15:00"
    if let Ok(time) = parse_time_of_day(&s) {
        let now_local = Local::now();
        let today_dt = now_local.date_naive().and_time(time);
        let candidate = match Local.from_local_datetime(&today_dt) {
            chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
            chrono::LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
            chrono::LocalResult::None => return Err(format!("Invalid local time: {}", today_dt)),
        };

        // If time is later than current time today, assume user meant yesterday at that time
        if candidate > Utc::now() {
            let yesterday_dt = (now_local - Duration::days(1)).date_naive().and_time(time);
            return match Local.from_local_datetime(&yesterday_dt) {
                chrono::LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
                chrono::LocalResult::Ambiguous(dt, _) => Ok(dt.with_timezone(&Utc)),
                chrono::LocalResult::None => Err(format!("Invalid local time: {}", yesterday_dt)),
            };
        } else {
            return Ok(candidate);
        }
    }

    Err(format!("Could not parse natural time expression: '{}'. Examples: '3pm', 'yesterday at 4pm', '30m ago', '2 hours ago'", input))
}

/// Helper to parse relative duration strings: "30m", "2 hours", "3 days", "1 week"
fn parse_relative_offset(input: &str) -> Result<DateTime<Utc>, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let (num_str, unit) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else if parts.len() == 1 {
        // e.g. "30m", "2h", "3d"
        let s = parts[0];
        let idx = s.find(|c: char| c.is_alphabetic()).ok_or_else(|| format!("Invalid offset: '{}'", input))?;
        (&s[..idx], &s[idx..])
    } else {
        return Err(format!("Invalid relative offset: '{}'", input));
    };

    let count: i64 = num_str.parse().map_err(|_| format!("Invalid number '{}' in time offset", num_str))?;

    let duration = match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => Duration::seconds(count),
        "m" | "min" | "mins" | "minute" | "minutes" => Duration::minutes(count),
        "h" | "hr" | "hrs" | "hour" | "hours" => Duration::hours(count),
        "d" | "day" | "days" => Duration::days(count),
        "w" | "wk" | "wks" | "week" | "weeks" => Duration::weeks(count),
        _ => return Err(format!("Unknown time unit '{}' in offset", unit)),
    };

    Ok(Utc::now() - duration)
}

/// Helper to parse clock time strings: "3pm", "3:30pm", "10am", "15:00"
fn parse_time_of_day(input: &str) -> Result<NaiveTime, String> {
    let s = input.trim().to_lowercase().replace(' ', "");

    let (is_pm, is_am, time_str) = if s.ends_with("pm") {
        (true, false, s.trim_end_matches("pm"))
    } else if s.ends_with("am") {
        (false, true, s.trim_end_matches("am"))
    } else {
        (false, false, s.as_str())
    };

    let (hour, minute) = if let Some((h_str, m_str)) = time_str.split_once(':') {
        let h: u32 = h_str.parse().map_err(|_| format!("Invalid hour in '{}'", input))?;
        let m: u32 = m_str.parse().map_err(|_| format!("Invalid minute in '{}'", input))?;
        (h, m)
    } else {
        let h: u32 = time_str.parse().map_err(|_| format!("Invalid hour in '{}'", input))?;
        (h, 0)
    };

    let adjusted_hour = if is_pm {
        if hour == 12 { 12 } else { hour + 12 }
    } else if is_am {
        if hour == 12 { 0 } else { hour }
    } else {
        hour
    };

    NaiveTime::from_hms_opt(adjusted_hour, minute, 0)
        .ok_or_else(|| format!("Invalid time values: hour={}, minute={}", adjusted_hour, minute))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_now() {
        let dt = parse_natural_time("now").unwrap();
        let diff = (Utc::now() - dt).num_seconds().abs();
        assert!(diff <= 1);
    }

    #[test]
    fn test_parse_relative_offsets() {
        let dt30m = parse_natural_time("30m ago").unwrap();
        let diff_mins = (Utc::now() - dt30m).num_minutes();
        assert!((diff_mins - 30).abs() <= 1);

        let dt2h = parse_natural_time("2 hours ago").unwrap();
        let diff_hrs = (Utc::now() - dt2h).num_hours();
        assert_eq!(diff_hrs, 2);

        let dt3d = parse_natural_time("3 days ago").unwrap();
        let diff_days = (Utc::now() - dt3d).num_days();
        assert_eq!(diff_days, 3);
    }

    #[test]
    fn test_parse_clock_times() {
        let t1 = parse_time_of_day("3pm").unwrap();
        assert_eq!(t1, NaiveTime::from_hms_opt(15, 0, 0).unwrap());

        let t2 = parse_time_of_day("10:30am").unwrap();
        assert_eq!(t2, NaiveTime::from_hms_opt(10, 30, 0).unwrap());

        let t3 = parse_time_of_day("14:45").unwrap();
        assert_eq!(t3, NaiveTime::from_hms_opt(14, 45, 0).unwrap());
    }

    #[test]
    fn test_parse_today_and_yesterday() {
        let dt_today = parse_natural_time("today at 3pm").unwrap();
        let dt_yesterday = parse_natural_time("yesterday at 3pm").unwrap();
        let diff_days = (dt_today - dt_yesterday).num_days();
        assert_eq!(diff_days, 1);
    }
}
