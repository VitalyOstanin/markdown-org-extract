# TODO

Отложенные задачи, требующие отдельного согласования или существенного объёма работы.

## Содержание

- [Переход на edition 2024](#переход-на-edition-2024)
- [Параллельный walker (rayon)](#параллельный-walker-rayon)
- [Property-based и fuzz-тесты](#property-based-и-fuzz-тесты)
- [Локализация сообщений CLI](#локализация-сообщений-cli)
- [Бенчмарки (criterion)](#бенчмарки-criterion)

## Переход на edition 2024

Сейчас проект использует `edition = "2021"` и `rust-version = "1.80"`. Edition 2024 стабилизирована в Rust 1.85.

План:

1. Поднять `rust-version = "1.85"` в `Cargo.toml`.
2. Прогнать `cargo fix --edition` и проверить тесты.
3. Обновить MSRV в README и CI (`dtolnay/rust-toolchain@<SHA> # 1.85.0`).

Не делается сейчас: повышает MSRV, требует у пользователей свежего toolchain'а.

## Параллельный walker (rayon)

Crate `ignore` поддерживает параллельный walker через `WalkBuilder::build_parallel()`. На больших vault даёт 2-4x ускорение.

Требует:

- Передачи `mappings`, `matcher`, `stats` через `Arc`/каналы.
- Сбора `tasks` через `Mutex<Vec<Task>>` или `mpsc`.

По правилам не повышать параллелизм без согласования.

## Property-based и fuzz-тесты

Зоны риска:

- `closest_date` для разных `value`, `unit`, `prefer` — инвариант `Past <= current <= Future`.
- `parse_repeater(format(...))` round-trip.
- `add_months` ассоциативность.

Инструменты: `proptest` или `quickcheck`. `cargo-fuzz` для регексов в `timestamp/*`.

## Локализация сообщений CLI

CLI ориентирована на ru-локаль (RU-праздники, `--locale ru,en`), но все сообщения и `--help` на английском. Варианты:

1. Перевод всех сообщений на русский (ломает пайплайны, ожидающие английский текст).
2. Двуязычные сообщения с переключателем через `LANG`/`LC_ALL`.
3. Оставить как есть.

## Бенчмарки (criterion)

Зоны:

- `extract_tasks` на больших markdown.
- `build_week_agenda` / `build_day_agenda` с большим числом repeating-задач.
- `closest_date` для разных `unit`.

Каталог `benches/`, dev-dependency `criterion`.
