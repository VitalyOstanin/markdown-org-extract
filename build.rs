use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=holidays_ru.json");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR must be set by cargo");
    let dest_path = Path::new(&out_dir).join("holidays_data.rs");

    let json =
        fs::read_to_string("holidays_ru.json").expect("build.rs: failed to read holidays_ru.json");
    let data: serde_json::Value =
        serde_json::from_str(&json).expect("build.rs: holidays_ru.json is not valid JSON");
    let root = data
        .as_object()
        .expect("build.rs: top-level JSON value must be an object keyed by year");

    let mut holidays_code = String::from("pub static HOLIDAYS: &[(i32, u32, u32)] = &[\n");
    let mut workdays_code = String::from("pub static WORKDAYS: &[(i32, u32, u32)] = &[\n");

    for (year_key, year_data) in root {
        if let Some(arr) = year_data.get("holidays").and_then(|v| v.as_array()) {
            emit_dates(arr, "holidays", year_key, &mut holidays_code);
        }
        if let Some(arr) = year_data.get("workdays").and_then(|v| v.as_array()) {
            emit_dates(arr, "workdays", year_key, &mut workdays_code);
        }
    }

    holidays_code.push_str("];\n\n");
    workdays_code.push_str("];\n");

    let mut code = holidays_code;
    code.push_str(&workdays_code);

    fs::write(&dest_path, code).expect("build.rs: failed to write generated holidays_data.rs");
}

fn emit_dates(arr: &[serde_json::Value], kind: &str, year_key: &str, code: &mut String) {
    for entry in arr {
        let date_str = entry.as_str().unwrap_or_else(|| {
            panic!("build.rs: {kind} entry for year {year_key} must be a string (got {entry:?})")
        });
        let (year, month, day) = parse_date(date_str).unwrap_or_else(|err| {
            panic!("build.rs: invalid {kind} date '{date_str}' under year {year_key}: {err}")
        });
        code.push_str(&format!("    ({year}, {month}, {day}),\n"));
    }
}

fn parse_date(s: &str) -> Result<(i32, u32, u32), String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("expected YYYY-MM-DD, got '{s}'"));
    }
    let year: i32 = parts[0]
        .parse()
        .map_err(|e| format!("year '{}' is not an integer: {e}", parts[0]))?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|e| format!("month '{}' is not an integer: {e}", parts[1]))?;
    let day: u32 = parts[2]
        .parse()
        .map_err(|e| format!("day '{}' is not an integer: {e}", parts[2]))?;

    // Validate the calendar date so a malformed JSON fails the build with a clear message
    // (without pulling chrono into [build-dependencies]).
    if !(1..=12).contains(&month) {
        return Err(format!("month {month} is out of range 1..=12"));
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => unreachable!(),
    };
    if !(1..=max_day).contains(&day) {
        return Err(format!(
            "day {day} is out of range 1..={max_day} for month {month}"
        ));
    }

    Ok((year, month, day))
}
