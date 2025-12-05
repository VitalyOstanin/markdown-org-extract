use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("holidays_data.rs");

    let json = fs::read_to_string("holidays_ru.json").expect("Failed to read holidays_ru.json");
    let data: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");

    let mut code = String::from("pub static HOLIDAYS: &[(i32, u32, u32)] = &[\n");

    for (year, year_data) in data.as_object().unwrap() {
        if let Some(holidays) = year_data.get("holidays").and_then(|v| v.as_array()) {
            for holiday in holidays {
                if let Some(date_str) = holiday.as_str() {
                    let parts: Vec<&str> = date_str.split('-').collect();
                    if parts.len() == 3 {
                        code.push_str(&format!(
                            "    ({}, {}, {}),\n",
                            parts[0], parts[1], parts[2]
                        ));
                    }
                }
            }
        }
    }

    code.push_str("];\n\n");
    code.push_str("pub static WORKDAYS: &[(i32, u32, u32)] = &[\n");

    for (year, year_data) in data.as_object().unwrap() {
        if let Some(workdays) = year_data.get("workdays").and_then(|v| v.as_array()) {
            for workday in workdays {
                if let Some(date_str) = workday.as_str() {
                    let parts: Vec<&str> = date_str.split('-').collect();
                    if parts.len() == 3 {
                        code.push_str(&format!(
                            "    ({}, {}, {}),\n",
                            parts[0], parts[1], parts[2]
                        ));
                    }
                }
            }
        }
    }

    code.push_str("];\n");

    fs::write(&dest_path, code).expect("Failed to write generated code");
    println!("cargo:rerun-if-changed=holidays_ru.json");
}
