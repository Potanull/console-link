# Подключение аккаунта Discord

## Назначение

OAuth подтверждает личность пользователя (`identify`). Статус игры отправляется отдельно через локальный RPC настольного Discord. OAuth `identify` сам по себе не дает права изменять активность. Перед отправкой статуса идентификатор профиля OAuth сравнивается с `READY.data.user.id` настольного клиента. При несовпадении активность не отправляется.

Зарегистрированный публичный Application ID Console Link: `1569416573180051537`. Это не секрет. Public Client должен быть включен, а разрешенный redirect должен точно совпадать с `http://127.0.0.1:45961/discord/callback`. Client Secret и токен бота не используются.

## Реализация

- OAuth2 Authorization Code + PKCE S256 через поддерживаемую библиотеку `oauth2` 5.
- Только `identify`; partner scopes `rpc` / `rpc.activities.write` не запрашиваются.
- HTTP через `reqwest` с нативным TLS: SChannel на Windows и Security Framework на macOS. Проверка сертификатов включена, сетевые redirects запрещены, connect timeout 4 с, общий timeout 8 с.
- Приемник callback привязан только к `127.0.0.1:45961`. Проверяются GET, path, Host, state, дубли query-полей, размер заголовка и срок ожидания. Сравнение state использует timing-resistant traits OAuth-библиотеки.
- После callback браузер заменяет URL на путь без authorization code. Статический script разрешен только по CSP hash; параметры входа не вставляются в HTML.
- Вход, восстановление сессии и refresh выполняются на одном owner-потоке. UI не блокируется; отмена меняет generation и не дает запоздалому результату связать старый аккаунт.
- Refresh token сохраняется только через Keychain/Credential Manager. Access token используется в памяти для проверки профиля. В JSON настроек, логах и публичном API токенов нет. При недоступном защищенном хранилище не используется файловый fallback.
- Отключение сразу убирает публичную identity из UI/API и запрещает presence. Owner-поток удаляет credential, затем пытается отозвать refresh token на сервере. Серверный revoke является best effort: локальное отключение не обещает удаление записи из списка Authorized Apps при недоступном Discord.
- При временной ошибке refresh существующий credential не удаляется. Периодическая проверка повторяется через 30 с; при подтвержденном `invalid_grant` credential очищается. Начальное восстановление после сетевой ошибки также повторяется через 30 с без открытия браузера. При ротации новый refresh token сохраняется до проверки профиля и остается в памяти при ошибке профиля или хранилища.

## API для UI

`DiscordAuth::new(application_id)` создает worker, но не открывает браузер без `connect()`. `status()` возвращает `Disconnected`, `Connecting`, `Connected(DiscordAccount)` или `Error`. `account()` отдает только подтвержденные публичные поля `id` и `display_name`.

В диагностическом snapshot-режиме manager создавать нельзя: обычный startup читает защищенный credential для восстановления сессии.

`DiscordWorker::require_account(None)` включает fail-closed режим и очищает активность. При подключенном аккаунте передается `Some(account.id)`. До первого вызова метод сохраняет старое поведение IPC для совместимости; приложение должно вызвать его при старте.

## Проверка

Unit tests проверяют callback origin/path/state/дубли/лимиты, отказ пользователя, PKCE URL, form без Client Secret/Basic Auth, отмену старого поколения и отсутствие секрета в Debug. IPC tests проверяют извлечение identity из READY и блокировку несовпадающего/отсутствующего аккаунта.

Mac tests и Clippy, Windows cross-target Clippy проверяют код. Реальный браузерный grant, обмен с Discord, Keychain/Credential Manager, refresh и revoke требуют отдельной проверки с аккаунтом владельца. Успешная компиляция не доказывает, что raw secretless flow поддерживается конкретной конфигурацией Discord Application.

## Источники

- [OAuth authorization server discovery](https://discord.com/.well-known/oauth-authorization-server): объявляет S256 и token endpoint auth method `none`.
- [Discord OAuth2](https://docs.discord.com/developers/topics/oauth2).
- [Discord RPC READY](https://docs.discord.com/developers/topics/rpc#ready).
- [Rich Presence](https://docs.discord.com/developers/discord-social-sdk/development-guides/setting-rich-presence).
- [oauth2-rs](https://github.com/ramosbugs/oauth2-rs), [keyring-rs](https://github.com/hwchen/keyring-rs), [reqwest TLS](https://docs.rs/reqwest/latest/reqwest/tls/index.html).
