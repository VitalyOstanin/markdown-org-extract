use chrono::{Datelike, NaiveDate, TimeZone};
use chrono_tz::Tz;

use crate::error::AppError;
use crate::timestamp::parse_org_timestamp;
use crate::types::{DayAgenda, Task, TaskType, TaskWithOffset};

const DEADLINE_WARNING_DAYS: i64 = 14;

#[derive(Debug)]
pub enum AgendaOutput {
    Days(Vec<DayAgenda>),
    Tasks(Vec<Task>),
}

pub fn filter_agenda(
    tasks: Vec<Task>,
    mode: &str,
    date: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    tz: &str,
    current_date_override: Option<&str>,
) -> Result<AgendaOutput, AppError> {
    let tz: Tz = tz
        .parse()
        .map_err(|_| AppError::InvalidTimezone(tz.to_string()))?;

    let today = if let Some(date_str) = current_date_override {
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| AppError::InvalidDate(format!("current-date '{date_str}': {e}")))?
    } else {
        tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive()
    };

    match mode {
        "day" => {
            let target_date = if let Some(date_str) = date {
                NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .map_err(|e| AppError::InvalidDate(format!("date '{date_str}': {e}")))?
            } else {
                today
            };
            Ok(AgendaOutput::Days(vec![build_day_agenda(&tasks, target_date, today)]))
        }
        "week" => {
            let (start_date, end_date) = if let (Some(from_str), Some(to_str)) = (from, to) {
                let start = NaiveDate::parse_from_str(from_str, "%Y-%m-%d")
                    .map_err(|e| AppError::InvalidDate(format!("from '{from_str}': {e}")))?;
                let end = NaiveDate::parse_from_str(to_str, "%Y-%m-%d")
                    .map_err(|e| AppError::InvalidDate(format!("to '{to_str}': {e}")))?;
                
                if start > end {
                    return Err(AppError::DateRange(format!("Start date {from_str} is after end date {to_str}")));
                }
                
                (start, end)
            } else {
                get_current_week(&tz)
            };
            
            Ok(AgendaOutput::Days(build_week_agenda(&tasks, start_date, end_date, today)))
        }
        "tasks" => {
            let mut filtered: Vec<Task> = tasks
                .into_iter()
                .filter(|t| matches!(t.task_type, Some(TaskType::Todo)))
                .collect();
            filtered.sort_by_key(|t| t.priority.as_ref().map(|p| p.order()).unwrap_or(999));
            Ok(AgendaOutput::Tasks(filtered))
        }
        _ => Err(AppError::InvalidDate(format!("Invalid agenda mode '{mode}'"))),
    }
}

fn build_day_agenda(tasks: &[Task], day_date: NaiveDate, current_date: NaiveDate) -> DayAgenda {
    let mut agenda = DayAgenda::new(day_date);
    let is_today = day_date == current_date;
    
    for task in tasks {
        if let Some(ref ts) = task.timestamp {
            if let Some(parsed) = parse_org_timestamp(ts, None) {
                if let Some(ref repeater) = parsed.repeater {
                    handle_repeating_task(task, &parsed, repeater, day_date, current_date, &mut agenda);
                } else {
                    handle_non_repeating_task(task, &parsed, day_date, is_today, &mut agenda);
                }
            }
        }
    }
    
    agenda.overdue.sort_by_key(|t| t.days_offset);
    agenda.scheduled_timed.sort_by(|a, b| a.task.timestamp_time.cmp(&b.task.timestamp_time));
    agenda.upcoming.sort_by_key(|t| t.days_offset);
    
    agenda
}

fn handle_non_repeating_task(
    task: &Task,
    parsed: &crate::timestamp::ParsedTimestamp,
    day_date: NaiveDate,
    is_today: bool,
    agenda: &mut DayAgenda,
) {
    let task_date = parsed.date;
    let days_diff = (task_date - day_date).num_days();
    let is_done = matches!(task.task_type, Some(TaskType::Done));
    
    let days_offset = if days_diff != 0 { Some(days_diff) } else { None };
    
    if task_date == day_date {
        let task_with_offset = TaskWithOffset {
            task: task.clone(),
            days_offset,
        };
        if task_with_offset.task.timestamp_time.is_some() {
            agenda.scheduled_timed.push(task_with_offset);
        } else {
            agenda.scheduled_no_time.push(task_with_offset);
        }
    } else if days_diff < 0 && is_today && !is_done {
        agenda.overdue.push(create_task_without_time(task, days_offset));
    } else if days_diff > 0 {
        // Show upcoming only for DEADLINE within warning period
        if let Some(ref ts_type) = task.timestamp_type {
            if ts_type == "DEADLINE" && days_diff <= DEADLINE_WARNING_DAYS {
                agenda.upcoming.push(create_task_without_time(task, days_offset));
            }
        }
    }
}

fn create_task_without_time(task: &Task, days_offset: Option<i64>) -> TaskWithOffset {
    let mut task_copy = task.clone();
    task_copy.timestamp_time = None;
    task_copy.timestamp_end_time = None;
    TaskWithOffset {
        task: task_copy,
        days_offset,
    }
}

fn handle_repeating_task(
    task: &Task,
    parsed: &crate::timestamp::ParsedTimestamp,
    repeater: &crate::timestamp::Repeater,
    day_date: NaiveDate,
    current_date: NaiveDate,
    agenda: &mut DayAgenda,
) {
    use crate::timestamp::next_occurrence;
    
    let base_date = parsed.date;
    
    // For repeating tasks, calculate the occurrence for this day
    if day_date >= base_date {
        // Check if this day matches a repeating occurrence
        if is_occurrence_day(base_date, repeater, day_date) {
            // On occurrence day, no offset
            let task_with_offset = TaskWithOffset {
                task: task.clone(),
                days_offset: None,
            };
            
            // Repeating task on its occurrence day
            if task_with_offset.task.timestamp_time.is_some() {
                agenda.scheduled_timed.push(task_with_offset);
            } else {
                agenda.scheduled_no_time.push(task_with_offset);
            }
        }
        
        // Also show as overdue on current date if there are missed occurrences
        // But only if today is NOT an occurrence day (to avoid duplicates)
        if day_date == current_date && day_date > base_date && !is_occurrence_day(base_date, repeater, current_date) {
            let last_occurrence = find_last_occurrence(base_date, repeater, current_date);
            if let Some(last_occ) = last_occurrence {
                if last_occ < current_date {
                    let days_diff = (last_occ - current_date).num_days();
                    let mut task_copy = task.clone();
                    task_copy.timestamp_time = None;
                    task_copy.timestamp_end_time = None;
                    let task_with_offset = TaskWithOffset {
                        task: task_copy,
                        days_offset: Some(days_diff),
                    };
                    agenda.overdue.push(task_with_offset);
                }
            }
        }
    } else {
        // Future occurrence - show as upcoming only for DEADLINE within warning period
        if let Some(ref ts_type) = task.timestamp_type {
            if ts_type == "DEADLINE" {
                if let Some(next_occ) = next_occurrence(base_date, repeater, day_date) {
                    if next_occ > day_date {
                        let days_diff = (next_occ - day_date).num_days();
                        if days_diff <= DEADLINE_WARNING_DAYS {
                            let mut task_copy = task.clone();
                            task_copy.timestamp_time = None;
                            task_copy.timestamp_end_time = None;
                            let task_with_offset = TaskWithOffset {
                                task: task_copy,
                                days_offset: Some(days_diff),
                            };
                            agenda.upcoming.push(task_with_offset);
                        }
                    }
                }
            }
        }
    }
}

fn is_occurrence_day(base_date: NaiveDate, repeater: &crate::timestamp::Repeater, check_date: NaiveDate) -> bool {
    use crate::timestamp::RepeaterUnit;
    
    if check_date < base_date {
        return false;
    }
    
    let days_diff = (check_date - base_date).num_days();
    
    match repeater.unit {
        RepeaterUnit::Day => days_diff % (repeater.value as i64) == 0,
        RepeaterUnit::Week => days_diff % ((repeater.value * 7) as i64) == 0,
        RepeaterUnit::Hour => days_diff == 0, // Same day for hourly repeats
        RepeaterUnit::Month | RepeaterUnit::Year => {
            // For month/year, check if it's the right day of month
            check_date.day() == base_date.day()
        }
    }
}

fn find_last_occurrence(base_date: NaiveDate, repeater: &crate::timestamp::Repeater, before_date: NaiveDate) -> Option<NaiveDate> {
    use crate::timestamp::RepeaterUnit;
    
    let mut current = base_date;
    let days = match repeater.unit {
        RepeaterUnit::Day => repeater.value as i64,
        RepeaterUnit::Week => (repeater.value * 7) as i64,
        RepeaterUnit::Hour => 1,
        _ => return Some(base_date),
    };
    
    while current + chrono::Duration::days(days) < before_date {
        current += chrono::Duration::days(days);
    }
    
    Some(current)
}

/// Build agenda for a week
fn build_week_agenda(tasks: &[Task], start_date: NaiveDate, end_date: NaiveDate, current_date: NaiveDate) -> Vec<DayAgenda> {
    let mut result = Vec::new();
    let mut current = start_date;
    
    while current <= end_date {
        if current < current_date {
            result.push(DayAgenda::new(current));
        } else {
            result.push(build_day_agenda(tasks, current, current_date));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Priority;

    fn create_test_task_with_type(date_str: &str, time: Option<&str>, task_type: TaskType, ts_type: &str) -> Task {
        let timestamp = if let Some(t) = time {
            format!("{ts_type}: <{date_str} {t}>")
        } else {
            format!("{ts_type}: <{date_str}>")
        };
        
        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test task".to_string(),
            content: String::new(),
            task_type: Some(task_type),
            priority: None,
            created: None,
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some(ts_type.to_string()),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            timestamp_end_time: None,
        }
    }

    fn create_test_task(date_str: &str, time: Option<&str>, task_type: TaskType) -> Task {
        create_test_task_with_type(date_str, time, task_type, "SCHEDULED")
    }

    #[test]
    fn test_scheduled_future_not_shown_as_upcoming() {
        let tasks = vec![
            create_test_task("2024-12-10 Tue", None, TaskType::Todo),
            create_test_task("2024-12-20 Fri", None, TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 0, "SCHEDULED tasks in future should not appear as upcoming");
        assert_eq!(agenda.scheduled_timed.len(), 0);
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_deadline_within_14_days_shown_as_upcoming() {
        let tasks = vec![
            create_test_task_with_type("2024-12-10 Tue", None, TaskType::Todo, "DEADLINE"),
            create_test_task_with_type("2024-12-15 Sun", None, TaskType::Todo, "DEADLINE"),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 2, "DEADLINE within 14 days should appear as upcoming");
        assert_eq!(agenda.upcoming[0].days_offset, Some(5));
        assert_eq!(agenda.upcoming[1].days_offset, Some(10));
    }

    #[test]
    fn test_deadline_beyond_14_days_not_shown() {
        let tasks = vec![
            create_test_task_with_type("2024-12-20 Fri", None, TaskType::Todo, "DEADLINE"),
            create_test_task_with_type("2025-01-10 Fri", None, TaskType::Todo, "DEADLINE"),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 0, "DEADLINE beyond 14 days should not appear");
    }

    #[test]
    fn test_deadline_exactly_14_days_shown() {
        let tasks = vec![
            create_test_task_with_type("2024-12-19 Thu", None, TaskType::Todo, "DEADLINE"),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 1, "DEADLINE exactly 14 days away should appear");
        assert_eq!(agenda.upcoming[0].days_offset, Some(14));
    }

    #[test]
    fn test_deadline_15_days_not_shown() {
        let tasks = vec![
            create_test_task_with_type("2024-12-20 Fri", None, TaskType::Todo, "DEADLINE"),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 0, "DEADLINE 15 days away should not appear");
    }

    #[test]
    fn test_overdue_only_on_current_date() {
        let tasks = vec![
            create_test_task("2024-12-01 Sun", None, TaskType::Todo),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
        ];
        
        // Check on current date - should show overdue
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, current_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 2, "Overdue tasks should appear on current date");
        assert_eq!(agenda.overdue[0].days_offset, Some(-4));
        assert_eq!(agenda.overdue[1].days_offset, Some(-2));
        
        // Check on past date - should not show overdue
        let past_date = NaiveDate::from_ymd_opt(2024, 12, 2).unwrap();
        let agenda_past = build_day_agenda(&tasks, past_date, current_date);
        
        assert_eq!(agenda_past.overdue.len(), 0, "Overdue should not appear on past dates");
    }

    #[test]
    fn test_week_agenda_past_days_empty() {
        let tasks = vec![
            create_test_task("2024-12-02 Mon", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
            create_test_task("2024-12-05 Thu", Some("14:00"), TaskType::Todo),
        ];
        
        let start_date = NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(); // Monday
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap(); // Sunday
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(); // Thursday
        
        let week = build_week_agenda(&tasks, start_date, end_date, current_date);
        
        assert_eq!(week.len(), 7);
        
        // Monday (past) - empty
        assert_eq!(week[0].date, "2024-12-02");
        assert_eq!(week[0].scheduled_timed.len(), 0);
        assert_eq!(week[0].scheduled_no_time.len(), 0);
        
        // Tuesday (past) - empty
        assert_eq!(week[1].date, "2024-12-03");
        assert_eq!(week[1].scheduled_timed.len(), 0);
        assert_eq!(week[1].scheduled_no_time.len(), 0);
        
        // Wednesday (past) - empty
        assert_eq!(week[2].date, "2024-12-04");
        assert_eq!(week[2].scheduled_timed.len(), 0);
        
        // Thursday (current) - has tasks
        assert_eq!(week[3].date, "2024-12-05");
        assert_eq!(week[3].scheduled_timed.len(), 1);
        assert_eq!(week[3].overdue.len(), 2); // Monday and Tuesday tasks are overdue
        
        // Future days should have tasks if scheduled
        assert!(week[4].scheduled_timed.len() == 0); // Friday
    }

    #[test]
    fn test_build_day_agenda_scheduled_timed() {
        let tasks = vec![
            create_test_task("2024-12-05 Wed", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-05 Wed", Some("14:00"), TaskType::Todo),
            create_test_task("2024-12-05 Wed", None, TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_timed.len(), 2);
        assert_eq!(agenda.scheduled_no_time.len(), 1);
        assert_eq!(agenda.upcoming.len(), 0);
        assert_eq!(agenda.overdue.len(), 0);
        
        // Check time sorting
        assert_eq!(agenda.scheduled_timed[0].task.timestamp_time, Some("10:00".to_string()));
        assert_eq!(agenda.scheduled_timed[1].task.timestamp_time, Some("14:00".to_string()));
    }

    #[test]
    fn test_mixed_scheduled_and_deadline() {
        let tasks = vec![
            create_test_task("2024-12-10 Tue", None, TaskType::Todo), // SCHEDULED - not shown
            create_test_task_with_type("2024-12-10 Tue", None, TaskType::Todo, "DEADLINE"), // DEADLINE - shown
            create_test_task_with_type("2024-12-25 Wed", None, TaskType::Todo, "DEADLINE"), // DEADLINE too far - not shown
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 1, "Only DEADLINE within 14 days should appear");
        assert_eq!(agenda.upcoming[0].task.timestamp_type, Some("DEADLINE".to_string()));
    }

    fn create_test_task_with_repeater(date_str: &str, time: Option<&str>, repeater: &str, task_type: TaskType) -> Task {
        let timestamp = if let Some(t) = time {
            format!("SCHEDULED: <{date_str} {t} {repeater}>")
        } else {
            format!("SCHEDULED: <{date_str} {repeater}>")
        };
        
        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test task".to_string(),
            content: String::new(),
            task_type: Some(task_type),
            priority: None,
            created: None,
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some("SCHEDULED".to_string()),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            timestamp_end_time: None,
        }
    }

    fn create_test_task_with_repeater_deadline(date_str: &str, time: Option<&str>, repeater: &str, task_type: TaskType) -> Task {
        let timestamp = if let Some(t) = time {
            format!("DEADLINE: <{date_str} {t} {repeater}>")
        } else {
            format!("DEADLINE: <{date_str} {repeater}>")
        };
        
        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test task".to_string(),
            content: String::new(),
            task_type: Some(task_type),
            priority: None,
            created: None,
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some("DEADLINE".to_string()),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            timestamp_end_time: None,
        }
    }

    #[test]
    fn test_build_day_agenda_repeating_daily() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("10:00"), "+1d", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_timed.len(), 1);
        assert_eq!(agenda.scheduled_timed[0].task.timestamp_time, Some("10:00".to_string()));
    }

    #[test]
    fn test_build_day_agenda_repeating_not_occurrence_day() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", None, "+2d", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 4).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_timed.len(), 0);
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_build_day_agenda_repeating_weekly() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", None, "+1w", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 1);
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 9).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_build_day_agenda_repeating_every_2_days() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", None, "+2d", TaskType::Todo),
        ];
        
        let test_dates = vec![
            (NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(), true),
            (NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(), false),
            (NaiveDate::from_ymd_opt(2024, 12, 3).unwrap(), true),
            (NaiveDate::from_ymd_opt(2024, 12, 4).unwrap(), false),
            (NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(), true),
        ];
        
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        
        for (date, should_show) in test_dates {
            let agenda = build_day_agenda(&tasks, date, current_date);
            if should_show {
                assert_eq!(agenda.scheduled_no_time.len(), 1, "Failed for date {date}");
            } else {
                assert_eq!(agenda.scheduled_no_time.len(), 0, "Failed for date {date}");
            }
        }
    }

    #[test]
    fn test_overdue_repeating_task_on_non_occurrence_day() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("10:00"), "+2d", TaskType::Todo),
        ];
        
        // 2024-12-05 is NOT an occurrence day (+2d from 2024-12-01: 12-01, 12-03, 12-05 would be, but let's use 12-04)
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 4).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 4).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        // Should appear in overdue (missed occurrence on 12-03)
        assert!(agenda.overdue.len() > 0);
        assert_eq!(agenda.overdue[0].task.timestamp_time, None);
    }

    #[test]
    fn test_upcoming_repeating_task_has_no_time() {
        let tasks = vec![
            create_test_task_with_repeater_deadline("2024-12-10 Mon", Some("15:00"), "+1d", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 1);
        assert_eq!(agenda.upcoming[0].task.timestamp_time, None);
        assert_eq!(agenda.upcoming[0].days_offset, Some(5));
    }

    #[test]
    fn test_repeating_deadline_beyond_warning_not_shown() {
        let tasks = vec![
            create_test_task_with_repeater_deadline("2026-08-24 Mon", None, "+1y", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 0, "DEADLINE beyond 14 days should not appear in upcoming");
    }

    #[test]
    fn test_build_day_agenda_mixed_repeating_and_regular() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("10:00"), "+1d", TaskType::Todo),
            create_test_task("2024-12-05 Wed", Some("14:00"), TaskType::Todo),
            create_test_task_with_type("2024-12-06 Thu", None, TaskType::Todo, "DEADLINE"),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_timed.len(), 2);
        assert_eq!(agenda.upcoming.len(), 1); // Only DEADLINE
    }

    #[test]
    fn test_build_day_agenda_repeating_with_time_sorting() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("14:00"), "+1d", TaskType::Todo),
            create_test_task_with_repeater("2024-12-01 Sun", Some("09:00"), "+1d", TaskType::Todo),
            create_test_task("2024-12-05 Wed", Some("11:00"), TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_timed.len(), 3);
        assert_eq!(agenda.scheduled_timed[0].task.timestamp_time, Some("09:00".to_string()));
        assert_eq!(agenda.scheduled_timed[1].task.timestamp_time, Some("11:00".to_string()));
        assert_eq!(agenda.scheduled_timed[2].task.timestamp_time, Some("14:00".to_string()));
    }

    #[test]
    fn test_overdue_tasks_have_no_time() {
        let tasks = vec![
            create_test_task("2024-12-01 Mon", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-02 Tue", Some("14:00"), TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 2);
        assert_eq!(agenda.overdue[0].task.timestamp_time, None);
        assert_eq!(agenda.overdue[1].task.timestamp_time, None);
    }

    #[test]
    fn test_upcoming_deadline_tasks_have_no_time() {
        let tasks = vec![
            create_test_task_with_type("2024-12-06 Thu", Some("10:00"), TaskType::Todo, "DEADLINE"),
            create_test_task_with_type("2024-12-07 Fri", Some("14:00"), TaskType::Todo, "DEADLINE"),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 2);
        assert_eq!(agenda.upcoming[0].task.timestamp_time, None);
        assert_eq!(agenda.upcoming[1].task.timestamp_time, None);
    }

    #[test]
    fn test_repeating_task_on_occurrence_day_not_in_overdue() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("10:00"), "+1d", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        // Should appear in scheduled (it's an occurrence day)
        assert_eq!(agenda.scheduled_timed.len(), 1);
        assert_eq!(agenda.scheduled_timed[0].task.timestamp_time, Some("10:00".to_string()));
        assert_eq!(agenda.scheduled_timed[0].days_offset, None);
        
        // Should NOT appear in overdue (to avoid duplicate)
        assert_eq!(agenda.overdue.len(), 0);
    }

    #[test]
    fn test_repeating_task_no_overdue_if_not_missed() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-05 Wed", Some("10:00"), "+1d", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_timed.len(), 1);
        assert_eq!(agenda.overdue.len(), 0);
    }

    #[test]
    fn test_get_current_week() {
        let tz: Tz = "UTC".parse().unwrap();
        let (monday, sunday) = get_current_week(&tz);
        
        assert_eq!(monday.weekday(), chrono::Weekday::Mon);
        assert_eq!(sunday.weekday(), chrono::Weekday::Sun);
        assert_eq!((sunday - monday).num_days(), 6);
    }

    #[test]
    fn test_done_tasks_not_in_overdue() {
        let tasks = vec![
            create_test_task("2024-12-01 Sun", None, TaskType::Done),
            create_test_task("2024-12-02 Mon", Some("10:00"), TaskType::Done),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 1, "Only TODO tasks should appear in overdue");
        assert_eq!(agenda.overdue[0].task.task_type, Some(TaskType::Todo));
    }

    #[test]
    fn test_done_tasks_shown_on_their_date() {
        let tasks = vec![
            create_test_task("2024-12-05 Wed", None, TaskType::Done),
            create_test_task("2024-12-05 Wed", Some("14:00"), TaskType::Done),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 1, "DONE task without time should appear on its date");
        assert_eq!(agenda.scheduled_timed.len(), 1, "DONE task with time should appear on its date");
        assert_eq!(agenda.overdue.len(), 0, "DONE tasks should not appear in overdue");
    }

    #[test]
    fn test_done_deadline_not_in_overdue() {
        let tasks = vec![
            create_test_task_with_type("2024-12-01 Sun", None, TaskType::Done, "DEADLINE"),
            create_test_task_with_type("2024-12-02 Mon", None, TaskType::Todo, "DEADLINE"),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 1, "Only TODO deadline should appear in overdue");
        assert_eq!(agenda.overdue[0].task.task_type, Some(TaskType::Todo));
    }
}
