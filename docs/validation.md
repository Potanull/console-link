# Проверка первой реализации

Дата: 2026-11-09. Состояние: тестовая версия, аппаратная приемка не завершена.

## Выполнено

- macOS: успешный трехсекундный запуск окна с Metal после исправления отсутствующего GPU backend. Камера и микрофон не открывались.
- macOS arm64: общий `cargo check`, 33 unit tests, Clippy без предупреждений. Пакет 0.1.1 с иконкой прошел реальный smoke launch/exit.
- Windows GNU target: общий cross-target check и Clippy. Это проверка типов и платформенного кода, без запуска Windows.
- Native adapters: перечисление режимов AVFoundation и Media Foundation, тесты рациональных FPS и преобразования пикселей/stride.
- Аудио: bounded ring, PCM gain/mute/peak, silence on underrun, conversion sample rates, bounded drift feedback, concurrency и callbacks длиннее 20 мс.
- Scheduler: отрицательная коррекция на потоке 60 FPS, выбор последнего наступившего кадра и ограничение памяти.
- Настройки: сохранение неизвестной будущей схемы, поврежденные данные, повторная атомарная запись.
- Независимое ревью: устранены обрезание аудио на длинном callback, panic через Windows COM boundary, зависающий Windows IPC write, отсутствующий GPU backend, устаревший Discord status при очистке поля игры и некорректный fullscreen после повторного подключения.

## Аппаратный статус

На MacBook Pro M2 Pro перечисление вне sandbox вернуло встроенную FaceTime HD Camera. USB-карта захвата и отдельный HDMI audio endpoint не обнаружены. Пользователь сообщил о подключении карты; отсутствие в перечислении требует проверки USB data-порта, кабеля и хаба. Это не доказательство неисправности карты.

Характеристики продавца подтверждают только заявление о максимальном Full HD. Реальные 1080p60, UVC/UAC и форматы еще не проверены. Ни камера Mac, ни микрофон не использовались для тестовой записи.

## Еще требуется

- [ ] Устройство карты видно одновременно как видео- и аудиоисточник.
- [ ] Реальная картинка Switch в игровом окне; корректные цвета, пропорции, FPS и PNG.
- [ ] Звук именно HDMI-карты, выбранный выход, mute/gain/peak, оба направления offset.
- [ ] Реальное отключение USB, восстановление того же устройства и ручная отмена reconnect.
- [ ] Потеря аудиовыхода останавливает вывод без неожиданного переключения на динамики.
- [ ] 30-минутный прогон: CPU, RAM, потерянные кадры, underruns и отсутствие нарастающего рассогласования.
- [ ] Измерение HDMI-to-display latency внешней камерой/таймкодом.
- [ ] Discord Presence с зарегистрированным Application ID.
- [ ] Discord-звонок на Mac: принимающая сторона видит игру и слышит консоль; volume/mute ведут себя ожидаемо.
- [ ] Аналогичный Discord-звонок и аппаратный прогон на Windows 11.
- [x] Нативный Windows MSVC build/test/package в GitHub Actions.
- [x] macOS arm64 build/test/package в GitHub Actions.

## Ограничения реализации

- Видео использует host delivery timestamps и CPU BGRA-to-RGBA conversion. Полная синхронизация аппаратных clock domains и zero-copy не реализованы.
- Аудио использует линейный resampler и обратную связь по заполнению очереди, не аппаратный source-clock mapper. Базовый запас: max(20 мс, длительность native output callback), затем выбранная задержка и системная задержка выхода.
- CPAL 0.16 идентифицирует аудиоустройства уникальными именами. Дубли имен отклоняются; стабильность имени после смены USB-порта зависит от ОС.
- При timeout native driver может освобождаться дольше UI; остановка выполняется вне UI. Повторное открытие может временно получить «устройство занято».
- Коррекция видео ограничена бюджетом истории 320 МиБ. При уменьшении достижимого offset приложение показывает сообщение.
- Нет автоматического распознавания игры и независимого monitor/stream audio mix.
- macOS пакет имеет ad-hoc подпись без notarization; Windows code signing отсутствует.
- Сквозная работоспособность не доказана одними unit tests, cross-check и code review.

Зависимость block 0.1.6 сообщает предупреждение future-incompatibility текущего Rust. Сборку оно не блокирует; обновление цепочки зависимостей требуется до перехода на будущую несовместимую версию компилятора. Cargo.lock фиксирует windows 0.58.0 для gpu-allocator, совместимую с wgpu-hal; широкое ограничение allocator иначе может выбрать несовместимую windows 0.54.0 из аудиозависимостей.

## Подтверждение CI

[Native builds #1](https://github.com/Potanull/console-link/actions/runs/42643094504), исходный коммит `231e498acb43bd7e72748d86e06de6b708df8ea7`: оба задания завершились success, включая Formatting, Clippy, Tests, release packaging и upload artifacts. Это не аппаратный прогон и не проверка реального звонка.

## Версия 0.2.0: интерфейс и поставка

Переработаны визуальные компоненты; media lifecycle сохранен. Независимое ревью не выявило новых P1/P2. Визуально проверены собственные renderer-снимки обычного и компактного окон, страницы звука/Discord/настроек. Snapshot-режим не читает и не сохраняет пользовательские настройки. Локально прошли 33 теста, форматирование, Clippy на Mac и Windows cross-target и реальный smoke launch/exit Mac .app0.2.0. Inter OFL включен в поставку.

Windows-установщик и публикация Releases подтверждены в [Native builds #2](https://github.com/Potanull/console-link/actions/runs/42667922070), исходный коммит cdcb375bd1316ce71e46756ab88427de8936fd87. Все задания success. Windows smoke проверил установку без admin, отсутствие отдельного VC runtime в PE imports, запуск CLI, повторную установку с прежним AppId, ярлык/регистрацию, сохранность пользовательского sentinel и удаление managed files.

[Предварительный релиз v0.2.0](https://github.com/Potanull/console-link/releases/tag/v0.2.0) опубликован: Mac ZIP, Windows installer, portable ZIP и SHA256SUMS.txt. Все файлы скачаны через публичные ссылки; SHA256 фактических байтов сверены с manifest и GitHub asset digest. Проверены содержимое архивов, версии и наличие лицензионных уведомлений. Аппаратная приемка остается открытой.

## Версия 0.3.0: игровой режим и Discord

Локально прошли 57 library tests и 11 binary tests, форматирование, Mac Clippy всех targets и Windows GNU cross-target Clippy. Реальный Mac .app прошел smoke launch/exit. Независимое ревью подтвердило исправление потери FPS на источнике 60 Гц, busy polling без новых кадров, сохранения rotated refresh token и повторного восстановления входа после offline. UI позволяет забыть вход также после ошибки.

Discord Application Console Link зарегистрировано владельцем: Public Client включен, loopback redirect сохранен. PKCE, callback validation, отмена, secret redaction, account gate и временные ошибки покрыты тестами. Реальный consent/code exchange, Keychain, refresh/revoke и статус в настольном Discord пока не подтверждены.

Снимки интерфейса используют собственный renderer без доступа к Keychain и пользовательским настройкам. Диагностические кадры и циклы режимов окна не заменяют аппаратный захват. Обновлены уведомления для 302 зависимостей двух целевых платформ, включая OAuth/native TLS и Inter OFL.

Поставка подтверждена в [Native builds #3](https://github.com/Potanull/console-link/actions/runs/42678726910), исходный коммит `e50261f51649747d738167749713f2c434177769`. Все задания success: macOS arm64, Windows MSVC, тесты, Clippy, упаковка, установка/повторная установка/удаление Windows и публикация.

[Предварительный релиз v0.3.0](https://github.com/Potanull/console-link/releases/tag/v0.3.0) содержит Mac ZIP, Windows installer, portable ZIP и SHA256SUMS.txt. Все четыре файла скачаны через публичные ссылки. SHA256 фактических байтов совпали с manifest и GitHub asset digest. ZIP проверены на целостность и наличие бинарников/лицензий, Mac Info.plist подтверждает 0.3.0 build 4. Реальная авторизация Discord и аппаратная приемка остаются открытыми.

## Версия 0.3.1: громкость, курсор и Full HD

Добавлен двухсекундный индикатор громкости и mute, работающий от горячих клавиш и ползунка. Во время игры курсор скрывается после 4 секунд без движения; движение, открытие панели, остановка и смена фокуса сбрасывают таймер. В меню курсор не скрывается. Проверены собственный renderer-снимок индикатора и реальный smoke launch/exit Mac .app. Физическое поведение курсора на Windows и аппаратный захват требуют ручной проверки.

Автоматический режим предпочитает 1920×1080, если доступен в пределах выбранного FPS, затем ближайшую частоту. По умолчанию 1080p30 получает приоритет перед 720p60. При отсутствии Full HD действует прежний выбор доступного режима. Ручной выбор сохранен.

Локально прошли 57 library tests и 15 binary tests, Mac Clippy всех targets и Windows GNU cross-target Clippy. Тесты проверяют 4-секундный порог, возврат курсора, продление/истечение индикатора, приоритет Full HD и сохранение лимита FPS. Независимое ревью не выявило важных дефектов.

Безрамочный режим исправлен после уточнения пользователя: обычное окно без рамки заполняет полный rect текущего монитора, без native fullscreen/отдельного Space. macOS читает NSScreen.frame с преобразованием координат, Windows читает MONITORINFO.rcMonitor с учетом DPI. При выходе из native fullscreen ожидается также снятие NSWindow FullScreen bit. Оконная геометрия сохраняется только из оконного режима.

Нативные Mac diagnostic runs подтвердили размер безрамочного окна 2560×1440 и возврат обычного меню к 1040×760. Возврат после native fullscreen также дал 1040×760. Проверки выполнены без карты захвата. Первую CI-публикацию 0.3.1 (42681864316) намеренно отменили до publish, чтобы включить это исправление в тот же выпуск.

Финальный [Native builds #5](https://github.com/Potanull/console-link/actions/runs/42684119647), исходный коммит `185fe61d2d2e5cde024db6e426b4e37e9ec8352d`: все задания success, включая Windows MSVC, Mac arm64, Clippy/tests, упаковку, Windows install/reinstall/uninstall и публикацию. [Релиз v0.3.1](https://github.com/Potanull/console-link/releases/tag/v0.3.1) опубликован. Все четыре публичных файла скачаны; SHA256 совпали с manifest и GitHub digest. ZIP прошли проверку целостности/наличия бинарников и лицензий; Mac Info.plist подтверждает 0.3.1 build 5.

## Версия 0.4.0: распознавание и Discord

Локально прошли 67 library tests и 17 binary tests, форматирование, Mac и Windows GNU all-targets Clippy. Отдельный ignored-by-default release-тест с чистым кешем выполнил реальную загрузку двух моделей, проверил SHA256, распознал синтетический титульный экран и выбрал Zelda: Breath of the Wild из общего каталога. Это проверка настоящего inference, не аппаратный benchmark. Старые HF-веса не прошли тот же тест и были заменены актуальными официальными CLI S3-весами. Два файла занимают 12 212 656 байт; контрольные суммы зафиксированы.

Каталог содержит 24 017 базовых игр и 8655 региональных названий из шести регионов TitleDB. Это общий снимок метаданных, не список пользовательских пресетов. Тесты покрывают Unicode normalization, неоднозначные HOME-экраны, повторное подтверждение, разные сессии и выключение worker без создания model cache. Ручная коррекция и выключение блокируют устаревшие результаты.

Discord SET_ACTIVITY подтверждается ответом с проверкой opcode/command/nonce; тесты проверили отказ, проигнорированное/неподтвержденное название, Ping и ограничения. Защита аккаунта сохранена. Аватар строится из проверенных публичных полей профиля. Загрузка artwork имеет лимиты, allowlist и 30-секундный retry.

Валидный OAuth callback очищает URL и открывает tokenless console-link://auth/complete, native completion отдельно фокусирует окно. Invalid/denied callback не отправляет URI. Windows handler ищет существующее окно того же executable и восстанавливает его, Mac bundle объявляет URL scheme. Windows installer smoke дополнен проверкой регистрации/удаления обработчика.

Собственный renderer проверен с явно заданным тестовым профилем в обычном и компактном окне. Загружены публичный стандартный Discord avatar и Nintendo cover; fixture не является фактом живой авторизации.

Открытые проверки: реальная карта/титульный экран Switch, точность на реальных кадрах, ресурсный прогон OCR, фактические имя/обложка в настольном Discord, браузерный OAuth-return и Windows native activation. Публичный API Discord не позволяет начать Go Live или принудительно показать его stream prompt: приложение предоставляет собственную подсказку и переход в Discord, пользователь выбирает звонок/окно/звук.

[Native builds #6](https://github.com/Potanull/console-link/actions/runs/42691053403), исходный коммит `b96d49be5a783f1df426598b30c79bed59fdb330`: все задания success. Windows MSVC и Mac arm64 прошли Clippy/tests, release packaging и upload; Windows дополнительно install/reinstall/uninstall, сохранность пользовательского sentinel и регистрацию/удаление tokenless URL protocol. Mac runner начал работу после ожидания свободной емкости.

[Предварительный релиз v0.4.0](https://github.com/Potanull/console-link/releases/tag/v0.4.0) опубликован: Mac ZIP, Windows installer, portable ZIP и SHA256SUMS.txt. Все четыре файла скачаны публично; SHA256 фактических байтов совпали с manifest и GitHub asset digest. Проверены ZIP integrity, бинарники, лицензии и Mac Info.plist 0.4.0 build 6. Пользователь ответил, что живую проверку пока не выполнял; аппаратные/Discord пункты остаются открытыми.


## 2026-11-11 — управление звуком и диагностика

Windows GNU: `cargo build --locked` и `cargo test --all-targets --locked` прошли (73 теста библиотеки, 17 тестов приложения). Новые проверки: частотная характеристика и bypass EQ, независимость стереоканалов, конечный ограниченный выход 8–192 кГц, отсутствие чрезмерного ослабления при усилении всех полос, применение подготовленного EQ в Renderer до выходного индикатора, полный цикл сохранения/переименования/удаления пресета с чтением файла, совместимость старых настроек и ограничение истории мониторинга. Форматирование и `git diff --check` прошли.

Снимки: стартовое меню 1200×800; звуковая страница и Discord; узкое окно 720×600. В нативном окне просмотрены таблица диагностики и оба графика, пустые состояния при отсутствии сигнала и работа вертикальной прокрутки. OCR-код, загрузка моделей, прямые зависимости ocrs/rten и действия запуска Discord отсутствуют; оставлены каталог ручного выбора и Discord OAuth/Presence.

В ходе полной компиляции исправлена существовавшая после переноса констант строка форматирования OAuth callback. Проверки на физической карте захвата, прослушивание звука, новая авторизация Discord и сборка macOS в этой сессии не выполнялись.


## 2026-11-11 — исправление игрового оверлея и нагрузки

- Полный набор:74 теста библиотеки и20 тестов приложения прошли. Регрессионные проверки: реальная egui-рамка и закреплённая кнопка остаются внутри окна480×270 и выше; десять подписей частот/ползунков всегда на одной горизонтальной линии в основном окне и оверлее; старый флаг подсказки игнорируется без сброса настроек; типизированное сравнение замечает изменения пресетов.
- Release Windows GNU собран без предупреждений. Снимки release: оверлей960×600, высокий1100×1380, основная аудиостраница720×600, игровая сцена без share popup. Автоматический цикл игра→меню завершён с кодом0. Заголовок и нижняя кнопка закреплены; прокрутка разделена по вкладкам. Внешняя рамка учитывает собственную толщину.
- Удалены рендеринг, состояние, настройка и таймер подсказки «Поделиться игрой». JSON-сериализация настроек больше не выполняется на каждом игровом кадре; типизированное сравнение выполняется только при показе оверлея. Свернутый оверлей не копирует настройки.
- На текущем исходнике изолированное преобразование BGRA1080p:16.10мс/кадр без оптимизации и1.57мс с оптимизацией. Это почти весь бюджет16.67мс при60FPS в старой debug-сборке. Dev-профиль приложения теперь opt-level2; для игры подготовлен `target/release/console-link.exe` с полной оптимизацией.
- Это не измерение полной задержки HDMI и не аппаратная приёмка. Реальный захват после исправлений не запускался; текущие игровые процессы не прерывались.


## 2026-11-11 — SDR-цветокоррекция и пресеты

- Полный набор:83теста библиотеки +26тестов приложения прошли без предупреждений компилятора. Отдельный ignored-бенчмарк запущен явно и прошёл. Проверки включают точный нейтральный обход, линейную экспозицию, тональные регуляторы/температуру/насыщенность, ограничение нечисловых значений, совместимость schema1, сохранение/переименование/удаление пользовательских пресетов через файл.
- Реальный wgpu/DX12: NVIDIA GeForce RTX4080, driver32.0.16.1088. Pipeline/WGSL валидны. Readback совпал с CPU на257×35 и1920×1080 для обеих поверхностей Rgba8Unorm/Rgba8UnormSrgb, всех встроенных пресетов, крайних и нечисловых значений. Для bypass и alpha расхождение0; для обработанного RGB<=2значения из255. Проверяется переиспользование texture/Arc, без повторной загрузки одного кадра.
- PNG записан и прочитан обратно: исходные размеры/alpha/sRGB, обработанный результат и принудительный оригинал подтверждены. Неверные размеры не создают экспортный каталог. Состояние экспорта фиксируется с уже представленным кадром; Original сравнение не меняет сохранённые параметры.
- Синтетический тест120кадров,1080p: первая серия0.529мс/кадр original и0.551мс HDR; повтор0.580мс и0.546мс. Загружаются разные заранее подготовленные кадры, outputtexture/pipeline переиспользуются, в измерении нет CPU-grading и GPU-readback. Разница сопоставима с вариативностью теста. Это среднее walltime загрузки+отрисовки+завершения batch, НЕ HDMI-to-display latency и не гарантия для другой видеокарты.
- Release Windows GNU собран. Визуально проверены основной раздел1200×800, узкий720×600, F1-оверлей960×600 и смена нейтрального/HDR-пресета на подписанной тестовой шкале. Все7регуляторов доступны; на широком окне предпросмотр рядом с ними. Совместимость обычного bounded-оверлея и однорядного EQ сохранена. Независимое ревью математики, кеша, экспорта и пресетов без actionable findings.
- Физический захват и macOS в этой сессии не проверялись. Нет настоящего HDR/PQ/HLG/10бит или восстановления исходно потерянных деталей. Источник трактуется как SDR sRGB, так как текущие backends не сохраняют цветовые метаданные отдельно.

## 2026-11-11 — пространственный апскейл NIS / FSR 1

- Итоговый полный набор: 85 тестов библиотеки + 32 теста приложения прошли; два синтетических timing-теста помечены ignored, новый benchmark апскейла отдельно запущен и прошёл. Строгий Clippy всех целей с -D warnings и release Windows GNU собраны успешно. Независимое полное ревью не выявило actionable ошибок шейдеров, GPU-ресурсов, кеша или экспорта.
- Реальная RTX4080/DX12, driver32.0.16.1088: NIS и FSR сравнены с C++-эталоном из закреплённых исходных функций NVIDIA/AMD. Максимальная ошибка NIS1/255; FSR1 до4/255 при максимальной резкости RCAS (средняя <0.1/255), EASU отдельно <=1/255. Полные методика и значения: [отчёт GPU](superpowers/plans/upscaling-task1-report.md).
- Warm synthetic1080p->1440p,30batch по4кадра после5прогревов, без readback: Standard median0.625–0.637мс; NIS0.840–0.852мс; FSR0.841–0.863мс. p95 каждого режима <1мс. Это время компонента, включая upload/подготовку/submit/завершение batch; не измерение HDMI-to-display и не гарантия на другом GPU.
- Проверены RGB-константы, контуры/градиенты/текстуры, настройки резкости, native/downscale bypass, переиспользование текстур, DPI и округление viewport, snapshot параметров при смене кадра/режима/размера. Обработанные PNG нечётного размера29x21 прочитаны обратно с точными bytes/dimensions/sRGB для NIS/FSR и ненейтрального цвета; оригинальный экспорт сохраняет исходные пиксели/размер.
- Release UI визуально проверен1200x800 и оверлей960x600 для FSR и NIS; элементы внутри ограниченного drawer, содержимое прокручивается. Экспорт ограничен одним запросом; виден статус сохранения. Сравнение не меняет сохранённые настройки. Лицензии AMD/NVIDIA включены в поставляемый THIRD_PARTY_LICENSES.txt.
- RTX Video НЕ реализован: официальный SDK не получен (developer download -> login, публичные NGC routes ->HTTP403). Оба предложенных пользователем проекта также требуют отдельный SDK. Пункт интерфейса недоступен; наличие настройки enum не является реализацией. Нужны официальный архив, native-интеграция и реальные GPU-проверки. Физический HDMI-захват и macOS в этой работе не проверены. Новый релиз не опубликован.


## 0.6.1 macOS capture regression (2026-11-12)

- Reproduced 0.6.0 rejection on the connected USB capture card: `This capture device does not support explicit input format selection.` The required InputPriority preset is not a macOS capture prerequisite.
- Removed the preset gate. The production connection path then started native video and the selected audio pair (muted), delivered 1920x1080, and presented a frame in the gameplay UI.
- A post-start equality check exposed a real negotiation mismatch: the requested Full HD 50 stream actually delivered 20 callbacks/s with active durations 20/1, both with Standard and FSR1. Native dropped frames were zero and pixel conversion averaged about 2 ms. Reapply the advertised format and rational durations under the device lock after session startup, without another graph commit. The selected mode stays labeled as selected; the UI measures received FPS separately.
- Error banner previously fell below the visible area. Renderer snapshots now show it above the scrolling setup cards at 1040x760 and 1040x720.
- Actual Metal GPU tests passed on Apple M2 Pro for Standard/NIS/FSR1 constant-pattern render/readback, resource reuse and RTX error fallback. This does not certify every Apple Silicon GPU/OS combination.
- Hardware diagnostics record startup stages only, not media, credentials, settings or device IDs. Diagnostic runs use isolated defaults and stop after 30 seconds.
- Audible output, end-to-end HDMI latency and Discord calls are not covered by these checks.
- Spatial GPU initialization is lazy and uses validation/internal/out-of-memory error scopes; failures are cached and select ordinary rendering. Enhanced screenshot initialization uses the same guarded creation. RTX is disabled in the selector outside Windows.
- Added numeric native telemetry (callback/delivery/drop counts, conversion time, active duration metadata) to distinguish device negotiation from UI queue loss. It contains no media or device IDs.
- macOS idle cursor uses NSCursor.setHiddenUntilMouseMoves on state transitions. Restore on motion, menu, lost focus, leaving the viewport, capture stop and teardown. Duplicate pointer positions no longer reset the idle timer. The user subsequently confirmed that the idle cursor disappears and returns on movement in the final Mac bundle.
- Hardware GREEN after post-start configuration: Standard and FSR1 both delivered about 50 callbacks/s at 1920x1080, with active min/max durations equivalent to 50 FPS and zero native drops. UI usually measured 49-50 FPS; occasional UI stalls caused queue drops and are not a claim of flawless presentation. The card advertises a maximum of 50 FPS in this configuration, not 60.
- The first release workflow (43197498380) was canceled before publication when the user reported 20 FPS. Source commit 08807d7 alone is not the complete FPS/cursor fix.
