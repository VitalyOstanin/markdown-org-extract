# markdown-org-extract

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
- Debug: `target/debug/markdown-org-extract`
- Release: `target/release/markdown-org-extract`

### Запуск

После сборки запустите утилиту:

```bash
# Debug версия
./target/debug/markdown-org-extract [OPTIONS]

# Release версия
./target/release/markdown-org-extract [OPTIONS]
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

#### Покрытие тестами функционала рабочих дней

Модуль `holidays` (9 тестов):
- Загрузка календаря праздников
- Проверка обычных выходных и рабочих дней
- Новогодние каникулы 2025 (1-8 января) и 2026 (1-9 января)
- Переносы праздников 2026 (8 марта → 9 марта, 9 мая → 11 мая)
- Пропуск выходных и праздников при поиске следующего рабочего дня

Модуль `timestamp::repeater` (6 тестов):
- Парсинг повторов `+1wd`, `+2wd`, `++1wd`, `.+1wd`
- Расчет следующего повтора по рабочим дням
- Пропуск праздников в повторах

Модуль `timestamp::parser` (2 теста):
- Парсинг временных меток с `+1wd` и `+2wd`

## Использование

```bash
markdown-org-extract [OPTIONS]
```

### Параметры

- `--dir <DIR>` - каталог для поиска (по умолчанию: `.`)
- `--glob <GLOB>` - шаблон для фильтрации файлов (по умолчанию: `*.md`)
- `--format <FORMAT>` - формат вывода: `json`, `md`, `html` (по умолчанию: `json`)
- `--output <OUTPUT>` - файл для записи результата (по умолчанию: stdout)
- `--locale <LOCALE>` - локали для дней недели через запятую (по умолчанию: `ru,en`)
- `--agenda <MODE>` - режим agenda: `day`, `week`, `month` (по умолчанию: `day`)
- `--tasks` - показать все TODO задачи, отсортированные по приоритету (альтернатива `--agenda tasks`)
- `--date <DATE>` - дата для режима `day` в формате YYYY-MM-DD (по умолчанию: текущая дата)
- `--from <DATE>` - начальная дата для режима `week` в формате YYYY-MM-DD (по умолчанию: понедельник текущей недели)
- `--to <DATE>` - конечная дата для режима `week` в формате YYYY-MM-DD (по умолчанию: воскресенье текущей недели)
- `--tz <TIMEZONE>` - часовой пояс для определения текущей даты (по умолчанию: `Europe/Moscow`)
- `--current-date <DATE>` - явная текущая дата для расчета overdue в формате YYYY-MM-DD (по умолчанию: сегодня в указанной таймзоне)
- `--holidays <YEAR>` - вывести список праздников для указанного года (1900-2100) в формате JSON

### Примеры использования

Извлечь задачи из текущего каталога в JSON:
```bash
markdown-org-extract
```

Извлечь задачи из конкретного каталога:
```bash
markdown-org-extract --dir ./notes
```

Сохранить результат в HTML файл:
```bash
markdown-org-extract --dir ./notes --format html --output agenda.html
```

Вывести в markdown формате:
```bash
markdown-org-extract --dir ./notes --format md
```

Использовать примеры из проекта:
```bash
markdown-org-extract --dir ./examples
markdown-org-extract --dir ./examples --format md
markdown-org-extract --dir ./examples --format html --output examples-agenda.html
```

Использовать только русские дни недели:
```bash
markdown-org-extract --dir ./notes --locale ru
```

Использовать только английские дни недели:
```bash
markdown-org-extract --dir ./notes --locale en
```

#### Примеры работы с agenda

Задачи на сегодня (по умолчанию):
```bash
markdown-org-extract --dir ./notes
```

Задачи на конкретную дату:
```bash
markdown-org-extract --dir ./notes --agenda day --date 2025-12-10
```

Получить список праздников для года:
```bash
markdown-org-extract --holidays 2025
markdown-org-extract --holidays 2026
```

Пример вывода праздников:
```json
[
  "2025-01-01",
  "2025-01-02",
  "2025-01-03",
  "2025-01-04",
  "2025-01-05",
  "2025-01-06",
  "2025-01-07",
  "2025-01-08",
  "2025-02-23",
  "2025-03-08",
  "2025-05-01",
  "2025-05-09",
  "2025-06-12",
  "2025-11-04"
]
```
```

Задачи на текущую неделю:
```bash
markdown-org-extract --dir ./notes --agenda week
```

Задачи на текущий месяц:
```bash
markdown-org-extract --dir ./notes --agenda month
```

Задачи на диапазон дат:
```bash
markdown-org-extract --dir ./notes --agenda week --from 2025-12-01 --to 2025-12-07
markdown-org-extract --dir ./notes --agenda month --from 2025-12-01 --to 2025-12-31
```

Все TODO задачи, отсортированные по приоритету:
```bash
markdown-org-extract --dir ./notes --tasks
```

Использовать другой часовой пояс:
```bash
markdown-org-extract --dir ./notes --tz UTC
markdown-org-extract --dir ./notes --tz America/New_York
```

Использовать явную текущую дату (для тестов):
```bash
markdown-org-extract --dir ./notes --agenda week --current-date 2024-12-05
```

Вывести список праздников для года:
```bash
markdown-org-extract --holidays 2025
markdown-org-extract --holidays 2026
```

## Примеры файлов

В каталоге `examples/` находятся примеры markdown файлов с различными метками:

- `project-tasks.md` - задачи разработки проекта
- `personal-notes.md` - личные заметки и задачи
- `meeting-notes.md` - заметки со встреч

Попробуйте запустить:
```bash
./target/release/markdown-org-extract --dir ./examples --format md
```

## Режимы Agenda

Утилита поддерживает четыре режима работы с задачами, аналогично Emacs Org-mode:

### day - Задачи на день

Показывает задачи с временными метками (SCHEDULED, DEADLINE) на указанную дату. По умолчанию используется текущая дата в указанной таймзоне.

```bash
# Задачи на сегодня
markdown-org-extract --agenda day

# Задачи на конкретную дату
markdown-org-extract --agenda day --date 2025-12-10
```

### week - Задачи на неделю

Показывает задачи с временными метками в диапазоне дат. По умолчанию используется текущая неделя (понедельник-воскресенье).

Каждый день показывает:
- Задачи, запланированные на этот день (scheduled)
- Предстоящие задачи относительно этого дня (upcoming)
- Просроченные задачи (overdue) - только для текущей даты

```bash
# Задачи на текущую неделю
markdown-org-extract --agenda week

# Задачи на конкретный диапазон
markdown-org-extract --agenda week --from 2025-12-01 --to 2025-12-07
```

### month - Задачи на месяц

Показывает задачи с временными метками в диапазоне дат. По умолчанию используется текущий месяц (с первого по последний день).

Работает аналогично режиму `week` - каждый день показывает scheduled, upcoming и overdue задачи.

```bash
# Задачи на текущий месяц
markdown-org-extract --agenda month

# Задачи на конкретный диапазон
markdown-org-extract --agenda month --from 2025-12-01 --to 2025-12-31
```

### tasks - Все TODO задачи

Показывает все задачи со статусом TODO, отсортированные по приоритету (A → B → C → без приоритета). Временные метки не учитываются.

```bash
# Все TODO задачи по приоритетам
markdown-org-extract --tasks
```

### Часовые пояса

Параметр `--tz` определяет часовой пояс для вычисления текущей даты и недели. Поддерживаются все стандартные IANA таймзоны.

```bash
# Московское время (по умолчанию)
markdown-org-extract --agenda day --tz Europe/Moscow

# UTC
markdown-org-extract --agenda day --tz UTC

# Нью-Йорк
markdown-org-extract --agenda day --tz America/New_York
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
`CREATED: <2024-12-01 Mon>`
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

**Примечание:** Метка `CREATED` извлекается отдельно от других временных меток и сохраняется в поле `created`. Это позволяет отслеживать дату создания задачи независимо от других временных меток (SCHEDULED, DEADLINE, CLOSED).

### Учет времени (CLOCK)

Утилита поддерживает метки CLOCK для отслеживания времени, потраченного на задачи, аналогично Emacs Org-mode.

**Формат CLOCK записей (в обратных кавычках, как временные метки):**
```markdown
### TODO Implement feature

`SCHEDULED: <2024-12-10 Tue>`
`CLOCK: <2024-12-09 Mon 10:00>--<2024-12-09 Mon 12:30> => 2:30`
`CLOCK: <2024-12-09 Mon 14:00>--<2024-12-09 Mon 16:15> => 2:15`
```

**Альтернативный формат (в code blocks, как в org-mode):**
```markdown
### TODO Implement feature

`SCHEDULED: <2024-12-10 Tue>`

```
CLOCK: [2024-12-09 Mon 10:00]--[2024-12-09 Mon 12:30] =>  2:30
CLOCK: [2024-12-09 Mon 14:00]--[2024-12-09 Mon 16:15] =>  2:15
```
```

**Открытая CLOCK запись (активная работа):**
```markdown
`CLOCK: <2024-12-10 Tue 09:00>`
```

**Возможности:**
- Автоматическое извлечение всех CLOCK записей под заголовком
- Подсчет общего времени (`total_clock_time`) по всем записям
- Поддержка открытых (активных) CLOCK записей без времени окончания
- Отображение в JSON, Markdown и HTML форматах
- Поддержка как квадратных `[...]` (как в org-mode), так и угловых `<...>` скобок

**Пример вывода JSON:**
```json
{
  "heading": "Implement feature",
  "clocks": [
    {
      "start": "2024-12-09 Mon 10:00",
      "end": "2024-12-09 Mon 12:30",
      "duration": "2:30"
    },
    {
      "start": "2024-12-09 Mon 14:00",
      "end": "2024-12-09 Mon 16:15",
      "duration": "2:15"
    }
  ],
  "total_clock_time": "4:45"
}
```

**Пример вывода Markdown:**
```markdown
## Implement feature
**Total Time:** 4:45

**Clock:**
- 2024-12-09 Mon 10:00 → 2024-12-09 Mon 12:30 (2:30)
- 2024-12-09 Mon 14:00 → 2024-12-09 Mon 16:15 (2:15)
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

Формат вывода зависит от режима agenda:

### Режим `--tasks` (список задач)

#### JSON

```json
[
  {
    "file": "/path/to/file.md",
    "line": 42,
    "heading": "Task title",
    "content": "Task description",
    "task_type": "TODO",
    "priority": "A",
    "created": "CREATED: <2024-12-01 Mon>",
    "timestamp": "DEADLINE: <2024-12-15 Sun>",
    "timestamp_type": "DEADLINE",
    "timestamp_date": "2024-12-15",
    "timestamp_time": null,
    "timestamp_end_time": null
  }
]
```

#### Markdown

```markdown
# Tasks

## Task title
**File:** /path/to/file.md:42
**Type:** TODO
**Priority:** [#A]
**Created:** CREATED: <2024-12-01 Mon>
**Time:** DEADLINE: <2024-12-15 Sun>

Task description
```

### Режимы `--agenda day` и `--agenda week` (дневная agenda)

В этих режимах задачи группируются по дням. Каждый день содержит категории задач (в порядке отображения):

1. **Overdue** (только для текущей даты) - просроченные задачи, самые старые наверху
2. **Scheduled (with time)** - задачи дня со временем, отсортированные по времени (ранние наверху)
3. **Scheduled (no time)** - задачи дня без времени
4. **Upcoming** - предстоящие задачи относительно этого дня, ближайшие наверху

**Важно:** Каждый день показывает upcoming задачи относительно себя, а не относительно общей референсной даты.

#### JSON

```json
[
  {
    "date": "2024-12-05",
    "overdue": [],
    "scheduled_timed": [
      {
        "file": "./examples/project-tasks.md",
        "line": 5,
        "heading": "Design database schema",
        "content": "Need to finalize the database structure.",
        "task_type": "TODO",
        "priority": "A",
        "timestamp": "SCHEDULED: <2024-12-05 Wed 10:00>",
        "timestamp_type": "SCHEDULED",
        "timestamp_date": "2024-12-05",
        "timestamp_time": "10:00"
      }
    ],
    "scheduled_no_time": [
      {
        "file": "./examples/project-tasks.md",
        "line": 10,
        "heading": "Review code",
        "content": "Code review needed.",
        "task_type": "TODO",
        "timestamp": "SCHEDULED: <2024-12-05 Wed>",
        "timestamp_type": "SCHEDULED",
        "timestamp_date": "2024-12-05"
      }
    ],
    "upcoming": [
      {
        "file": "./examples/project-tasks.md",
        "line": 47,
        "heading": "Review pull request #42",
        "content": "Critical bug fix needs review.",
        "task_type": "TODO",
        "timestamp": "DEADLINE: <2024-12-06 Thu>",
        "timestamp_type": "DEADLINE",
        "timestamp_date": "2024-12-06",
        "days_offset": 1
      }
    ]
  }
]
```

Поле `days_offset` показывает:
- Положительное число - количество дней до срока (upcoming)
- Отрицательное число - количество дней просрочки (overdue)
- Отсутствует для задач текущего дня (scheduled)

#### Markdown

```markdown
# Agenda

## 2024-12-05

### Overdue

#### Create project repository (4 days ago)
**File:** ./examples/project-tasks.md:13
**Type:** Done
**Time:** CLOSED: <2024-12-01 Mon>

Repository created and initial structure set up.

### Scheduled

#### Design database schema
**File:** ./examples/project-tasks.md:5
**Type:** Todo
**Priority:** A
**Time:** SCHEDULED: <2024-12-05 Wed>

Need to finalize the database structure.

### Upcoming

#### Review pull request #42 (in 1 days)
**File:** ./examples/project-tasks.md:47
**Type:** Todo
**Time:** DEADLINE: <2024-12-06 Thu>

Critical bug fix needs review.
```

#### Поля разобранных временных меток

Для удобства отрисовки agenda внешними потребителями, временные метки автоматически разбираются на составные части:

- `timestamp_type` - тип временной метки: `SCHEDULED`, `DEADLINE`, `CLOSED`, `PLAIN`
- `timestamp_date` - дата в формате `YYYY-MM-DD`
- `timestamp_time` - время начала (если указано), например `10:00`
- `timestamp_end_time` - время окончания (если указан диапазон), например `12:00`

Эти поля позволяют внешним системам отображать задачи на временной шкале без повторного парсинга строки `timestamp`.

## Повторяющиеся задачи

Утилита поддерживает синтаксис повторов org-mode для автоматического планирования задач:

### Типы повторов

Поддерживаются все стандартные единицы org-mode:

- `+Nh` - повтор каждые N часов
- `+Nd` - повтор каждые N дней (строгий, сохраняет просрочку)
- `+Nw` - повтор каждые N недель
- `+Nm` - повтор каждые N месяцев
- `+Ny` - повтор каждые N лет
- `+Nwd` - **повтор каждые N рабочих дней** (расширение, с учетом праздников и выходных РФ)

Модификаторы типа повтора:
- `+` - строгий повтор (cumulative), сохраняет просрочку
- `++` - умный повтор (catch-up), сохраняет день недели
- `.+` - повтор от даты завершения (restart)

### Рабочие дни

Повторы с суффиксом `wd` (workday) учитывают:
- Обычные выходные (суббота, воскресенье)
- Официальные праздники РФ
- Переносы праздничных дней

Данные о праздниках хранятся в файле `holidays_ru.json`. При сборке проекта (`build.rs`) данные компилируются в статические константы Rust для максимальной производительности - парсинг JSON происходит один раз на этапе компиляции, а не в runtime.

### Примеры

```markdown
### TODO Проверка каждый час
`SCHEDULED: <2025-12-05 Thu 10:00 +1h>`

### TODO Ежедневная задача
`SCHEDULED: <2025-12-05 Thu +1d>`

### TODO Еженедельная встреча
`SCHEDULED: <2025-12-05 Thu +1w>`

### TODO Ежемесячный отчет
`SCHEDULED: <2025-12-05 Thu +1m>`

### TODO Ежегодная проверка
`SCHEDULED: <2025-12-05 Thu +1y>`

### TODO Рабочая задача (только по рабочим дням)
`SCHEDULED: <2025-12-05 Thu +1wd>`

### TODO Задача каждые 2 рабочих дня
`SCHEDULED: <2025-12-05 Thu +2wd>`
```

## Структура проекта

```
markdown-org-extract/
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
