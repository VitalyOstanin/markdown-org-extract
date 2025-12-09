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
            } else if let Some(date_str) = date {
                let target_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .map_err(|e| AppError::InvalidDate(format!("date '{date_str}': {e}")))?;
                get_week_for_date(target_date)
            } else {
                get_current_week(&tz)
            };
            
            Ok(AgendaOutput::Days(build_week_agenda(&tasks, start_date, end_date, today)))
        }
        "month" => {
            let (start_date, end_date) = if let (Some(from_str), Some(to_str)) = (from, to) {
                let start = NaiveDate::parse_from_str(from_str, "%Y-%m-%d")
                    .map_err(|e| AppError::InvalidDate(format!("from '{from_str}': {e}")))?;
                let end = NaiveDate::parse_from_str(to_str, "%Y-%m-%d")
                    .map_err(|e| AppError::InvalidDate(format!("to '{to_str}': {e}")))?;
                
                if start > end {
                    return Err(AppError::DateRange(format!("Start date {from_str} is after end date {to_str}")));
                }
                
                (start, end)
            } else if let Some(date_str) = date {
                let target_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .map_err(|e| AppError::InvalidDate(format!("date '{date_str}': {e}")))?;
                get_month_for_date(target_date)
            } else {
                get_current_month(&tz)
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
    
    // Show task on its scheduled date
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
        // Overdue only in today agenda
        agenda.overdue.push(create_task_without_time(task, days_offset));
    } else if days_diff > 0 && is_today {
        // Upcoming only in today agenda, only for DEADLINE within warning period
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
    use crate::timestamp::{closest_date, DatePreference};
    
    let base_date = parsed.date;
    let is_today = day_date == current_date;
    
    // Calculate deadline (last occurrence <= today) and repeat (next occurrence >= day_date)
    // Following org-mode logic from org-agenda.el
    let deadline = closest_date(base_date, current_date, DatePreference::Past, repeater);
    let repeat = if day_date <= current_date {
        deadline
    } else {
        closest_date(base_date, day_date, DatePreference::Future, repeater)
    };
    
    // Show task if:
    // 1. current == deadline (last occurrence day)
    // 2. current == repeat (next occurrence day)
    // 3. today? (for upcoming/overdue)
    if let (Some(deadline_date), Some(repeat_date)) = (deadline, repeat) {
        // Show on deadline or repeat day
        if day_date == deadline_date || day_date == repeat_date {
            let mut task_copy = task.clone();
            task_copy.timestamp_date = Some(day_date.format("%Y-%m-%d").to_string());
            
            // Update timestamp string with actual occurrence date
            if let Some(ref ts_type) = task.timestamp_type {
                let weekday = day_date.format("%a").to_string();
                let date_str = day_date.format("%Y-%m-%d").to_string();
                let time_part = if let Some(ref time) = task.timestamp_time {
                    format!(" {time}")
                } else {
                    String::new()
                };
                task_copy.timestamp = Some(format!("{}: <{} {}{} +{}{}>", 
                    ts_type, date_str, weekday, time_part, repeater.value, 
                    match repeater.unit {
                        crate::timestamp::RepeaterUnit::Day => "d",
                        crate::timestamp::RepeaterUnit::Week => "w",
                        crate::timestamp::RepeaterUnit::Month => "m",
                        crate::timestamp::RepeaterUnit::Year => "y",
                        crate::timestamp::RepeaterUnit::Hour => "h",
                        crate::timestamp::RepeaterUnit::Workday => "wd",
                    }
                ));
            }
            
            let task_with_offset = TaskWithOffset {
                task: task_copy,
                days_offset: None,
            };
            
            if task_with_offset.task.timestamp_time.is_some() {
                agenda.scheduled_timed.push(task_with_offset);
            } else {
                agenda.scheduled_no_time.push(task_with_offset);
            }
        }
        
        // Show as overdue in today agenda if deadline < today
        if is_today && deadline_date < current_date {
            // For workday repeaters, only show as overdue if current_date is a workday
            let should_show_overdue = if repeater.unit == crate::timestamp::RepeaterUnit::Workday {
                use crate::holidays::HolidayCalendar;
                if let Ok(calendar) = HolidayCalendar::load() {
                    calendar.is_workday(current_date)
                } else {
                    true
                }
            } else {
                true
            };
            
            if should_show_overdue {
                let days_diff = (deadline_date - current_date).num_days();
                let mut task_copy = task.clone();
                task_copy.timestamp_time = None;
                task_copy.timestamp_end_time = None;
                task_copy.timestamp_date = Some(deadline_date.format("%Y-%m-%d").to_string());
                
                // Update timestamp string with deadline date
                if let Some(ref ts_type) = task.timestamp_type {
                    let weekday = deadline_date.format("%a").to_string();
                    let date_str = deadline_date.format("%Y-%m-%d").to_string();
                    task_copy.timestamp = Some(format!("{}: <{} {} +{}{}>", 
                        ts_type, date_str, weekday, repeater.value, 
                        match repeater.unit {
                            crate::timestamp::RepeaterUnit::Day => "d",
                            crate::timestamp::RepeaterUnit::Week => "w",
                            crate::timestamp::RepeaterUnit::Month => "m",
                            crate::timestamp::RepeaterUnit::Year => "y",
                            crate::timestamp::RepeaterUnit::Hour => "h",
                            crate::timestamp::RepeaterUnit::Workday => "wd",
                        }
                    ));
                }
                
                let task_with_offset = TaskWithOffset {
                    task: task_copy,
                    days_offset: Some(days_diff),
                };
                agenda.overdue.push(task_with_offset);
            }
        }
        
        // Show as upcoming in today agenda for DEADLINE within warning period
        if is_today && repeat_date > current_date {
            if let Some(ref ts_type) = task.timestamp_type {
                if ts_type == "DEADLINE" {
                    let days_diff = (repeat_date - current_date).num_days();
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

#[allow(dead_code)]
fn find_last_occurrence_before(base_date: NaiveDate, repeater: &crate::timestamp::Repeater, before_date: NaiveDate) -> Option<NaiveDate> {
    use crate::timestamp::RepeaterUnit;
    
    // For workday repeaters, use next_occurrence to find the next occurrence from before_date - 1
    // If it equals before_date, then before_date is an occurrence day (handled separately)
    // Otherwise, the next occurrence is in the future
    if repeater.unit == RepeaterUnit::Workday {
        // Check if there's any occurrence between base_date and before_date
        let mut check_date = base_date;
        let mut last_found = None;
        
        // Limit iterations to prevent infinite loops
        let max_iterations = 1000;
        let mut iterations = 0;
        
        while check_date < before_date && iterations < max_iterations {
            if is_occurrence_day(base_date, repeater, check_date) {
                last_found = Some(check_date);
            }
            check_date += chrono::Duration::days(1);
            iterations += 1;
        }
        
        last_found
    } else {
        // For regular repeaters, calculate directly
        match repeater.unit {
            RepeaterUnit::Day | RepeaterUnit::Week | RepeaterUnit::Hour => {
                let mut current = base_date;
                let days = match repeater.unit {
                    RepeaterUnit::Day => repeater.value as i64,
                    RepeaterUnit::Week => (repeater.value * 7) as i64,
                    RepeaterUnit::Hour => 1,
                    _ => unreachable!(),
                };
                
                while current + chrono::Duration::days(days) < before_date {
                    current += chrono::Duration::days(days);
                }
                
                if current < before_date {
                    Some(current)
                } else {
                    None
                }
            }
            RepeaterUnit::Month | RepeaterUnit::Year => {
                use crate::timestamp::next_occurrence;
                
                // Find the next occurrence after before_date, then step back
                if let Some(next) = next_occurrence(base_date, repeater, before_date) {
                    if next > before_date {
                        use crate::timestamp::add_months;
                        let months_to_subtract = if repeater.unit == RepeaterUnit::Year {
                            -((repeater.value * 12) as i32)
                        } else {
                            -(repeater.value as i32)
                        };
                        
                        add_months(next, months_to_subtract)
                            .filter(|&last| last >= base_date && last < before_date)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[allow(dead_code)]
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
        RepeaterUnit::Year => {
            use chrono::Datelike;
            if check_date.day() != base_date.day() || check_date.month() != base_date.month() {
                return false;
            }
            let years_diff = check_date.year() - base_date.year();
            years_diff >= 0 && (years_diff as u32) % repeater.value == 0
        }
        RepeaterUnit::Month => {
            use chrono::Datelike;
            if check_date.day() != base_date.day() {
                return false;
            }
            let months_diff = (check_date.year() - base_date.year()) * 12 + (check_date.month() as i32 - base_date.month() as i32);
            months_diff >= 0 && (months_diff as u32) % repeater.value == 0
        }
        RepeaterUnit::Workday => {
            use crate::holidays::HolidayCalendar;
            if let Ok(calendar) = HolidayCalendar::load() {
                if !calendar.is_workday(check_date) {
                    return false;
                }
                if check_date == base_date {
                    return true;
                }
                let mut current = calendar.next_workday(base_date);
                let mut workday_count = 1u32;
                while current < check_date {
                    current = calendar.next_workday(current);
                    workday_count += 1;
                }
                if current == check_date {
                    workday_count % repeater.value == 0
                } else {
                    false
                }
            } else {
                false
            }
        }
    }
}

/// Build agenda for a week
fn build_week_agenda(tasks: &[Task], start_date: NaiveDate, end_date: NaiveDate, current_date: NaiveDate) -> Vec<DayAgenda> {
    let mut result = Vec::new();
    let mut current = start_date;
    
    while current <= end_date {
        result.push(build_day_agenda(tasks, current, current_date));
        current += chrono::Duration::days(1);
    }
    
    result
}

/// Get week boundaries (Monday to Sunday) for a specific date
fn get_week_for_date(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let weekday = date.weekday();
    let days_from_monday = weekday.num_days_from_monday();
    let monday = date - chrono::Duration::days(days_from_monday as i64);
    let sunday = monday + chrono::Duration::days(6);
    (monday, sunday)
}

/// Get current week (Monday to Sunday) in the given timezone
fn get_current_week(tz: &Tz) -> (NaiveDate, NaiveDate) {
    let today = tz
        .from_utc_datetime(&chrono::Utc::now().naive_utc())
        .date_naive();
    get_week_for_date(today)
}

/// Get month boundaries (first to last day) for a specific date
fn get_month_for_date(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first_day = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap();
    let last_day = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year(), 12, 31).unwrap()
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap() - chrono::Duration::days(1)
    };
    (first_day, last_day)
}

/// Get current month (first to last day) in the given timezone
fn get_current_month(tz: &Tz) -> (NaiveDate, NaiveDate) {
    let today = tz
        .from_utc_datetime(&chrono::Utc::now().naive_utc())
        .date_naive();
    get_month_for_date(today)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            clocks: None,
            total_clock_time: None,
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
        
        // Monday (past) - shows scheduled task on its day
        assert_eq!(week[0].date, "2024-12-02");
        assert_eq!(week[0].scheduled_timed.len(), 1);
        assert_eq!(week[0].scheduled_no_time.len(), 0);
        
        // Tuesday (past) - shows scheduled task on its day
        assert_eq!(week[1].date, "2024-12-03");
        assert_eq!(week[1].scheduled_timed.len(), 0);
        assert_eq!(week[1].scheduled_no_time.len(), 1);
        
        // Wednesday (past) - no tasks
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
            clocks: None,
            total_clock_time: None,
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
            clocks: None,
            total_clock_time: None,
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
            (NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(), false), // Past, not deadline day
            (NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(), false),
            (NaiveDate::from_ymd_opt(2024, 12, 3).unwrap(), false), // Past, not deadline day
            (NaiveDate::from_ymd_opt(2024, 12, 4).unwrap(), false),
            (NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(), true),  // deadline = 2024-12-05 (last occurrence <= today)
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
        
        // 2024-12-06 is NOT an occurrence day (+2d from 2024-12-01: 12-01, 12-03, 12-05)
        // Next occurrence is 12-05, which is in the past, so task is overdue
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        eprintln!("overdue: {:?}", agenda.overdue.len());
        eprintln!("scheduled_timed: {:?}", agenda.scheduled_timed.len());
        eprintln!("scheduled_no_time: {:?}", agenda.scheduled_no_time.len());
        
        // Should appear in overdue (next occurrence 12-05 is in the past)
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
    fn test_get_current_month() {
        let tz: Tz = "UTC".parse().unwrap();
        let (first, last) = get_current_month(&tz);
        
        assert_eq!(first.day(), 1);
        assert_eq!(first.month(), last.month());
        assert_eq!(first.year(), last.year());
        assert!(last.day() >= 28 && last.day() <= 31);
    }

    #[test]
    fn test_get_current_month_december() {
        // Test December specifically (has 31 days)
        let today = NaiveDate::from_ymd_opt(2024, 12, 15).unwrap();
        
        // Simulate getting month for December
        let first_day = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
        let last_day = NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap();
        
        assert_eq!(first_day, NaiveDate::from_ymd_opt(2024, 12, 1).unwrap());
        assert_eq!(last_day, NaiveDate::from_ymd_opt(2024, 12, 31).unwrap());
    }

    #[test]
    fn test_get_current_month_february_leap() {
        // Test February in leap year
        let first_day = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let last_day = NaiveDate::from_ymd_opt(2024, 3, 1).unwrap() - chrono::Duration::days(1);
        
        assert_eq!(first_day, NaiveDate::from_ymd_opt(2024, 2, 1).unwrap());
        assert_eq!(last_day, NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    }

    #[test]
    fn test_get_current_month_february_non_leap() {
        // Test February in non-leap year
        let first_day = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        let last_day = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap() - chrono::Duration::days(1);
        
        assert_eq!(first_day, NaiveDate::from_ymd_opt(2025, 2, 1).unwrap());
        assert_eq!(last_day, NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());
    }

    #[test]
    fn test_month_agenda_length() {
        let tasks = vec![
            create_test_task("2024-12-15 Sun", None, TaskType::Todo),
        ];
        
        let start_date = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        
        let month = build_week_agenda(&tasks, start_date, end_date, current_date);
        
        assert_eq!(month.len(), 31, "December should have 31 days");
        assert_eq!(month[0].date, "2024-12-01");
        assert_eq!(month[30].date, "2024-12-31");
    }

    #[test]
    fn test_month_agenda_past_days_empty() {
        let tasks = vec![
            create_test_task("2024-12-02 Mon", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
            create_test_task("2024-12-10 Tue", Some("14:00"), TaskType::Todo),
        ];
        
        let start_date = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        
        let month = build_week_agenda(&tasks, start_date, end_date, current_date);
        
        // Day 1 should be empty
        assert_eq!(month[0].scheduled_timed.len(), 0);
        assert_eq!(month[0].scheduled_no_time.len(), 0);
        
        // Day 2 should show scheduled task
        assert_eq!(month[1].scheduled_timed.len(), 1);
        
        // Day 3 should show scheduled task
        assert_eq!(month[2].scheduled_no_time.len(), 1);
        
        // Day 4 should be empty
        assert_eq!(month[3].scheduled_timed.len(), 0);
        
        // Current day should have overdue tasks
        assert_eq!(month[4].date, "2024-12-05");
        assert!(month[4].overdue.len() > 0, "Current day should have overdue tasks");
        
        // Future days should have scheduled tasks if applicable
        assert_eq!(month[9].scheduled_timed.len(), 1, "Day 10 should have scheduled task");
    }

    #[test]
    fn test_month_agenda_february() {
        let tasks = vec![
            create_test_task("2024-02-15 Thu", None, TaskType::Todo),
        ];
        
        let start_date = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(); // Leap year
        let current_date = NaiveDate::from_ymd_opt(2024, 2, 10).unwrap();
        
        let month = build_week_agenda(&tasks, start_date, end_date, current_date);
        
        assert_eq!(month.len(), 29, "February 2024 (leap year) should have 29 days");
        assert_eq!(month[0].date, "2024-02-01");
        assert_eq!(month[28].date, "2024-02-29");
    }

    #[test]
    fn test_month_agenda_custom_range() {
        let tasks = vec![
            create_test_task("2024-12-10 Tue", None, TaskType::Todo),
            create_test_task("2024-12-15 Sun", None, TaskType::Todo),
        ];
        
        let start_date = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 20).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 12).unwrap();
        
        let range = build_week_agenda(&tasks, start_date, end_date, current_date);
        
        assert_eq!(range.len(), 11, "Range should have 11 days (10-20 inclusive)");
        assert_eq!(range[0].date, "2024-12-10");
        assert_eq!(range[10].date, "2024-12-20");
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

    #[test]
    fn test_workday_repeater_not_overdue_on_weekend() {
        // Task scheduled for Friday with +1wd repeater
        let tasks = vec![
            create_test_task_with_repeater("2025-12-05 Fri", None, "+1wd", TaskType::Todo),
        ];
        
        // Today is Saturday - next workday is Monday
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        // Should NOT appear as overdue because next occurrence is Monday (in the future)
        assert_eq!(agenda.overdue.len(), 0, "Task with +1wd should not be overdue on Saturday");
        assert_eq!(agenda.scheduled_timed.len(), 0);
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_workday_repeater_not_overdue_on_sunday() {
        let tasks = vec![
            create_test_task_with_repeater("2025-12-05 Fri", None, "+1wd", TaskType::Todo),
        ];
        
        // Today is Sunday - next workday is Monday
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 0, "Task with +1wd should not be overdue on Sunday");
    }

    #[test]
    fn test_year_repeater_shows_on_occurrence_day() {
        let tasks = vec![
            create_test_task_with_repeater_deadline("2025-12-11 Thu", None, "+1y", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 11).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 11).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 1);
        assert_eq!(agenda.overdue.len(), 0);
    }

    #[test]
    fn test_year_repeater_shows_in_upcoming() {
        let tasks = vec![
            create_test_task_with_repeater_deadline("2025-12-11 Thu", None, "+1y", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 1);
        assert_eq!(agenda.upcoming[0].days_offset, Some(5));
    }

    #[test]
    fn test_year_repeater_not_in_upcoming_too_far() {
        let tasks = vec![
            create_test_task_with_repeater_deadline("2025-12-11 Thu", None, "+1y", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 0);
    }

    #[test]
    fn test_month_repeater_shows_on_occurrence_day() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-05 Thu", None, "+1m", TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 1);
    }

    #[test]
    fn test_workday_repeater_scheduled_on_monday() {
        let tasks = vec![
            create_test_task_with_repeater("2025-12-05 Fri", None, "+1wd", TaskType::Todo),
        ];
        
        // Today is Monday - this is the next occurrence day
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 1, "Task should be scheduled on Monday");
        assert_eq!(agenda.overdue.len(), 0, "Task should not be overdue on its occurrence day");
    }

    #[test]
    fn test_yearly_deadline_shows_on_occurrence_day() {
        // День Рождения Джамика: DEADLINE <2024-12-05 Thu +1y>
        // В 2025 году дедлайн должен быть 2025-12-05 (пятница)
        let tasks = vec![
            create_test_task_with_repeater_deadline("2024-12-05 Thu", None, "+1y", TaskType::Todo),
        ];
        
        // Пятница 2025-12-05 - день deadline (последнее вхождение <= today)
        // По логике org-mode показывается, даже если это прошлая дата
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap(); // Сегодня воскресенье
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 1, "Task should be shown on deadline day (org-mode logic)");
        assert_eq!(agenda.overdue.len(), 0);
        
        // Проверим будущий occurrence day (2026-12-05)
        let future_day = NaiveDate::from_ymd_opt(2026, 12, 5).unwrap();
        let agenda_future = build_day_agenda(&tasks, future_day, current_date);
        
        assert_eq!(agenda_future.scheduled_no_time.len(), 1, "Future occurrence day should show task");
        assert_eq!(agenda_future.scheduled_no_time[0].task.timestamp_date, Some("2026-12-05".to_string()));
        assert!(agenda_future.scheduled_no_time[0].task.timestamp.as_ref().unwrap().contains("2026-12-05"));
    }

    #[test]
    fn test_yearly_deadline_shows_as_overdue_after_occurrence() {
        // День Рождения Джамика: DEADLINE <2024-12-05 Thu +1y>
        // В 2025 году дедлайн был 2025-12-05 (пятница)
        let tasks = vec![
            create_test_task_with_repeater_deadline("2024-12-05 Thu", None, "+1y", TaskType::Todo),
        ];
        
        // Воскресенье 2025-12-07 - через 2 дня после дедлайна
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 1, "Task should be overdue on Sunday");
        assert_eq!(agenda.overdue[0].days_offset, Some(-2), "Task should be 2 days overdue");
        
        // Check that timestamp shows last occurrence date (2025-12-05)
        assert_eq!(agenda.overdue[0].task.timestamp_date, Some("2025-12-05".to_string()));
        assert!(agenda.overdue[0].task.timestamp.as_ref().unwrap().contains("2025-12-05"));
    }
}
