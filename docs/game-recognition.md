# Локальное определение игры

Console Link периодически читает название игры на кадре карты захвата. Это OCR титульного экрана или видимой подписи в меню Switch, а не классификация произвольного геймплея. Кадры и распознанный текст не отправляются в сеть и не сохраняются. Обработка выполняется отдельно от захвата, звука и отрисовки.

Не чаще раза в четыре секунды worker принимает ссылку на последний кадр. Очередь содержит максимум один ожидающий кадр. В рабочем потоке изображение уменьшается до 1280 пикселей по длинной стороне, затем локальный OCR читает текст. Название должно целиком совпасть с именем или региональным вариантом из каталога; разные несвязанные названия на одном экране считаются неоднозначностью. Предложение появляется после двух последовательных совпадений на разных кадрах одной сессии. Отсутствие названия в обычном геймплее не означает смену игры. Ручной выбор имеет приоритет в интерфейсе приложения.

Отключение очищает очередь и результаты, меняет поколение работы и отменяет последующую загрузку моделей. Уже выполняющийся сетевой вызов ограничен общим таймаутом 20 секунд; после возврата из чтения отмененная загрузка не пишет очередной блок и удаляет временный файл. Саму уже начавшуюся OCR-инференцию прервать нельзя, но ее результат отбрасывается. Результаты старых сессий тоже отбрасываются.

## Модели

При первом включенном анализе реального кадра загружаются только две модели, всего 12 212 656 байт. До первого кадра запросов нет. Затем модели хранятся в каталоге кеша, переданном приложением, и работают без интернета. Размер и SHA256 проверяются до загрузки модели в память. Перенаправления разрешены только через HTTPS, максимум пять; сообщения об ошибках не содержат подписанных CDN URL. Картинка для обложки загружается интерфейсом отдельно.

Автор моделей: Robert Knight, проект [ocrs-models](https://github.com/robertknight/ocrs-models). Используются актуальные ONNX-веса из [официального CLI](https://github.com/robertknight/ocrs/blob/main/ocrs-cli/src/main.rs). Лицензия весов: [Creative Commons Attribution-ShareAlike 4.0](https://creativecommons.org/licenses/by-sa/4.0/), [подтверждение автора](https://github.com/robertknight/ocrs-models/issues/34). Модели используются без изменений. Обучающий набор HierText опубликован Google Research под CC BY-SA 4.0. Лицензия движка [ocrs](https://github.com/robertknight/ocrs): MIT OR Apache-2.0; движка [RTen](https://github.com/robertknight/rten): MIT OR Apache-2.0.

Базовый URL: `https://ocrs-models.s3-accelerate.amazonaws.com/`. Содержимое зафиксировано размером и SHA256. Если upstream изменит файл по этому URL, приложение откажется использовать его до проверенного обновления контрольных сумм.

| Файл | Байт | SHA256 |
| --- | ---: | --- |
| text-detection.onnx | 2499479 | a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe |
| text-recognition.onnx | 9713177 | 86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5 |

Модели читают латиницу. Каталог содержит и японские, и русские варианты названий, но это не расширяет алфавит OCR. Стилизованные логотипы, обрезанные подписи, плохо читаемый текст и игры без видимого названия могут не определиться. В таком случае доступен ручной поиск. Скорость и нагрузка зависят от CPU; отсутствие влияния на каждый компьютер не гарантируется. Для замера OCR нужна release-сборка, отладочная инференция существенно медленнее.

## Каталог

В `assets/games.json` находится полный снимок записей базовых игр из шести региональных каталогов [TitleDB](https://github.com/blawar/titledb): US.en, GB.en, JP.ja, RU.ru, FR.fr, DE.de. Зафиксирован commit `d424690446eec6f4484209b9a1c067c7f7511398`. Снимок содержит **24 017** уникальных базовых Title ID и **8655** дополнительных региональных названий. Это каталог источника, а не выбранный вручную набор игр. Абсолютная полнота всех мировых релизов зависит от TitleDB и охвата регионов.

Фильтрация: корректный шестнадцатеричный Title ID, младшие 13 бит равны нулю, `isDemo` выключен, название непустое. Обновления и DLC исключены по структуре Title ID. Дубликаты Title ID объединены; полные различные региональные имена сохранены в aliases. Убраны описания, скриншоты и другие ненужные поля. Ссылки обложек взяты из `iconUrl`/`frontBoxArt` источника; права на изображения принадлежат соответствующим правообладателям. Обложки в репозиторий не включены. Официальный URL магазина не выдумывается: пока источник его не предоставляет, `game_url` равен null.

Ревизии, URL и SHA256 исходных JSON записаны в `assets/games-sources.json`. Обновление выполняется только при разработке:

```sh
python3 scripts/update-game-catalog.py --cache /path/to/build-time-cache
```

Python не нужен пользователю приложения. Скрипт загружает большие исходные каталоги в явно указанный кеш, в приложение включается только компактный результат.

TitleDB предоставлен под MIT, Copyright (c) 2019 Blake Warner:

```text
MIT License

Copyright (c) 2019 Blake Warner

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Проверка реального OCR

Обычные unit-тесты не используют интернет. Отдельный тест ниже загружает проверяемые модели при отсутствии кеша, рисует название игры шрифтом из приложения, сохраняет синтетический PNG в тестовый кеш и действительно пропускает его через OCR и matcher:

```sh
CONSOLE_LINK_OCR_TEST_CACHE=/path/to/test-cache \
  cargo test --release --lib game_recognition::tests::real_ocr_recognizes_a_rendered_title_screen -- --ignored --nocapture
```

Этот тест проверяет цепочку OCR, но не качество определения произвольных кадров реальной карты захвата.
