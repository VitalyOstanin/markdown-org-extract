# CLOCK Implementation

## Обзор

Реализована полная поддержка меток CLOCK для отслеживания времени, потраченного на задачи, аналогично Emacs Org-mode.

## Формат

CLOCK записи работают так же, как временные метки - могут быть в обратных кавычках (inline) или в code blocks.

**Inline формат (рекомендуется):**
```markdown
### TODO Task name
`SCHEDULED: <2024-12-10 Tue>`
`CLOCK: <2024-12-09 Mon 10:00>--<2024-12-09 Mon 12:30> => 2:30`
`CLOCK: <2024-12-09 Mon 14:00>--<2024-12-09 Mon 16:15> => 2:15`
```

**Code block формат:**
```markdown
### TODO Task name
`SCHEDULED: <2024-12-10 Tue>`

```
CLOCK: [2024-12-09 Mon 10:00]--[2024-12-09 Mon 12:30] =>  2:30
CLOCK: [2024-12-09 Mon 14:00]--[2024-12-09 Mon 16:15] =>  2:15
```
```

Поддерживаются:
- Закрытые CLOCK записи с временем начала, окончания и длительностью
- Открытые CLOCK записи (только время начала, без окончания)
- Автоматический подсчет общего времени
- Квадратные `[...]` и угловые `<...>` скобки

## Архитектура

### Новые модули

**src/clock.rs**
- `extract_clocks()` - извлечение CLOCK записей из текста
- `calculate_total_minutes()` - подсчет общего времени в минутах
- `format_duration()` - форматирование времени в HH:MM
- `parse_duration()` - парсинг строки длительности

### Изменения в типах (src/types.rs)

```rust
pub struct ClockEntry {
    pub start: String,
    pub end: Option<String>,
    pub duration: Option<String>,
}

pub struct Task {
    // ... существующие поля
    pub clocks: Option<Vec<ClockEntry>>,
    pub total_clock_time: Option<String>,
}
```

### Изменения в парсере (src/parser.rs)

Переработана логика извлечения задач:
- Накопление всех данных заголовка (timestamps, clocks, content) перед созданием задачи
- Финализация задачи при встрече нового заголовка или конца документа
- Извлечение CLOCK из параграфов и code blocks

```rust
struct HeadingInfo {
    heading: String,
    task_type: Option<TaskType>,
    priority: Option<Priority>,
    line: u32,
    content: String,
    created: Option<String>,
    timestamp: Option<String>,
    clocks: Vec<ClockEntry>,
}
```

### Изменения в рендеринге (src/render.rs)

Добавлена поддержка отображения CLOCK в форматах:

**Markdown:**
```markdown
**Total Time:** 4:45

**Clock:**
- 2024-12-09 Mon 10:00 → 2024-12-09 Mon 12:30 (2:30)
- 2024-12-09 Mon 14:00 → 2024-12-09 Mon 16:15 (2:15)
```

**HTML:**
```html
<p><strong>Total Time:</strong> 4:45</p>
<p><strong>Clock:</strong></p>
<ul>
  <li>2024-12-09 Mon 10:00 → 2024-12-09 Mon 12:30 (2:30)</li>
  <li>2024-12-09 Mon 14:00 → 2024-12-09 Mon 16:15 (2:15)</li>
</ul>
```

## Тестирование

Добавлены тесты в `src/clock.rs`:
- `test_extract_closed_clock` - извлечение закрытой CLOCK записи
- `test_extract_open_clock` - извлечение открытой CLOCK записи
- `test_calculate_total` - подсчет общего времени
- `test_parse_duration` - парсинг длительности

Все существующие тесты обновлены для поддержки новых полей.

## Примеры использования

```bash
# Извлечь задачи с CLOCK в JSON
cargo run -- --dir examples --glob "clock-test.md" --tasks --format json

# Вывести в Markdown
cargo run -- --dir examples --glob "clock-test.md" --tasks --format md

# Создать HTML отчет
cargo run -- --dir examples --glob "clock-test.md" --tasks --format html --output report.html
```

## Совместимость

Изменения полностью обратно совместимы:
- Поля `clocks` и `total_clock_time` опциональны
- Задачи без CLOCK записей работают как раньше
- Все существующие тесты проходят
