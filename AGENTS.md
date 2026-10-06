# AGENTS.md — dictation

Нативное GPUI-приложение диктовки для Mundus Engine: окно статуса, плашка-pill,
захват хоткея, настройки. Единственный крейт — `gpui/` (`dictation-gpui`);
корневого Cargo workspace нет, все `cargo`-команды идут с
`--manifest-path gpui/Cargo.toml`. Главная платформа — Windows; Linux и macOS
собираются и проверяются в CI. Приложение ставится Engine из GitHub Releases
этого репо.

## Карта

- `gpui/src/main.rs` — запуск; `app.rs` — состояние приложения;
  `session.rs` — автомат сессии записи (PTT, переключатель, осиротевший старт,
  доставка результата); `worker.rs` — блокирующие вызовы Engine в отдельном
  потоке; `hotkey.rs` — хоткей; `pill.rs`, `pill_wave.rs`, `status_window.rs`,
  `settings.rs`, `queue.rs`, `view.rs` — UI.
- `gpui/windows/`, `gpui/build.rs` — иконка и VERSIONINFO.
- `scripts/release.py`, `publish-version.sh`, `test_release.py` — версии,
  упаковка и публикация релизов.

## Границы

- Приложение говорит с Engine только через клиент RPC/WS из
  `mundus-gpui-kit`: операции `dictation.*` (запись, транскрипция, отмена,
  конфиг, состояние, локальные модели, очередь). Микрофон, глобальный хоткей,
  провайдеры распознавания, ключи API и вставка текста принадлежат Engine
  (`cortex/runtime/crates/engine-dictation/`). Нужна новая возможность — это
  новая операция Engine в cortex, а не обход в приложении.
- Ключи API хранятся в cortex; приложение не читает и не стирает их напрямую.
- `DICTATION_GPUI_OFFSCREEN` убирает окно с экрана для headless-запуска.

## Запуск

```text
cargo run --manifest-path gpui/Cargo.toml
```

Engine берётся из `makekosmos/cortex`: `pnpm run dev -- --engine-only` в корне
cortex. Системные пакеты для Linux перечислены в шаге `Linux dependencies`
файла `.github/workflows/build.yml`.

## Проверки

CI (`.github/workflows/build.yml`) запускается на каждый PR и на push в `main`:
fmt, clippy с `-D warnings`, тесты и release-сборка на Windows, Linux и
macOS; плюс `python scripts/test_release.py`. Ночью (00:00 МСК) тот же
workflow выпускает релиз.

Локально те же проверки гонит `hk` (`hk.pkl`):

```text
cargo install hk --locked && hk install     # один раз на копию
hk run pre-push                             # или hk check --all
cargo fmt --manifest-path gpui/Cargo.toml -- --check
cargo clippy --locked --manifest-path gpui/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path gpui/Cargo.toml --all-features
python scripts/test_release.py
```

Не обходи хуки через `--no-verify`.

## Правила кода

- Мёртвый код удаляй сразу, вместе с тестами только на него. Clippy идёт с
  чистым `-D warnings`: не добавляй `-A …` в `hk.pkl` и `build.yml` и
  `#[allow(...)]` в код, чини код.
- `gpui` (псевдоним `gpui-kit`) и `gpui-component` закреплены точными версиями
  и должны совпадать с cortex/manager-gpui, agenda-gpui и memoria-gpui: две
  версии gpui в одной сборке — ошибка типов. Поднимай вместе.
  `imago-gpui` и `mundus-gpui-kit` закреплены по rev; их ревизии сейчас
  отличаются от остальных приложений — выровняй при очередном обновлении.
- Строки интерфейса — русские. Используй токены и компоненты
  imago/`mundus-gpui-kit`, сохраняй клавиатурную навигацию, фокус и доступные
  имена.
- Подписки, таймеры и слушатели снимай при уничтожении окна или view.

## Релиз

Теги вида `gpui-vX.Y.Z`; источник версии — `gpui/Cargo.toml`. Релиз выпускает
workflow по расписанию: версию вручную не меняй, тег не ставь и релиз не
публикуй без просьбы. Уже опубликованные версии не перезаписываются; старая
линия `vX.Y.Z` (Vue `.kspkg`) закрыта и не используется.
