use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "markdown-extract")]
#[command(about = "Extract tasks from markdown files")]
pub struct Cli {
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    #[arg(long, default_value = "*.md")]
    pub glob: String,

    #[arg(long, default_value = "json")]
    pub format: String,

    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long, default_value = "ru,en")]
    pub locale: String,

    #[arg(long, default_value = "day")]
    pub agenda: String,

    #[arg(long)]
    pub date: Option<String>,

    #[arg(long)]
    pub from: Option<String>,

    #[arg(long)]
    pub to: Option<String>,

    #[arg(long, default_value = "Europe/Moscow")]
    pub tz: String,
}

pub fn get_weekday_mappings(locale: &str) -> Vec<(&'static str, &'static str)> {
    let locales: Vec<&str> = locale.split(',').map(|s| s.trim()).collect();
    let mut mappings = Vec::new();
    
    for loc in locales {
        if loc == "ru" {
            mappings.extend_from_slice(&[
                ("Понедельник", "Monday"), ("Вторник", "Tuesday"),
                ("Среда", "Wednesday"), ("Четверг", "Thursday"),
                ("Пятница", "Friday"), ("Суббота", "Saturday"),
                ("Воскресенье", "Sunday"),
                ("Пн", "Mon"), ("Вт", "Tue"), ("Ср", "Wed"), 
                ("Чт", "Thu"), ("Пт", "Fri"), ("Сб", "Sat"), ("Вс", "Sun"),
            ]);
        }
    }
    mappings
}
