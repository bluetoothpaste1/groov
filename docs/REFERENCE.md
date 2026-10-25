# Справочник groov

Этот файл описывает форматы и значения, которые являются публичным контрактом groov. Для пошаговых объяснений смотрите [USER_GUIDE.md](USER_GUIDE.md), для написания расширений — [MODULES.md](MODULES.md).

## CLI

Общий синтаксис:

```text
groov [command] [arguments]
```

Без команды запускается TUI. `help`, `--help` и `-h` печатают краткую справку.

`groov version`, `groov --version` и `groov -V` печатают текущую версию.

| Команда | Аргументы | Результат |
|---|---|---|
| `tui` | нет | открыть плеер |
| `init` | нет | создать каталоги и default settings |
| `uninstall` | нет | удалить конфигурацию, кэш и всю локальную библиотеку после подтверждения |
| `paths` | нет | вывести XDG-пути |
| `doctor` | нет | вывести диагностику |
| `version` | нет | вывести версию |
| `list` | `--tag TAG` повторяемый, `--json` | список библиотеки |
| `scan` | нет | проверить библиотеку |
| `add` | `FILE` и опции импорта | импортировать аудио |
| `edit` | `ID` и metadata-опции | изменить песню |
| `theme` | `list`, `install FILE`, `use NAME` | управлять темами |
| `modules` / `module` | `list`, `install DIRECTORY`, `remove NAME`, `enable NAME`, `disable NAME`, `info NAME` | устанавливать и управлять модулями |

### `groov add`

```text
groov add FILE
groov add FILE --copy
groov add FILE --move
groov add FILE --dry-run
groov add FILE --id ID --title TITLE --artist ARTIST --album ALBUM
groov add FILE --desc DESCRIPTION --tag TAG --image COVER --lyrics LYRICS
```

`--tag` можно указывать несколько раз. `--copy` и `--move` взаимоисключающие по смыслу; последняя указанная опция побеждает. ID должен содержать только ASCII-буквы, цифры, `-` или `_`.

### `groov edit`

```text
groov edit ID --title TITLE
groov edit ID --artist ARTIST
groov edit ID --album ALBUM
groov edit ID --desc DESCRIPTION
groov edit ID --tag TAG
groov edit ID --untag TAG
groov edit ID --clear-tags
groov edit ID --image COVER
groov edit ID --lyrics LYRICS
```

`--tag` и `--untag` принимают comma-separated список. Изменение выполняется через временный JSON и rename.

### `groov list`

```text
groov list
groov list --tag rock --tag night
groov list --json
```

`--json` выводит normalized `SongInfo` с capitalized metadata keys.

### `groov uninstall`

```text
groov uninstall
```

Команда показывает предупреждение и пути `config`, `data` и `cache`, затем ждёт `y` или `yes`. Любой другой ответ отменяет операцию. После подтверждения удаляются все эти каталоги целиком: настройки, модули, темы, кэш, аудиофайлы, обложки, lyrics и metadata. Исполняемый файл groov команда не удаляет.

### `groov modules`

```text
groov modules
groov modules list
groov modules install DIRECTORY
groov module add DIRECTORY
groov modules remove NAME
groov modules enable NAME
groov modules disable NAME
groov modules on NAME
groov modules off NAME
groov modules info NAME
```

`install` и `add` копируют каталог с `module.json` в `$XDG_CONFIG_HOME/groov/modules/<name>`. `--force` заменяет уже установленный модуль. `remove` удаляет установленную копию и соответствующую запись из `settings.json`. `enable`/`disable` и их короткие алиасы `on`/`off` меняют поле `enabled`. `info` печатает source manifest path, working directory и serialized metadata.

### `groov theme`

```text
groov theme list
groov theme install FILE
groov theme use NAME
```

При установке имя берётся из top-level `name`, затем из filename stem. Допустимы ASCII-буквы, цифры, `-` и `_`.

## TUI modes and commands

The TUI has independent input and view states. `NORMAL` input navigates the full `NORMAL` view; `COMMAND` input is the temporary `:` prompt. The view command is:

```text
:mode
:mode normal
:mode compact
```

`:mode` reports the active view. `normal` restores the library, track details, timeline, and status layout. `compact` renders only the current track's cover and keeps playback controls active: `j/k` (or arrows) immediately start the next/previous visible track, `h/l` seek by `skip_seconds`, and `Space` toggles playback. `:follow on|off` controls whether the selected row follows tracks started by playback transitions; it is persisted in `settings.json` and defaults to off. The view mode itself is runtime state and is not persisted.

## Пути

Пусть:

```text
CONFIG_HOME = XDG_CONFIG_HOME или ~/.config
DATA_HOME   = XDG_DATA_HOME   или ~/.local/share
CACHE_HOME  = XDG_CACHE_HOME  или ~/.cache
```

Тогда:

```text
config_dir   = CONFIG_HOME/groov
settings     = CONFIG_HOME/groov/settings.json
data_dir     = DATA_HOME/groov
songs_dir    = DATA_HOME/groov/songs
cache_dir    = CACHE_HOME/groov
modules_dir  = CONFIG_HOME/groov/modules
themes_dir   = CONFIG_HOME/groov/themes
```

Все каталоги создаются командой `groov init` или автоматически командами, которым нужна запись.

## `settings.json`

Полный пример:

```json
{
  "refresh_ms": 100,
  "theme": "monochrome",
  "image_mode": "auto",
  "player_command": "mpv --no-video --no-terminal --audio-pitch-correction=yes --start={start} --input-ipc-server={player_ipc} {file}",
  "skip_seconds": 5,
  "repeat": false,
  "follow_playback": false,
  "cover_size": 16,
  "themes": {},
  "modules": [],
  "keymap": {
    "quit": "q",
    "play": "space",
    "stop": "x",
    "search": "/"
  }
}
```

### Поля settings

| Поле | Тип | Default | Описание |
|---|---|---|---|
| `refresh_ms` | unsigned integer | `100` | базовый интервал опроса событий; для active animation/tick host использует собственный deadline |
| `theme` | string | `monochrome` | активная тема |
| `image_mode` | string | `auto` | `auto`, `ansi`, `sixel`, `kitty`, `iterm2`, `none` |
| `player_command` | string | mpv live command when available, otherwise ffplay | shell command audio backend |
| `skip_seconds` | unsigned integer | `5` | шаг `h/l` |
| `repeat` | boolean | `false` | повтор текущего трека |
| `follow_playback` | boolean | `false` | перемещать выделение к начавшему играть треку |
| `cover_size` | unsigned integer | `16` | ширина обложки в cell; TUI ограничивает 4–64 |
| `themes` | object | `{}` | встроенные пользовательские палитры |
| `modules` | array | `[]` | inline module configurations |
| `keymap` | object | defaults | основные keyboard bindings |

Неизвестные top-level keys сохраняются в `extra`, поэтому новые клиенты могут расширять settings без разрушения старых значений.

### `keymap`

Текущие поля:

```json
{
  "quit": "q",
  "play": "space",
  "stop": "x",
  "search": "/"
}
```

Поддерживаемые значения для этих полей: single character, `enter`, `esc`/`escape`, `space`. Основные навигационные клавиши остаются Vim-like defaults. Дополнительные key bindings модулей описаны в [MODULES.md](MODULES.md).

## Темы

Встроенные имена:

```text
monochrome, ocean, gruvbox, nord, dracula
solarized, catppuccin, tokyo-night, transparent
```

Формат `ThemeConfig`:

```json
{
  "foreground": "#e6e6e6",
  "background": "#101010",
  "accent": "#5fd7ff",
  "muted": "#808080",
  "highlight": "#303030",
  "playing": "#5fff87",
  "border": "#606060"
}
```

Допустимы шестизначные hex colors и базовые names: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `gray`, `darkgray`, `reset`.

`transparent` — встроенная тема с `reset` для `background` и `highlight`, поэтому фон терминала остаётся прозрачным для groov. Остальные встроенные темы задают собственный фон.

Для удобства принимаются aliases:

| Canonical | Aliases |
|---|---|
| `foreground` | `fg`, `text` |
| `background` | `bg` |
| `accent` | `primary` |
| `muted` | `secondary` |
| `highlight` | `selection` |
| `playing` | `active` |
| `border` | `line` |

Тему можно разместить как `$XDG_CONFIG_HOME/groov/themes/name.json` или установить через CLI. Файл может быть прямым объектом `ThemeConfig` или оболочкой `{ "name": "my-theme", "colors": { ... } }`; в последнем случае имя берётся из поля `name`. Если одно имя есть и в settings, и в file, inline settings имеют приоритет. После добавления файла выполните `:reload` или перезапустите groov.

## Формат песни

Обязательные файлы:

```text
$XDG_DATA_HOME/groov/songs/ID/song.mp3
$XDG_DATA_HOME/groov/songs/ID/info.json
```

Опциональные:

```text
image.png, image.jpg, image.jpeg, image.webp, lyrics.txt
```

`SongInfo` использует следующие JSON keys:

```json
{
  "Id": "13",
  "Artist": "Artist",
  "Lyrics": "lyrics.txt",
  "Title": "Song",
  "Album": "Album",
  "Description": "Note",
  "Image": "image.jpg",
  "Tags": ["rock"],
  "Duration": 120,
  "AddedAt": "unix-seconds"
}
```

ID каталога считается authoritative. Tags normalized to lowercase, sorted, deduplicated. `Description` также принимает старый alias `Desc`.

## Player contract

`player_command` запускается через shell. groov:

1. подставляет `{file}` shell-quoted путём;
2. подставляет `{start}` числом секунд;
3. подставляет `{audio_filter}` как `-af 'FILTER'`, либо пустую строку без активного фильтра;
4. подставляет `{player_ipc}` путём локального IPC endpoint;
5. если `{file}` отсутствует, добавляет quoted path в конец;
6. если выполняется seek и команда содержит `ffplay`, а `{start}` отсутствует, добавляет `ffplay -ss N`;
7. если выполняется seek и команда содержит `mpv`, а `{start}` отсутствует, добавляет `mpv --start=N`;
8. запускает команду с `exec`, чтобы signal pause/stop адресовался реальному player process.

Модули могут менять эти параметры через JSONL response `type: "playback"`; полный контракт и список действий находятся в [MODULES.md](MODULES.md). Если `{audio_filter}` не указан, непустой фильтр автоматически добавляется как `-af` только для команд, содержащих `ffplay`; другие backend-команды должны явно предоставить placeholder отдельным аргументом.

stdin, stdout и stderr процесса изолированы. Пауза/продолжение используют `SIGSTOP`/`SIGCONT` на Unix. На других платформах pause/resume может быть недоступен.

## Module environment contract

Для playback event hook:

| Variable | Значение |
|---|---|
| `GROOV_API_VERSION` | `1` |
| `GROOV_EVENT` | `play`, `stop`, `rescan`, `metadata` |
| `GROOV_EVENT_JSON` | JSON object с event и song fields |
| `GROOV_SONG_ID` | ID, если есть song |
| `GROOV_SONG_FILE` | absolute/normalized path, если есть song |
| `GROOV_PLAYER_IPC` | путь к локальному control endpoint для backend, если он поддерживает IPC |
| `GROOV_TERMINAL` | `1` для command alias с `terminal: true` |

Если module field `rpc` равен true, stdin/stdout используются JSONL protocol. При `session: true` один process принимает последовательность requests, включая input/key messages; request/response schema и declarative widgets описаны в [MODULES.md](MODULES.md).

RPC response `type: "canvas"` рисует cell-based слой поверх host UI. `canvas_patch` обновляет только named commands, а `animation` передаёт host таблицу кадров для локального frame loop. Полный список операций и примеры находятся в [MODULES.md](MODULES.md). Manifest field `terminal: true` передаёт указанный command alias отдельному процессу с inherited stdin/stdout/stderr, чтобы модуль мог запустить собственный Ratatui/crossterm UI; `fps` включает host `tick` requests для session-модулей.

Session requests `resize`, `tick`, `input` с `event: "mouse"` и `shutdown` описаны в [MODULES.md](MODULES.md). Mouse capture включается только для manifest с `mouse: true`.

`GROOV_EVENT_JSON` сейчас содержит:

```json
{
  "event": "play",
  "id": "13",
  "file": "/path/to/song.mp3",
  "title": "Song",
  "artist": "Artist"
}
```

Для command alias `GROOV_EVENT` имеет вид `command:<alias>`.

## Placeholders

В `args` и `env` module config:

| Placeholder | Значение |
|---|---|
| `{event}` | event name |
| `{id}` | song ID, если song передан |
| `{file}` | путь к audio file, если song передан |

Если song отсутствует, `{event}` заменяется, а остальные placeholders остаются как literal text.
