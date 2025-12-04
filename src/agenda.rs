use chrono::{Datelike, NaiveDate, TimeZone};
use chrono_tz::Tz;

use crate::timestamp::parse_org_timestamp;
use crate::types::{DayAgenda, Task, TaskType, TaskWithOffset};

/// Agenda output format
#[derive(Debug)]
pub enum AgendaOutput {
    Days(Vec<DayAgenda>),
    Tasks(Vec<Task>),
}

/// Filter tasks based on agenda mode
pub fn filter_agenda(
    tasks: Vec<Task>,
    mode: &str,
    date: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    tz: &str,
) -> Result<AgendaOutput, Box<dyn std::error::Error>> {
    let tz: Tz = tz
        .parse()
        .map_err(|_| format!("Invalid timezone: {tz}. Use IANA timezone names (e.g., 'Europe/Moscow', 'UTC')"))?;

    let today = tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive();

    match mode {
        "day" => {
            let target_date = if let Some(date_str) = date {
                NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .map_err(|e| format!("Invalid date format '{date_str}': {e}. Use YYYY-MM-DD"))?
            } else {
                today
            };
            Ok(AgendaOutput::Days(vec![build_day_agenda(tasks, target_date, today)]))
        }
        "week" => {
            let (start_date, end_date) = if let (Some(from_str), Some(to_str)) = (from, to) {
                let start = NaiveDate::parse_from_str(from_str, "%Y-%m-%d")
                    .map_err(|e| format!("Invalid 'from' date '{from_str}': {e}. Use YYYY-MM-DD"))?;
                let end = NaiveDate::parse_from_str(to_str, "%Y-%m-%d")
                    .map_err(|e| format!("Invalid 'to' date '{to_str}': {e}. Use YYYY-MM-DD"))?;
                
                if start > end {
                    return Err(format!("Start date {from_str} is after end date {to_str}").into());
                }
                
                (start, end)
            } else {
                get_current_week(&tz)
            };
            
            Ok(AgendaOutput::Days(build_week_agenda(tasks, start_date, end_date, today)))
        }
        "tasks" => {
            let mut filtered: Vec<Task> = tasks
                .into_iter()
                .filter(|t| matches!(t.task_type, Some(TaskType::Todo)))
                .collect();
            filtered.sort_by_key(|t| t.priority.as_ref().map(|p| p.order()).unwrap_or(999));
            Ok(AgendaOutput::Tasks(filtered))
        }
        _ => Err(format!("Invalid agenda mode '{mode}'. Valid modes: 'day', 'week', 'tasks'").into()),
    }
}

/// Build agenda for a single day
fn build_day_agenda(tasks: Vec<Task>, day_date: NaiveDate, current_date: NaiveDate) -> DayAgenda {
    let mut agenda = DayAgenda::new(day_date);
    let is_today = day_date == current_date;
    
    for task in tasks {
        if let Some(ref ts) = task.timestamp {
            if let Some(parsed) = parse_org_timestamp(ts, None) {
                let task_date = parsed.date;
                let days_diff = (task_date - day_date).num_days();
                
                let task_with_offset = TaskWithOffset {
                    task,
                    days_offset: if days_diff != 0 { Some(days_diff) } else { None },
                };
                
                if task_date == day_date {
                    // Tasks scheduled for this day
                    if task_with_offset.task.timestamp_time.is_some() {
                        agenda.scheduled_timed.push(task_with_offset);
                    } else {
                        agenda.scheduled_no_time.push(task_with_offset);
                    }
                } else if days_diff < 0 && is_today {
                    // Overdue tasks (only show on current date)
                    agenda.overdue.push(task_with_offset);
                } else if days_diff > 0 {
                    // Upcoming tasks
                    agenda.upcoming.push(task_with_offset);
                }
            }
        }
    }
    
    // Sort overdue: oldest first
    agenda.overdue.sort_by_key(|t| t.days_offset);
    
    // Sort scheduled_timed: earliest time first
    agenda.scheduled_timed.sort_by(|a, b| {
        a.task.timestamp_time.cmp(&b.task.timestamp_time)
    });
    
    // Sort upcoming: nearest first
    agenda.upcoming.sort_by_key(|t| t.days_offset);
    
    agenda
}

/// Build agenda for a week
fn build_week_agenda(tasks: Vec<Task>, start_date: NaiveDate, end_date: NaiveDate, current_date: NaiveDate) -> Vec<DayAgenda> {
    let mut result = Vec::new();
    let mut current = start_date;
    
    while current <= end_date {
        result.push(build_day_agenda(tasks.clone(), current, current_date));
        current += chrono::Duration::days(1);
    }
    
    result
}

/// Get current week (Monday to Sunday) in the given timezone
fn get_current_week(tz: &Tz) -> (NaiveDate, NaiveDate) {
    let today = tz
        .from_utc_datetime(&chrono::Utc::now().naive_utc())
        .date_naive();
    let weekday = today.weekday();
    let days_from_monday = weekday.num_days_from_monday();
    let monday = today - chrono::Duration::days(days_from_monday as i64);
    let sunday = monday + chrono::Duration::days(6);
    (monday, sunday)
}
