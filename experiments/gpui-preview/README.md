# Console Link - GPUI Preview

Изолированный технический прототип первого этапа перехода на **GPUI 0.2.2**. Основной бинарник, его Cargo.toml/Cargo.lock, настройки и данные входа не заменяются.

Проверяется простейшая граница: синтетический источник 1920x1080 с целевыми 60 FPS или прежний capture backend, затем CPU RGBA -> BGRA и `RenderImage` GPUI. Этот путь намеренно обозначен как **CPU baseline**, а не как принятый production-presenter.

## Запуск

Из корня репозитория:

```sh
cargo run --locked --manifest-path experiments/gpui-preview/Cargo.toml -- --smoke-seconds 15
```

Без `--smoke-seconds` окно работает до закрытия или Cmd+Q / Ctrl+Q. Аргумент задает таймер автоматического завершения для проверок без записи настроек. Таймер работает отдельно от render callbacks: скрытие или перекрытие окна не должно оставлять smoke-процесс работающим бесконечно. Занятый main loop все еще может задержать обработку события таймера. По умолчанию используется только синтетический поток, камера не открывается.

На этом Mac достаточно Command Line Tools: feature `runtime_shaders` обходит вызов отсутствующего `xcrun metal`. Это вариант эксперимента, не проверенная production-схема сборки Metal. Linux не является целью данного crate.

Для отдельного Mac-приложения:

```sh
cd experiments/gpui-preview
bash scripts/package-macos.sh
open 'dist/Console Link GPUI Preview.app' --args --smoke-seconds 15
```

Скрипт уважает `CARGO_TARGET_DIR` и `GPUI_PREVIEW_OUTPUT_DIR`. Bundle имеет отдельный ID и разрешение камеры для опциональной проверки захвата. Он подписан ad-hoc, без Apple notarization. Production Console Link.app и его разрешения не изменяются.

Windows-артефакт `console-link-gpui-preview.exe` - экспериментальный бинарник, не установщик или новый релиз Console Link. Пример:

```powershell
.\console-link-gpui-preview.exe --smoke-seconds 15
```

## Управление

- Tab / Esc: открыть или закрыть боковую панель настроек поверх видео.
- F11, Cmd+Enter на Mac или Alt+Enter на Windows: нативный fullscreen и возврат.
- Cmd+Q / Ctrl+Q, закрытие последнего окна: завершение приложения и остановка источника.
- `--borderless`: запуск без рамки на основном мониторе. На Mac после создания окна применяется AppKit Borderless + frame текущего NSScreen: один `titlebar=None` в GPUI 0.2.2 оставляет NSTitled и смещает окно под строку меню. Это начальный режим окна; отдельный переход в него во время работы еще не реализован. Нативный fullscreen из этого режима отключен; запускай без `--borderless` для его отдельной проверки.
- `--fullscreen`: запуск сразу в нативном fullscreen. Не совмещается с `--borderless`.
- Без этих флагов: обычное изменяемое окно.

Пример безрамочного опыта:

```sh
cargo run --locked --manifest-path experiments/gpui-preview/Cargo.toml -- --borderless --smoke-seconds 20
```

## Реальная карта захвата, только явно

```sh
cargo run --locked --manifest-path experiments/gpui-preview/Cargo.toml -- --list-devices
```

Затем передай индекс нужной карты:

```sh
cargo run --locked --manifest-path experiments/gpui-preview/Cargo.toml -- --capture 0 --smoke-seconds 20
```

Для Mac рекомендуется запускать capture через bundle с собственным разрешением камеры. Индекс зависит от текущего порядка обнаружения: сверяй показанное имя и режим источника. Приложение не выбирает случайную камеру вместо недоступного индекса. Это видео без аудио, Discord, Nintendo и пользовательских настроек.

Режим карты выбирает существующая политика backend. Ранее проверенная USB-карта объявляла максимум Full HD 50 FPS. Синтетические 60 FPS не доказывают поддержку 60 FPS этой картой.

## Что означают метрики

Примерно раз в секунду активного рендера stderr получает строку `METRICS`, при освобождении view - `FINAL`. `WINDOW` сообщает текущие границы, viewport, DPI scale и fullscreen для проверки режимов. Во время задержки рендера строки тоже задерживаются, а не дописываются искусственно:

- `received`, `input_fps`: сколько кадров источник передал в ограниченную очередь до ее потерь.
- `ui_updates`, `ui_updates_fps`: сколько новых кадров преобразовано и передано view GPUI. Это **не presented/display FPS**.
- `source_dropped`: замены устаревшего кадра в очереди емкостью один.
- `queue_age_ms`: возраст последнего кадра в начале `render()` по монотонному timestamp backend. Для capture включает время после получения sample callback; для синтетики timestamp ставится после подготовки кадра.
- `cpu_conversion_ms`: CPU-конвертация BGRA, создание RenderImage и замена предыдущего изображения. GPU upload и завершение показа сюда не входят.
- `atlas_retired`: число вызовов успешного `Window::drop_image` для предыдущих изображений. Один Arc drop не освобождает запись atlas, поэтому это сделано явно.

Обновления запрашиваются через `request_animation_frame`; частота также зависит от монитора, GPUI и нагрузки. Изображение сохраняет пропорции через `ObjectFit::Contain`, включая resize и DPI. GPUI view хранит одно текущее изображение, backend держит не более одного ожидающего кадра.

## Проверки

```sh
cargo fmt --check --manifest-path experiments/gpui-preview/Cargo.toml
cargo clippy --locked --manifest-path experiments/gpui-preview/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path experiments/gpui-preview/Cargo.toml
```

Тест CPU-преобразования проверяет правильный порядок каналов и альфу. Тесты источника проверяют ограниченную очередь, остановку и неблокирующее закрытие при задержанном запуске backend. Они не используют реальные аккаунты или оборудование.

## Границы опыта

- Оверлей GPUI и видеоплоскость находятся в одном окне, но нынешние wgpu/NIS/FSR/RTX-проходы еще не включены. Нет доказанного Metal/D3D11/D3D12 texture interop.
- Нет показателя end-to-end HDMI latency, display latency или сигнала о завершении GPU present. Название `RenderImage` не делает CPU-путь zero-copy.
- Во время полного выхода окно освобождает собственный atlas; при замене кадров записи удаляются явно. Длительная стабильность RSS/GPU-памяти все равно требует измерения.
- Скрытие неподвижного курсора, переключение обычного/безрамочного окна на лету и перенос остальных экранов не входят в этот CPU baseline.
- Mac/Windows runtime-поведение и показатели следует фиксировать отдельно. Успешная компиляция Windows сама по себе не доказывает проверку интерфейса на Windows 11.
