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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Priority;

    fn create_test_task(date_str: &str, time: Option<&str>, task_type: TaskType) -> Task {
        let timestamp = if let Some(t) = time {
            format!("SCHEDULED: <{} {}>", date_str, t)
        } else {
            format!("SCHEDULED: <{}>", date_str)
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
        let agenda = build_day_agenda(tasks, day_date, current_date);
        
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
        let agenda = build_day_agenda(tasks, day_date, current_date);
        
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
        let agenda = build_day_agenda(tasks.clone(), day_date, current_date);
        
        assert_eq!(agenda.overdue.len(), 2);
        assert_eq!(agenda.overdue[0].days_offset, Some(-4)); // oldest first
        assert_eq!(agenda.overdue[1].days_offset, Some(-2));
        
        // Test on different date - should NOT show overdue
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(tasks, day_date, current_date);
        
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
        let agenda = build_day_agenda(tasks, day_date, current_date);
        
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
}
