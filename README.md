# markdown-extract

CLI утилита для извлечения задач из markdown файлов с поддержкой меток Emacs Org-mode.

## Установка и сборка

### Требования

- Rust 1.70 или новее
- Cargo

### Сборка проекта

Сборка в режиме разработки:
```bash
cargo build
```

Сборка оптимизированной версии для production:
```bash
cargo build --release
```

Бинарный файл будет создан в:
- Debug: `target/debug/markdown-extract`
- Release: `target/release/markdown-extract`

### Запуск

После сборки запустите утилиту:

```bash
# Debug версия
./target/debug/markdown-extract [OPTIONS]

# Release версия
./target/release/markdown-extract [OPTIONS]
```

Или используйте cargo для запуска без явной сборки:
```bash
cargo run -- [OPTIONS]
```

### Тестирование

Запуск тестов:
```bash
cargo test
```

Запуск с подробным выводом:
```bash
cargo test -- --nocapture
```

Проверка кода:
```bash
cargo check
cargo clippy
```

## Использование

```bash
markdown-extract [OPTIONS]
```

### Параметры

- `--dir <DIR>` - каталог для поиска (по умолчанию: `.`)
- `--glob <GLOB>` - шаблон для фильтрации файлов (по умолчанию: `*.md`)
- `--format <FORMAT>` - формат вывода: `json`, `md`, `html` (по умолчанию: `json`)
- `--output <OUTPUT>` - файл для записи результата (по умолчанию: stdout)
- `--locale <LOCALE>` - локали для дней недели через запятую (по умолчанию: `ru,en`)

### Примеры использования

Извлечь задачи из текущего каталога в JSON:
```bash
markdown-extract
```

Извлечь задачи из конкретного каталога:
```bash
markdown-extract --dir ./notes
```

Сохранить результат в HTML файл:
```bash
markdown-extract --dir ./notes --format html --output agenda.html
```

Вывести в markdown формате:
```bash
markdown-extract --dir ./notes --format md
```

Использовать примеры из проекта:
```bash
markdown-extract --dir ./examples
markdown-extract --dir ./examples --format md
markdown-extract --dir ./examples --format html --output examples-agenda.html
```

Использовать только русские дни недели:
```bash
markdown-extract --dir ./notes --locale ru
```

Использовать только английские дни недели:
```bash
markdown-extract --dir ./notes --locale en
```

## Примеры файлов

В каталоге `examples/` находятся примеры markdown файлов с различными метками:

- `project-tasks.md` - задачи разработки проекта
- `personal-notes.md` - личные заметки и задачи
- `meeting-notes.md` - заметки со встреч

Попробуйте запустить:
```bash
./target/release/markdown-extract --dir ./examples --format md
```

## Поддерживаемые метки

### Метки задач

Утилита распознает метки TODO и DONE в заголовках:

```markdown
### TODO Implement feature
### DONE Complete task
```

### Приоритеты задач

Поддерживаются приоритеты в формате org-mode (буквы A-Z в квадратных скобках):

```markdown
### TODO [#A] Критическая задача
### TODO [#B] Важная задача
### TODO [#C] Обычная задача
### DONE [#A] Завершенная задача высокого приоритета
```

Приоритет указывается после метки TODO/DONE и перед текстом задачи. Наиболее распространенные приоритеты:
- `[#A]` - высокий приоритет (критические задачи)
- `[#B]` - средний приоритет (важные задачи)
- `[#C]` - низкий приоритет (обычные задачи)

Приоритет является необязательным параметром.

### Временные метки

Временные метки должны быть заключены в обратные кавычки:

**Простая временная метка:**
```markdown
`<2024-12-10 Mon 10:00-12:00>`
```

**Метки планирования:**
```markdown
`DEADLINE: <2024-12-15 Sun>`
`SCHEDULED: <2024-12-05 Wed>`
`CLOSED: <2024-12-01 Mon>`
```

**Диапазон дат:**
```markdown
`<2024-12-20 Mon>--<2024-12-22 Wed>`
```

**Неактивные временные метки (НЕ извлекаются):**
```markdown
`[2024-12-10 Mon]` - квадратные скобки означают неактивную метку
```

## Поддержка локалей

Утилита поддерживает дни недели на разных языках через параметр `--locale`.

### Поддерживаемые локали

- `en` - английский (Mon, Tue, Wed, Thu, Fri, Sat, Sun, Monday, Tuesday, ...)
- `ru` - русский (Пн, Вт, Ср, Чт, Пт, Сб, Вс, Понедельник, Вторник, ...)

По умолчанию используются обе локали: `--locale ru,en`

### Примеры с русскими днями недели

```markdown
### TODO Встреча
`<2024-12-10 Пн 10:00>`

### Конференция
`<2024-12-20 Понедельник>--<2024-12-22 Среда>`

### TODO Задача
`DEADLINE: <2024-12-15 Вс>`
```

Все русские дни недели автоматически нормализуются в английский формат при извлечении.

## Формат вывода

### JSON (по умолчанию)

```json
[
  {
    "file": "/path/to/file.md",
    "line": 42,
    "heading": "Task title",
    "content": "Task description",
    "task_type": "TODO",
    "priority": "A",
    "timestamp": "DEADLINE: <2024-12-15 Sun>"
  }
]
```

### Markdown

```markdown
# Tasks

## Task title
**File:** /path/to/file.md:42
**Type:** TODO
**Priority:** [#A]
**Time:** DEADLINE: <2024-12-15 Sun>

Task description
```

### HTML

```html
<html><body><h1>Tasks</h1>
<h2>Task title</h2>
<p><strong>File:</strong> /path/to/file.md:42</p>
<p><strong>Type:</strong> TODO</p>
<p><strong>Priority:</strong> [#A]</p>
<p><strong>Time:</strong> DEADLINE: <2024-12-15 Sun></p>
<p>Task description</p>
</body></html>
```

## Структура проекта

```
markdown-extract/
├── src/
│   └── main.rs          # Основной код приложения
├── examples/            # Примеры markdown файлов
│   ├── project-tasks.md
│   ├── personal-notes.md
│   └── meeting-notes.md
├── Cargo.toml           # Зависимости проекта
└── README.md            # Документация
```

## Зависимости

- `clap` - парсинг аргументов командной строки
- `comrak` - парсинг markdown
- `glob` - поиск файлов по шаблону
- `regex` - работа с регулярными выражениями
- `serde` / `serde_json` - сериализация данных

## Лицензия

MIT
