# Руководство пользователя groov

## Что такое groov

groov — локальный музыкальный плеер для терминала. Музыка хранится в обычных каталогах, метаданные — в JSON, обложки и тексты — рядом с треком. Интерфейс имеет два режима отображения и отдельное состояние ввода команд:

- `NORMAL` — библиотека, очередь и полная панель управления;
- `COMPACT` — только обложка текущего трека;
- `COMMAND` — временный ввод команд после `:` или поиск после `/`.

Поиск и справка не являются отдельными режимами: поиск выполняется командой `search`, а справка открывается временной панелью. Это упрощает настройку и расширение поведения.

Воспроизведение передаётся внешней программе. По умолчанию используется `ffplay`, но команду можно заменить на любой backend, который принимает путь к файлу.

## Установка зависимостей

Для сборки нужны:

- Rust и Cargo;
- FFmpeg (`ffplay`) для воспроизведения;
- FFmpeg (`ffprobe`) необязателен, но позволяет автоматически читать title, artist, album и duration.

На macOS:

```sh
brew install ffmpeg python
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

На Debian или Ubuntu:

```sh
sudo apt update
sudo apt install --yes ffmpeg python3
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

Если `rustc --version` и `cargo --version` уже работают, установщик Rust повторно запускать не нужно. `ffprobe` необязателен, а Python 3 нужен только для примеров `basic_modules`. Если macOS сообщает об отсутствующем linker или `cc`, установите только Command Line Tools: `xcode-select --install`; полное приложение Xcode не требуется.

Сборка:

```sh
make
```

Бинарник появится в `target/debug/groov`. Сборка и тесты:

```sh
make test
cargo test
cargo clippy --all-targets -- -D warnings
```

Для локальной установки release binary и man page:

```sh
make release
make install PREFIX="$HOME/.local"
```

То же самое можно сделать готовым скриптом из корня репозитория:

```sh
./install.sh
# или
PREFIX="$HOME/.local" ./install.sh
```

Скрипт собирает release binary и устанавливает man page без `sudo`, если используется пользовательский prefix.

После установки добавьте `$HOME/.local/bin` в `PATH`, если его там ещё нет:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Удаление установленных файлов:

```sh
make uninstall PREFIX="$HOME/.local"
```

## Первый запуск

Создайте каталоги и файл настроек:

```sh
groov init
```

После этого добавьте аудиофайл:

```sh
groov add ~/Downloads/song.mp3
```

По умолчанию исходный файл перемещается в библиотеку. Чтобы сохранить оригинал:

```sh
groov add ~/Downloads/song.mp3 --copy
```

Перед импортом можно посмотреть результат без изменений:

```sh
groov add ~/Downloads/song.mp3 --dry-run
```

Откройте плеер:

```sh
groov
# или
groov tui
```

## Каталоги и переменные окружения

groov использует XDG Base Directory:

| Назначение | По умолчанию | Переопределение |
|---|---|---|
| Настройки | `~/.config/groov` | `XDG_CONFIG_HOME` |
| Библиотека | `~/.local/share/groov` | `XDG_DATA_HOME` |
| Кэш | `~/.cache/groov` | `XDG_CACHE_HOME` |

Полные пути можно вывести:

```sh
groov paths
groov --version
```

Внутри каталогов groov используются:

```text
$XDG_CONFIG_HOME/groov/settings.json
$XDG_CONFIG_HOME/groov/modules/<name>/module.json
$XDG_CONFIG_HOME/groov/themes/<name>.json
$XDG_DATA_HOME/groov/songs/<id>/
$XDG_CACHE_HOME/groov/
```

Если XDG-переменные не заданы, groov использует `HOME`. Относительные пути в командах интерпретируются относительно текущего рабочего каталога процесса.

## Импорт музыки

Полная форма:

```sh
groov add FILE [--move|--copy] [--dry-run] \
  [--id ID] [--title TITLE] [--artist ARTIST] [--album ALBUM] \
  [--desc DESCRIPTION] [--tag TAG] [--image COVER] [--lyrics FILE]
```

Опции:

- `--move` — переместить исходный файл; это поведение по умолчанию;
- `--copy` — оставить исходный файл;
- `--dry-run` — показать план импорта без записи;
- `--id ID` — задать ID вручную; допустимы буквы, цифры, `-` и `_`;
- `--title`, `--artist`, `--album`, `--desc` — переопределить метаданные;
- `--tag TAG` — добавить тег; опцию можно повторять;
- `--image FILE` — скопировать JPG, JPEG, PNG или WebP;
- `--lyrics FILE` — скопировать текст песни.

Если доступен `ffprobe`, groov сначала читает embedded metadata. Явные опции командной строки имеют приоритет. Title без metadata берётся из имени файла.

Импорт выполняется через временный каталог и атомарное переименование. Неполный импорт не должен появиться как рабочая песня.

## Формат библиотеки

Каждый трек — самостоятельный каталог:

```text
songs/13/song.mp3
songs/13/info.json
songs/13/image.jpg
songs/13/lyrics.txt
```

Минимально нужны `song.mp3` и `info.json`. Поддерживаемые имена обложки по умолчанию: `image.png`, `image.jpg`, `image.jpeg`, `image.webp`. Текст по умолчанию — `lyrics.txt`.

Пример `info.json`:

```json
{
  "Id": "13",
  "Artist": "Artist name",
  "Lyrics": "lyrics.txt",
  "Title": "A title",
  "Album": "An album",
  "Description": "A short note",
  "Image": "image.jpg",
  "Tags": ["rock", "night"],
  "Duration": 245,
  "AddedAt": "1710000000"
}
```

Поля metadata:

| Поле | Тип | Назначение |
|---|---|---|
| `Id` | string | стабильный ID каталога |
| `Artist` | string | исполнитель |
| `Lyrics` | string | относительный путь к тексту |
| `Title` | string | название |
| `Album` | string | альбом |
| `Description` | string | описание |
| `Image` | string | относительный путь к обложке |
| `Tags` | array | список тегов |
| `Duration` | unsigned integer | длительность в секундах |
| `AddedAt` | string | время добавления |

При загрузке ID берётся из имени каталога. Пустые title и artist заменяются на `Untitled` и `Unknown artist`. Теги обрезаются, переводятся в lowercase, сортируются и очищаются от дубликатов.

## Просмотр и редактирование библиотеки

Список песен:

```sh
groov list
groov list --tag rock --tag night
groov list --json
```

Несколько `--tag` используют AND-семантику: песня должна иметь каждый указанный тег.

Проверка библиотеки:

```sh
groov scan
groov doctor
```

Полное удаление пользовательских данных:

```sh
groov uninstall
```

Команда показывает предупреждение и три каталога, которые будут удалены: конфигурацию, данные библиотеки и кэш. Для продолжения введите `y` или `yes`; любой другой ответ отменяет удаление. Будут удалены треки, обложки, metadata, lyrics, модули, темы и настройки. Сам исполняемый файл groov остаётся на месте.

Редактирование:

```sh
groov edit 13 --title "New title"
groov edit 13 --artist "Group" --album "Live"
groov edit 13 --tag live --untag draft
groov edit 13 --clear-tags
groov edit 13 --image ~/Pictures/cover.png --lyrics ~/Music/lyrics.txt
```

Редактирование metadata записывается атомарно. Исходные файлы обложки и текста не изменяются.

## Управление TUI

### NORMAL

| Клавиша | Действие |
|---|---|
| `j`, `Down` | вниз |
| `k`, `Up` | вверх |
| `gg` | в начало списка |
| `G` | в конец списка |
| `Enter` или `Space` | начать/поставить на паузу выбранный трек |
| `x` | остановить |
| `h` | перемотать назад на `skip_seconds` |
| `l` | перемотать вперёд на `skip_seconds` |
| `n`, `]` | следующий трек |
| `[` | предыдущий трек |
| `a` | добавить выбранный трек в очередь |
| `/` | начать поиск |
| `:` | открыть COMMAND |
| `?` | открыть справку |
| `q` | выйти |

Модуль может добавить свои key bindings; см. [MODULES.md](MODULES.md).

### COMPACT

Включите компактное отображение командой `:mode compact`. На экране остаётся только обложка текущего трека; заголовок, список, метаданные, таймлайн и статус скрыты. Команды воспроизведения остаются доступными:

| Клавиша | Действие |
|---|---|
| `j` или `Down` | сразу включить следующий видимый трек |
| `k` или `Up` | сразу включить предыдущий видимый трек |
| `h` | перемотать назад на `skip_seconds` |
| `l` | перемотать вперёд на `skip_seconds` |
| `Space` | начать или поставить воспроизведение на паузу |
| `:` | открыть COMMAND для команды |

Вернуться к полной панели можно командой `:mode normal`. `:mode` без аргумента показывает текущий режим. Режим действует до выхода из groov и не изменяет `settings.json`.

### COMMAND

COMMAND отображается с префиксом `:`. Значения в кавычках поддерживаются:

```text
:set title "Title with spaces"
:tag "live set"
```

`Esc` отменяет ввод. `Enter` выполняет команду. `Tab` дополняет команды, поля `set`, подкоманды modules и пути к обложке/тексту.

### Поиск и фильтры

```text
/dark
:search dark
:tags rock night
:clear
```

`/dark` открывает COMMAND с заготовкой `search`. `:tags` меняет фильтр по тегам, а `:clear` очищает и поиск, и теговый фильтр.

### Воспроизведение и очередь

```text
:play
:stop
:next
:prev
:queue
:queue visible
:queue clear
:enqueue
```

При начале трека groov добавляет видимую библиотеку в очередь, если очередь пуста. `:queue visible` заменяет очередь текущим видимым списком. `:next` пропускает треки, скрытые текущим фильтром.

Повтор текущего трека:

```text
:repeat on
:repeat off
:repeat
```

`:repeat` без аргумента показывает состояние. Настройка сохраняется в `settings.json`.

Следить выделением за воспроизведением:

```text
:follow on
:follow off
:follow
```

При `:follow on` выделенный ряд автоматически перемещается к треку, который начал играть, в том числе при переходе очереди после окончания песни. `:follow` без аргумента показывает состояние. Настройка сохраняется в `settings.json`; по умолчанию она выключена.

### Metadata-команды

```text
:tag live
:untag draft
:tags rock night
:set title "New title"
:set artist "Artist"
:set album "Album"
:set desc "Description"
:set duration 245
:set image ~/Pictures/cover.jpg
:set lyrics ~/Music/lyrics.txt
```

Поле `image` и `lyrics` копирует файл внутрь каталога песни. Пути с `~` раскрываются через `HOME`.

### Настройки из TUI

```text
:set skips 10
:set coversize 20
:set theme ocean
:themes
:reload
```

`:reload` перечитывает настройки, пользовательские темы и манифесты модулей без перезапуска процесса.

### Модули из TUI

```text
:modules
:modules list
:modules on hello
:modules off hello
:modules info hello
```

`:modules` и `:modules list` открывают список. `:modules info` открывает подробную панель; любая клавиша закрывает её.

Для обычных env-модулей используйте examples/modules/hello. Для JSONL RPC и declarative UI views используйте examples/modules/rpc_dashboard.

Для готовых независимых расширений используйте [`basic_modules`](../basic_modules/README.md):

```sh
groov modules install basic_modules/flame
groov modules install basic_modules/easyadd
groov modules enable flame
groov modules enable easyadd
```

`groov module add DIRECTORY` — короткий алиас для `modules install`. Удаление выполняется через `groov modules remove NAME`, а временное выключение — через `groov modules disable NAME`; вернуть модуль можно командой `groov modules enable NAME`.

## Темы

Встроенные темы:

```text
monochrome
ocean
gruvbox
nord
dracula
solarized
catppuccin
tokyo-night
transparent
```

Использование:

```sh
groov theme list
groov theme use ocean
groov theme install ./my-theme.json
```

TUI-команда:

```text
:set theme ocean
```

Пользовательская тема может быть отдельным файлом `$XDG_CONFIG_HOME/groov/themes/name.json` или объектом в `settings.json`. После добавления файла перезапустите groov или выполните `:reload`, затем выберите имя через `groov theme use NAME` или `:set theme NAME`. Можно использовать как прямой объект цветов, так и оболочку `{ "name": "my-theme", "colors": { ... } }`. Формат описан в [REFERENCE.md](REFERENCE.md).

Тема `transparent` использует `reset` для фона и выделения: терминал сохраняет собственный фон, а интерфейс рисует светло-серый текст, акценты и тонкие линии.

Минимальная пользовательская тема:

```json
{
  "name": "sunset",
  "colors": {
    "foreground": "#fff4e6",
    "background": "reset",
    "accent": "#ff9f43",
    "muted": "#b8a99a",
    "highlight": "reset",
    "playing": "#ffd166",
    "border": "#d6c2b5"
  }
}
```

Сохраните файл как `$XDG_CONFIG_HOME/groov/themes/sunset.json`, выполните `:reload` и выберите `:set theme sunset`.

## Обложки и терминалы

`image_mode: "auto"` использует ANSI halfblocks и не угадывает графический протокол по имени терминала. Это безопасный вариант для Linux, macOS, SSH, tmux и терминалов IDE.

Явные режимы:

- `ansi` или `ascii` — ANSI halfblocks;
- `sixel` — Sixel;
- `kitty` — Kitty graphics;
- `iterm2` или `iterm` — iTerm2;
- `none` — отключить обложки.

Для неизвестного терминала оставляйте `auto`. Обработка изображения выполняется в worker thread, поэтому большая обложка не должна блокировать навигацию.

## Аудиобэкенд

В `settings.json`:

```json
{
  "player_command": "ffplay -nodisp -autoexit -loglevel quiet {file}"
}
```

Поддерживаются placeholders:

- `{file}` — shell-quoted путь к треку;
- `{start}` — начальная позиция в секундах.

Если `{file}` отсутствует, groov добавит путь в конец команды. Для seek у стандартного `ffplay` groov автоматически добавляет `-ss`, если в команде нет `{start}`.

stdout, stderr и stdin аудиобэкенда изолированы, чтобы backend не портил интерфейс. Для диагностики добавьте перенаправление прямо в `player_command`.

## Диагностика

```sh
groov doctor
groov paths
groov scan
groov modules
groov theme list
```

Частые проблемы:

### Плеер не запускается

Проверьте:

```sh
command -v ffplay
ffplay -version
groov doctor
```

Укажите свой backend через `player_command`, если используете другой проигрыватель.

### Трек не появился

Проверьте наличие:

```text
$XDG_DATA_HOME/groov/songs/<id>/song.mp3
$XDG_DATA_HOME/groov/songs/<id>/info.json
```

Запустите `groov scan`. Повреждённые или неполные каталоги пропускаются с сообщением в stderr.

### Терминал показывает мусор

Установите `image_mode: "auto"` или `"ansi"`. Не выбирайте Kitty/Sixel/iTerm2, если терминал действительно не поддерживает этот протокол. Перезапустите groov после изменения `settings.json`.

### Модуль не виден

Проверьте расположение:

```text
$XDG_CONFIG_HOME/groov/modules/<name>/module.json
```

Затем выполните `:reload` или перезапустите groov. `groov modules info NAME` покажет, какой манифест был найден.

## Безопасность и границы

Модули и audio backend — это внешние процессы. groov запускает их с правами текущего пользователя. Манифестам и командам нужно доверять так же, как shell-скриптам. groov не выполняет произвольный код из metadata песни, но `command`, `args`, `env` и `player_command` являются пользовательской конфигурацией.

Внешний модуль не может напрямую изменить внутренние структуры TUI. Он взаимодействует через стабильный JSON-манифест, environment contract, события, команды и key bindings. Это сохраняет процесс живым при падении модуля и позволяет писать дополнения на любом языке.
