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
- `scripts/release.mjs`, `publish-version.mjs`, `test_release.mjs` — версии,
  упаковка и публикация релизов. `scripts/make_icon.py` — единственный
  Python-скрипт (нужен Pillow, только для разработки иконки, в CI нет).

## Границы

- Приложение говорит с Engine только через клиент RPC/WS из
  `mundus-gpui-kit`: операции `dictation.*` (запись, транскрипция, отмена,
  конфиг, состояние, локальные модели, очередь). Микрофон, глобальный хоткей,
  провайдеры распознавания, ключи API и вставка текста принадлежат Engine
  (`cortex/runtime/crates/engine-dictation/`). Нужна новая возможность — это
  новая операция Engine в cortex, а не обход в приложении.
- Ключи API хранятся в cortex, приложению они не нужны: не добавляй работу с ними сюда.
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
fmt, тесты и release-сборка на Windows, Linux и macOS, clippy с
`-D warnings` — только на Windows; плюс `node --test scripts/test_release.mjs`.
Ночью (00:00 МСК) тот же workflow выпускает релиз, если исходники изменились
с прошлого.

Локально те же проверки гонит `lefthook` (`lefthook.yml`; ставится через
`pnpm install`, хуки подключает `prepare`):

```text
pnpm install                                # один раз на копию
pnpm exec lefthook run pre-push             # или lefthook run check
cargo fmt --manifest-path gpui/Cargo.toml -- --check
cargo clippy --locked --manifest-path gpui/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path gpui/Cargo.toml --all-features
node --test scripts/test_release.mjs
```

Не обходи хуки через `--no-verify`.

## Правила кода

- Мёртвый код удаляй сразу, вместе с тестами только на него. Clippy идёт с
  чистым `-D warnings`: не добавляй `-A …` в `lefthook.yml` и `build.yml` и
  `#[allow(...)]` в код, чини код.
- `gpui` (псевдоним `gpui-kit`) и `gpui-component` закреплены точными версиями
  и должны совпадать с cortex/manager-gpui, agenda-gpui и memoria-gpui: две
  версии gpui в одной сборке — ошибка типов. Поднимай вместе.
  `imago-gpui` и `mundus-gpui-kit` закреплены по rev; ревизии сейчас
  отличаются от остальных приложений, а `mundus-gpui-kit` берётся из
  `makekosmos/kosmos-gpui-kit`, не из imago — выровняй при очередном
  обновлении. Здесь нет `deny.toml` и `cargo shear`.
- Строки интерфейса — русские. Используй токены и компоненты
  imago/`mundus-gpui-kit`, сохраняй клавиатурную навигацию, фокус и доступные
  имена.
- Подписки, таймеры и слушатели снимай при уничтожении окна или view.

## Релиз

Теги вида `gpui-vX.Y.Z`; источник версии — `gpui/Cargo.toml`. Релиз выпускает
workflow по расписанию: версию вручную не меняй, тег не ставь и релиз не
публикуй без просьбы. Опубликованные (не draft) версии не перезаписываются; старая
линия `vX.Y.Z` (Vue `.kspkg`) закрыта и не используется.
