# Kosmos Dictation

Голосовой ввод для Kosmos: hotkey (toggle или push-to-talk), оверлей-«таблетка»
с waveform и статусами записи, локальные и облачные модели распознавания,
вставка текста в активное окно. Микрофон, глобальный hotkey, жизненный цикл
оверлея, credentials, провайдеры транскрибации и вставка текста — зона
ответственности Kosmos Engine (`dictation.v2`); это приложение — GPUI-клиент
поверх Engine RPC (`POST /v1/rpc`) и broadcast WebSocket, он никогда не
обращается к этим возможностям напрямую.

**KOS-241**: Vue-версия приложения (Marketplace-пакет `.kspkg`, `kosmos-host`)
и отдельный Windows-воркер (`worker/dictation-worker.exe`) удалены из этого
репозитория. `dictation-gpui` — единственный продукт. Настройки диктовки и
hotkey уже перенесены в GPUI Manager («Диктовка»); этот репозиторий отвечает
только за pill-оверлей, окно/автозапуск и Engine RPC/WS-клиент.

## Почему исчез `worker/`

`worker/worker.ts` (компилировался в `worker/dictation-worker.exe`) был
Kosmos-worker-процессом, которого Marketplace-хост запускал и кормил через
stdin/stdout сообщениями `worker.invoke` — он транслировал внешний hotkey
(`dictation.trigger`, kind=ptt/toggle, phase=up/down) в ту же последовательность
Engine-операций: `dictation.window.foreground` → `dictation.capture.start` →
`dictation.capture.stop` → `dictation.speech.transcribe` →
`dictation.input.insert_text`.

`dictation-gpui` реализует ровно эту же state-машину нативно в Rust
(`src/app.rs`, `src/worker.rs` — внутренний background-поток, не отдельный
процесс) и получает hotkey-триггеры напрямую широковещательными событиями
Engine по WebSocket (`dictation_toggle_trigger`, `dictation_ptt_trigger`),
без stdin/stdout-моста и без отдельного бинарника. В коде `dictation-gpui` нет
ни одного вызова, порождающего дочерний процесс — `worker/dictation-worker.exe`
никем не запускается и не может быть запущен после удаления Marketplace-хоста
(эпик KOS-236). Поэтому воркер полностью избыточен и удалён вместе с
Vue-пакетом, а не оставлен «на всякий случай».

## Совместимость с данными пользователей (KOS-241)

Ничего в путях и формате пользовательских данных не меняется — меняется
только способ доставки приложения (exe-компонент вместо Marketplace-пакета).
`compatibility.json` документирует и сохраняет legacy-идентичность:

- `extension_id: "dictation"`, `app_id: "com.kosmos.dictation"` — та же
  идентичность, что была у Vue-пакета.
- legacy-корни данных: `extensions/dictation`, `extensions-data/dictation`,
  `extensions-backups/dictation` — конфиг, credentials, локальные speech-модели
  и настройки диктовки как хранились, так и хранятся Engine по этим путям;
  `dictation-gpui` их не трогает напрямую, только через `dictation.v2` RPC.
- `manifest.json` и `package.manifest.json` (Marketplace-манифест: `kind: vue`,
  `entrypoint: dist/index.html`, targets `kosmos-host` + `worker`) удалены —
  они описывали только формат `.kspkg`-пакета, которого больше нет.
  `compatibility.json` остаётся источником истины по legacy-идентичности и
  используется Engine/Cortex независимо от способа упаковки.

## Упаковка в состав Kosmos (Cortex)

Kosmos зашивает этот бинарь как компонент
`resources/components/dictation/Kosmos Dictation.exe`, тем же способом, что
`agenda-gpui` (`resources/components/agenda`) и `memoria-gpui`
(`resources/components/memoria`).

Контракт для Cortex (`cortex/desktop/scripts/build-package-components.mjs`
или его аналог):

- **Исходники**: checkout этого репозитория (`Cargo.toml` в корне).
- **Команда сборки**:

  ```powershell
  cargo build --locked --release --target x86_64-pc-windows-msvc
  ```

- **Версия**: переменная окружения `KOSMOS_DICTATION_VERSION=<X.Y.Z>` —
  `build.rs` штампует ею VERSIONINFO (`FileVersion`/`ProductVersion`) и иконку
  (`windows/app.ico`) собранного `Kosmos Dictation.exe`; без переменной версия
  берётся из `Cargo.toml` (для локальной сборки).
- **Результат**: `target/x86_64-pc-windows-msvc/release/dictation-gpui.exe` →
  переименовывается/копируется Cortex в
  `resources/components/dictation/Kosmos Dictation.exe`.
- **Windows subsystem**: `#![windows_subsystem = "windows"]` (см.
  `src/main.rs`) — без консольного окна; паника пишется в
  `%APPDATA%\Kosmos\dictation-gpui-panic.log`.
- **Данные**: `KOSMOS_DATA_DIR` переопределяет папку данных; по умолчанию
  `%APPDATA%\Kosmos` — тот же `engine.lock.json`, что читает GPUI Manager.

## Локальная сборка и запуск

Требования: Rust stable, Windows (приложение использует `windows-sys` —
реестр для автозапуска, WH-хук и т.п. живут в Engine, но иконка/VERSIONINFO
и часть путей в `src/main.rs` собираются только под Windows).

```powershell
cargo build
cargo run
```

Сначала запусти Kosmos Engine с той же папкой данных, что использует GPUI
Manager — приложение читает `engine.lock.json` и обращается к
`http://127.0.0.1:<port>/v1/rpc` с bearer-токеном оттуда.

`DICTATION_GPUI_OFFSCREEN=1` паркует окно за пределами экрана (headless-запуск,
автотесты). `--background` — режим автозапуска: окно создаётся скрытым, hotkey
и pill продолжают работать; повторный запуск экземпляра сигналит уже
работающему процессу показать (развернуть) окно.

## Локальные гейты

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo` на этой машине проходит через общий shared build cache (`mbx`)
автоматически — отдельного шага bootstrap не требуется. Отдельных
Git-хуков (lefthook/husky и т.п.) в репозитории больше нет — вместе с
Vue-инструментарием удалён и хук, гонявший его линт/формат/тесты; CI
(`.github/workflows/ci.yml`) прогоняет те же три команды на `windows-latest`.
