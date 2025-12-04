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
    current_date_override: Option<&str>,
) -> Result<AgendaOutput, Box<dyn std::error::Error>> {
    let tz: Tz = tz
        .parse()
        .map_err(|_| format!("Invalid timezone: {tz}. Use IANA timezone names (e.g., 'Europe/Moscow', 'UTC')"))?;

    let today = if let Some(date_str) = current_date_override {
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| format!("Invalid current-date format '{date_str}': {e}. Use YYYY-MM-DD"))?
    } else {
        tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive()
    };

    match mode {
        "day" => {
            let target_date = if let Some(date_str) = date {
                NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .map_err(|e| format!("Invalid date format '{date_str}': {e}. Use YYYY-MM-DD"))?
            } else {
                today
            };
            Ok(AgendaOutput::Days(vec![build_day_agenda(&tasks, target_date, today)]))
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
        _ => Err(format!("Invalid agenda mode '{mode}'. Valid modes: 'day', 'week', 'tasks'").into()),
    }
}

/// Build agenda for a single day
fn build_day_agenda(tasks: &[Task], day_date: NaiveDate, current_date: NaiveDate) -> DayAgenda {
    let mut agenda = DayAgenda::new(day_date);
    let is_today = day_date == current_date;
    
    for task in tasks {
        if let Some(ref ts) = task.timestamp {
            if let Some(parsed) = parse_org_timestamp(ts, None) {
                // Handle repeating tasks
                if let Some(ref repeater) = parsed.repeater {
                    handle_repeating_task(task, &parsed, repeater, day_date, current_date, &mut agenda);
                } else {
                    // Non-repeating task
                    handle_non_repeating_task(task, &parsed, day_date, is_today, &mut agenda);
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

fn handle_non_repeating_task(
    task: &Task,
    parsed: &crate::timestamp::ParsedTimestamp,
    day_date: NaiveDate,
    is_today: bool,
    agenda: &mut DayAgenda,
) {
    let task_date = parsed.date;
    let days_diff = (task_date - day_date).num_days();
    
    let task_with_offset = TaskWithOffset {
        task: task.clone(),
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
        } else if day_date == current_date && day_date > base_date {
            // Show as overdue on current date if we're past the base date
            // and today is not an occurrence day
            let last_occurrence = find_last_occurrence(base_date, repeater, current_date);
            if let Some(last_occ) = last_occurrence {
                if last_occ < current_date {
                    let days_diff = (last_occ - current_date).num_days();
                    let task_with_offset = TaskWithOffset {
                        task: task.clone(),
                        days_offset: Some(days_diff),
                    };
                    agenda.overdue.push(task_with_offset);
                }
            }
        }
    } else {
        // Future occurrence - show as upcoming
        if let Some(next_occ) = next_occurrence(base_date, repeater, day_date) {
            if next_occ > day_date {
                let days_diff = (next_occ - day_date).num_days();
                let task_with_offset = TaskWithOffset {
                    task: task.clone(),
                    days_offset: Some(days_diff),
                };
                agenda.upcoming.push(task_with_offset);
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
        result.push(build_day_agenda(tasks, current, current_date));
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

    fn create_test_task(date_str: &str, time: Option<&str>, task_type: TaskType) -> Task {
        let timestamp = if let Some(t) = time {
            format!("SCHEDULED: <{date_str} {t}>")
        } else {
            format!("SCHEDULED: <{date_str}>")
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
    fn test_build_day_agenda_upcoming() {
        let tasks = vec![
            create_test_task("2024-12-06 Thu", None, TaskType::Todo),
            create_test_task("2024-12-08 Sat", None, TaskType::Todo),
            create_test_task("2024-12-07 Fri", None, TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.upcoming.len(), 3);
        assert_eq!(agenda.scheduled_timed.len(), 0);
        assert_eq!(agenda.scheduled_no_time.len(), 0);
        
        // Check sorting by days_offset (nearest first)
        assert_eq!(agenda.upcoming[0].days_offset, Some(1));
        assert_eq!(agenda.upcoming[1].days_offset, Some(2));
        assert_eq!(agenda.upcoming[2].days_offset, Some(3));
    }

    #[test]
    fn test_build_day_agenda_overdue_only_on_current_date() {
        let tasks = vec![
            create_test_task("2024-12-01 Mon", None, TaskType::Todo),
            create_test_task("2024-12-03 Wed", None, TaskType::Todo),
        ];
        
        // Test on current date - should show overdue
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 2);
        assert_eq!(agenda.overdue[0].days_offset, Some(-4)); // oldest first
        assert_eq!(agenda.overdue[1].days_offset, Some(-2));
        
        // Test on different date - should NOT show overdue
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 0);
    }

    #[test]
    fn test_build_day_agenda_mixed() {
        let tasks = vec![
            create_test_task("2024-12-03 Mon", None, TaskType::Todo), // overdue
            create_test_task("2024-12-05 Wed", Some("09:00"), TaskType::Todo), // scheduled timed
            create_test_task("2024-12-05 Wed", Some("15:00"), TaskType::Todo), // scheduled timed
            create_test_task("2024-12-05 Wed", None, TaskType::Todo), // scheduled no time
            create_test_task("2024-12-07 Fri", None, TaskType::Todo), // upcoming
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 1);
        assert_eq!(agenda.scheduled_timed.len(), 2);
        assert_eq!(agenda.scheduled_no_time.len(), 1);
        assert_eq!(agenda.upcoming.len(), 1);
    }

    #[test]
    fn test_filter_agenda_day_mode() {
        let tasks = vec![
            create_test_task("2024-12-05 Wed", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-06 Thu", None, TaskType::Todo),
        ];
        
        let result = filter_agenda(
            tasks,
            "day",
            Some("2024-12-05"),
            None,
            None,
            "UTC",
            Some("2024-12-05"),
        ).unwrap();
        
        match result {
            AgendaOutput::Days(days) => {
                assert_eq!(days.len(), 1);
                assert_eq!(days[0].date, "2024-12-05");
                assert_eq!(days[0].scheduled_timed.len(), 1);
                assert_eq!(days[0].upcoming.len(), 1);
            }
            _ => panic!("Expected Days output"),
        }
    }

    #[test]
    fn test_filter_agenda_week_mode() {
        let tasks = vec![
            create_test_task("2024-12-02 Mon", None, TaskType::Todo),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
            create_test_task("2024-12-04 Wed", None, TaskType::Todo),
        ];
        
        let result = filter_agenda(
            tasks,
            "week",
            None,
            Some("2024-12-02"),
            Some("2024-12-04"),
            "UTC",
            Some("2024-12-03"),
        ).unwrap();
        
        match result {
            AgendaOutput::Days(days) => {
                assert_eq!(days.len(), 3);
                
                // Day 1: 2024-12-02 (before current_date)
                assert_eq!(days[0].date, "2024-12-02");
                assert_eq!(days[0].scheduled_no_time.len(), 1);
                assert_eq!(days[0].upcoming.len(), 2);
                
                // Day 2: 2024-12-03 (current_date)
                assert_eq!(days[1].date, "2024-12-03");
                assert_eq!(days[1].overdue.len(), 1); // 2024-12-02 is overdue
                assert_eq!(days[1].scheduled_no_time.len(), 1);
                assert_eq!(days[1].upcoming.len(), 1);
                
                // Day 3: 2024-12-04 (after current_date)
                assert_eq!(days[2].date, "2024-12-04");
                assert_eq!(days[2].overdue.len(), 0); // overdue only on current_date
                assert_eq!(days[2].scheduled_no_time.len(), 1);
            }
            _ => panic!("Expected Days output"),
        }
    }

    #[test]
    fn test_filter_agenda_tasks_mode() {
        let mut task1 = create_test_task("2024-12-05 Wed", None, TaskType::Todo);
        task1.priority = Some(Priority::B);
        
        let mut task2 = create_test_task("2024-12-06 Thu", None, TaskType::Todo);
        task2.priority = Some(Priority::A);
        
        let task3 = create_test_task("2024-12-07 Fri", None, TaskType::Done);
        
        let tasks = vec![task1, task2, task3];
        
        let result = filter_agenda(
            tasks,
            "tasks",
            None,
            None,
            None,
            "UTC",
            None,
        ).unwrap();
        
        match result {
            AgendaOutput::Tasks(tasks) => {
                assert_eq!(tasks.len(), 2); // Only TODO tasks
                assert_eq!(tasks[0].priority, Some(Priority::A)); // Sorted by priority
                assert_eq!(tasks[1].priority, Some(Priority::B));
            }
            _ => panic!("Expected Tasks output"),
        }
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
    fn test_build_day_agenda_repeating_daily() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("10:00"), "+1d", TaskType::Todo),
        ];
        
        // Check Dec 5 - should show as scheduled
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
        
        // Check Dec 4 - not an occurrence day (1, 3, 5, 7...)
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
        
        // Check Dec 8 (Sunday) - should show
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 1);
        
        // Check Dec 9 (Monday) - should not show
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 9).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_build_day_agenda_mixed_repeating_and_regular() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("10:00"), "+1d", TaskType::Todo),
            create_test_task("2024-12-05 Wed", Some("14:00"), TaskType::Todo),
            create_test_task("2024-12-06 Thu", None, TaskType::Todo),
        ];
        
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        // Should have both repeating task and regular task
        assert_eq!(agenda.scheduled_timed.len(), 2);
        assert_eq!(agenda.upcoming.len(), 1);
    }

    #[test]
    fn test_build_day_agenda_repeating_before_base_date() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-05 Wed", None, "+1d", TaskType::Todo),
        ];
        
        // Check Dec 3 - before base date, should not show
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 3).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);
        
        assert_eq!(agenda.scheduled_no_time.len(), 0);
        assert_eq!(agenda.upcoming.len(), 1); // Should show as upcoming
    }

    #[test]
    fn test_build_day_agenda_repeating_every_2_days() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", None, "+2d", TaskType::Todo),
        ];
        
        // Dec 1, 3, 5, 7 should show
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
        // Should be sorted by time: 09:00, 11:00, 14:00
        assert_eq!(agenda.scheduled_timed[0].task.timestamp_time, Some("09:00".to_string()));
        assert_eq!(agenda.scheduled_timed[1].task.timestamp_time, Some("11:00".to_string()));
        assert_eq!(agenda.scheduled_timed[2].task.timestamp_time, Some("14:00".to_string()));
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
            heading: "Test repeating task".to_string(),
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
}
