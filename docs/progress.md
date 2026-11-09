# Результат реализации 0.1.1

План: superpowers/plans/2026-11-09-console-link.md.

- Реализованы native video adapters, CPAL native audio, bounded history, separate game viewport, controls, PNG, settings, Discord IPC и reconnect.
- Сохранены решения интеграции: CPAL вместо дублирования realtime FFI; CPU conversion/upload вместо zero-copy; PNG encoder вместо двойной платформенной обвязки.
- Source-clock synchronization заменена консервативными host delivery timestamps и audio queue feedback в первой версии. Это ограничивает точность A/V относительно полной архитектуры; аппаратная калибровка обязательна.
- Независимое ревью и регрессионные исправления выполнены. Проверены 33 Mac tests, Clippy Mac/Windows cross-target, настоящий запуск .app с Metal.
- Код опубликован: https://github.com/Potanull/console-link. Native macOS и Windows MSVC CI завершились success: https://github.com/Potanull/console-link/actions/runs/42643094504.
- Аппаратная приемка и реальный Discord-звонок остаются открытыми: см. validation.md.

## Релиз 0.2.0

Новый UI, bundled Inter, Windows per-user installer и публичные downloads опубликованы: https://github.com/Potanull/console-link/releases/tag/v0.2.0. Нативная матрица и install/reinstall/uninstall smoke success: https://github.com/Potanull/console-link/actions/runs/42667922070. Публичные файлы скачаны и сверены по SHA256.
