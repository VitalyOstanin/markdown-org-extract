use chrono::{Datelike, NaiveDate, TimeZone};
use chrono_tz::Tz;

use crate::timestamp::{parse_org_timestamp, timestamp_in_range, timestamp_matches_date};
use crate::types::{Task, TaskType};

pub fn filter_agenda(mut tasks: Vec<Task>, mode: &str, date: Option<&str>, from: Option<&str>, to: Option<&str>, tz: &str) -> Result<Vec<Task>, Box<dyn std::error::Error>> {
    let tz: Tz = tz.parse()?;
    
    match mode {
        "day" => {
            let target_date = if let Some(date_str) = date {
                NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?
            } else {
                tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive()
            };
            tasks.retain(|t| task_matches_date(t, &target_date));
        }
        "week" => {
            let (start_date, end_date) = if let (Some(from_str), Some(to_str)) = (from, to) {
                (
                    NaiveDate::parse_from_str(from_str, "%Y-%m-%d")?,
                    NaiveDate::parse_from_str(to_str, "%Y-%m-%d")?,
                )
            } else {
                get_current_week(&tz)
            };
            tasks.retain(|t| task_in_range(t, &start_date, &end_date));
        }
        "tasks" => {
            tasks.retain(|t| matches!(t.task_type, Some(TaskType::Todo)));
            tasks.sort_by_key(|t| t.priority.as_ref().map(|p| p.order()).unwrap_or(999));
        }
        _ => return Err("Invalid agenda mode. Use: day, week, tasks".into()),
    }
    Ok(tasks)
}

fn get_current_week(tz: &Tz) -> (NaiveDate, NaiveDate) {
    let today = tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive();
    let weekday = today.weekday();
    let days_from_monday = weekday.num_days_from_monday();
    let monday = today - chrono::Duration::days(days_from_monday as i64);
    let sunday = monday + chrono::Duration::days(6);
    (monday, sunday)
}

fn task_matches_date(task: &Task, target_date: &NaiveDate) -> bool {
    if let Some(ref ts) = task.timestamp {
        if let Some(parsed) = parse_org_timestamp(ts, None) {
            return timestamp_matches_date(&parsed, target_date);
        }
    }
    false
}

fn task_in_range(task: &Task, start: &NaiveDate, end: &NaiveDate) -> bool {
    if let Some(ref ts) = task.timestamp {
        if let Some(parsed) = parse_org_timestamp(ts, None) {
            return timestamp_in_range(&parsed, start, end);
        }
    }
    false
}
