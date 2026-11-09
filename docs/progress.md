# Результат реализации 0.1.1

План: superpowers/plans/2026-11-09-console-link.md.

- Реализованы native video adapters, CPAL native audio, bounded history, separate game viewport, controls, PNG, settings, Discord IPC и reconnect.
- Сохранены решения интеграции: CPAL вместо дублирования realtime FFI; CPU conversion/upload вместо zero-copy; PNG encoder вместо двойной платформенной обвязки.
- Source-clock synchronization заменена консервативными host delivery timestamps и audio queue feedback в первой версии. Это ограничивает точность A/V относительно полной архитектуры; аппаратная калибровка обязательна.
- Независимое ревью и регрессионные исправления выполнены. Проверены 33 Mac tests, Clippy Mac/Windows cross-target, настоящий запуск .app с Metal.
- Публичная публикация и native Windows CI проверяются отдельно.
- Аппаратная приемка и реальный Discord-звонок остаются открытыми: см. validation.md.
