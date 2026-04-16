use chrono::DateTime;

use crate::types::{CategoryStats, ParsedTurn};
use std::collections::HashMap;

pub fn calculate_duration(first_ts: &str, last_ts: &str) -> f64 {
    if first_ts.is_empty() || last_ts.is_empty() {
        return 0.0;
    }

    let start = parse_timestamp(first_ts);
    let end = parse_timestamp(last_ts);

    match (start, end) {
        (Some(s), Some(e)) => {
            let dur = e.signed_duration_since(s);
            dur.num_seconds().max(0) as f64
        }
        _ => 0.0,
    }
}

fn parse_timestamp(ts: &str) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .or_else(|| {
            ts.parse::<DateTime<chrono::Utc>>()
                .ok()
                .map(|dt| dt.fixed_offset())
        })
}

pub fn assign_turn_durations(turns: &[ParsedTurn], category_map: &mut HashMap<String, CategoryStats>) {
    if turns.len() <= 1 {
        return;
    }

    let timestamps: Vec<Option<DateTime<chrono::FixedOffset>>> = turns
        .iter()
        .map(|t| {
            if t.timestamp.is_empty() {
                t.calls
                    .first()
                    .and_then(|c| parse_timestamp(&c.timestamp))
            } else {
                parse_timestamp(&t.timestamp)
            }
        })
        .collect();

    for i in 0..turns.len() {
        let current_ts = match &timestamps[i] {
            Some(ts) => ts,
            None => continue,
        };

        let duration = if i + 1 < turns.len() {
            if let Some(next_ts) = &timestamps[i + 1] {
                let dur = next_ts.signed_duration_since(current_ts);
                let secs = dur.num_seconds().max(0) as f64;
                secs.min(3600.0)
            } else {
                let last_call_ts = turns[i]
                    .calls
                    .last()
                    .and_then(|c| parse_timestamp(&c.timestamp));
                if let Some(last) = last_call_ts {
                    let dur = last.signed_duration_since(current_ts);
                    dur.num_seconds().max(0) as f64
                } else {
                    0.0
                }
            }
        } else {
            let last_call_ts = turns[i]
                .calls
                .last()
                .and_then(|c| parse_timestamp(&c.timestamp));
            if let Some(last) = last_call_ts {
                let dur = last.signed_duration_since(current_ts);
                dur.num_seconds().max(0) as f64
            } else {
                0.0
            }
        };

        if let Some(entry) = category_map.get_mut(&turns[i].category) {
            entry.duration_seconds += duration;
        }
    }
}

pub fn format_duration(seconds: f64) -> String {
    let total = seconds as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}
