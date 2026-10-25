# Разработка модулей и дополнений groov

Модуль groov — это внешний процесс с JSON-манифестом. Модуль не обязан быть написан на Rust: подойдут shell, Python, Lua, Go, Node.js, Rust или любой другой язык, который умеет запускать процесс и читать environment variables.

Модуль может:

- реагировать на playback и metadata events;
- добавлять команды в COMMAND mode;
- добавлять keyboard bindings в NORMAL mode;
- получать ID, путь, title и artist текущей песни;
- использовать собственные environment variables;
- работать из собственного каталога;
- хранить любые дополнительные metadata;
- отправлять JSONL RPC-ответы, из которых groov рендерит status, overlays, paragraphs, lists, gauges и declarative widget trees;
- рисовать произвольные cell-based canvas layers поверх обычного интерфейса: текст, цвета, заливки, линии, рамки и процентную геометрию;
- запускать host-driven animations: описание кадров отправляется один раз, а groov исполняет его локально с частотой до 240 FPS;
- обновлять canvas через маленькие `canvas_patch`-сообщения вместо передачи всего слоя;
- открывать интерактивные session views, закрывать их и просить groov пересканировать библиотеку;
- запускать отдельную terminal UI с прямым владением экраном, если host-composited canvas недостаточно;
- управлять playback через общий `playback` response: перезапускать текущий backend с фильтром, менять позицию, паузу и остановку;
- временно менять цвет границ через `palette` response;
- включаться, выключаться и перечитываться без перезапуска groov.

Это поведенческий plugin API. Модуль намеренно не загружается внутрь процесса groov и не получает прямой доступ к внутренним Rust-структурам. Процессная граница предотвращает падение TUI из-за ошибки расширения и позволяет сохранять совместимость между языками и версиями.

## Быстрый старт

Создайте каталог:

~~~sh
mkdir -p "$XDG_CONFIG_HOME/groov/modules/hello"
~~~

Если XDG_CONFIG_HOME не задан, используйте ~/.config.

Создайте module.json:

~~~json
{
  "name": "hello",
  "version": "1.0.0",
  "description": "A small example module",
  "author": "You",
  "enabled": true,
  "command": "sh",
  "args": ["hello.sh"],
  "events": ["play", "stop"],
  "commands": [
    { "name": "hello", "args": ["command"] }
  ],
  "keys": {
    "H": "hello"
  },
  "env": {
    "HELLO_MODULE": "1"
  }
}
~~~

Создайте рядом hello.sh:

~~~sh
#!/bin/sh
printf 'event=%s id=%s file=%s\n' \
  "$GROOV_EVENT" \
  "$GROOV_SONG_ID" \
  "$GROOV_SONG_FILE" >> "$HOME/groov-hello.log"
~~~

Проверьте манифест:

~~~sh
groov modules info hello
~~~

Перечитайте конфигурацию:

~~~text
:reload
~~~

Теперь:

~~~text
:modules info hello
:modules on hello
:hello
~~~

Нажатие H в NORMAL mode вызывает ту же команду.

Готовый минимальный пример находится в examples/modules/hello.

Полноценные out-of-tree examples находятся в `basic_modules/`: `flame` демонстрирует streaming palette updates, `easyadd` — interactive session view, input handling, atomic library writes и rescan request, а `speedr` — generic playback control через audio filter или live IPC backend.

## Где groov ищет модули

Автоматически сканируется один уровень:

~~~text
$XDG_CONFIG_HOME/groov/modules/<module-name>/module.json
~~~

Каждая подпапка с module.json становится одним module entry. Папки без манифеста игнорируются. Некорректный JSON пропускается; остальные модули продолжают загружаться.

Модули также можно указать inline в settings.json в массиве modules. Inline-конфигурация и обнаруженный манифест с одинаковым name объединяются. Явные непустые поля из settings.json имеют приоритет, а отсутствующие поля дополняются манифестом.

Манифест получает runtime source path. Он не записывается обратно в settings.json, но показывается через modules info. Если источник не найден, info показывает путь к settings.json.

## Полная схема манифеста

~~~json
{
  "name": "module-name",
  "version": "1.0.0",
  "description": "What the module does",
  "author": "Author or team",
  "homepage": "https://example.invalid/module",
  "enabled": true,
  "rpc": true,
  "session": true,
  "terminal": false,
  "fps": 0,
  "command": "python3",
  "args": ["main.py", "{event}", "{id}"],
  "events": ["play", "stop", "metadata"],
  "commands": [
    { "name": "module-command", "args": ["toggle", "{file}"] }
  ],
  "keys": {
    "v": "module-command",
    "ctrl-v": "module-command"
  },
  "env": {
    "MODULE_LOG_LEVEL": "info",
    "MODULE_ID": "{id}"
  },
  "cwd": "/optional/absolute/path",
  "custom_key": "preserved"
}
~~~

### Поля

| Поле | Тип | Default | Назначение |
|---|---|---|---|
| name | string | обязательно | стабильное имя и lookup key |
| version | string | пустая строка | версия модуля для info и совместимости |
| description | string | пустая строка | описание |
| author | string | пустая строка | автор |
| homepage | string | пустая строка | ссылка на проект |
| enabled | boolean | false | отправлять events и разрешать commands |
| rpc | boolean | false | включить JSONL request/response protocol |
| session | boolean | false | держать RPC process живым между сообщениями |
| terminal | boolean | false | передавать терминал отдельному command alias до его завершения |
| fps | unsigned integer | `0` | частота host `tick` requests для session-модуля; диапазон 1..240 |
| command | string или null | null | executable, который запускается |
| args | array<string> | [] | аргументы event hook |
| events | array<string> | [] | фильтр событий; пустой список означает все |
| commands | array | [] | команды, доступные в COMMAND mode |
| keys | object | {} | key → command line |
| env | object<string,string> | {} | дополнительные переменные процесса |
| cwd | string или null | manifest dir | рабочий каталог |
| unknown keys | any JSON | — | сохраняются в extra |

command обязателен только для реально запускаемого модуля. Manifest-only entry можно использовать как описание или заготовку, но он не будет выполнять hooks.

## Жизненный цикл

### Discovery

При запуске groov или команде :reload:

1. читается settings.json;
2. сканируются modules/*/module.json;
3. модули с одинаковым именем объединяются;
4. каждому discovered module назначаются manifest_path и default cwd;
5. enabled modules становятся активными.

### Event hook

Когда происходит событие:

1. groov проверяет enabled;
2. проверяет фильтр events;
3. создаёт внешний процесс;
4. передаёт environment contract;
5. добавляет args после interpolation;
6. запускает процесс detached и ждёт его в отдельном waiter thread.

stdout и stderr перенаправлены в null, поэтому логирование нужно настроить в самом модуле: файл, системный logger или собственное перенаправление.

Если rpc равно true, stdin/stdout используются для JSONL RPC и не являются обычным логом. Для обычных env-модулей rpc оставляйте false.

### Command alias

Для команды из commands groov:

1. ищет module по alias name;
2. проверяет enabled;
3. создаёт процесс из command;
4. соединяет commands[].args и аргументы, введённые пользователем;
5. передаёт event command:<alias>;
6. запускает процесс detached.

Например:

~~~json
{
  "command": "python3",
  "args": ["main.py"],
  "commands": [
    { "name": "lyrics", "args": ["lyrics"] }
  ]
}
~~~

Команда :lyrics song запустит:

~~~text
python3 main.py lyrics song
~~~

Команда отображает статус запуска, но не получает stdout модуля обратно в TUI. Если дополнение должно сообщить результат пользователю, оно может записать его в файл/лог, изменить библиотеку через CLI или отправить уведомление внешним способом.

### Key binding

keys связывает имя клавиши с command line:

~~~json
{
  "keys": {
    "v": "visualizer",
    "ctrl-v": "visualizer toggle",
    "alt-l": "next"
  }
}
~~~

Поддерживаемые имена (с учётом регистра для символов):

- один символ: v, H, 1; H и h — разные bindings;
- специальные: enter, escape, tab, up, down, left, right;
- модификаторы: ctrl- и alt-, например ctrl-v.

Если значение указывает на module alias, запускается модуль. Если alias не найден, groov пробует выполнить это как встроенную команду, например next, play или search.

Ядро имеет приоритет для quit, play, stop и search bindings. Модульные bindings обрабатываются перед обычными навигационными клавишами, поэтому модуль может осознанно занять свободную навигационную клавишу.

## Events

Текущие events:

| Event | Когда отправляется | Song context |
|---|---|---|
| play | начат или перезапущен трек | есть |
| stop | пользователь остановил воспроизведение | обычно есть |
| rescan | библиотека пересканирована | нет |
| metadata | изменены metadata песни | есть |

events: [] означает получать все текущие и будущие события, которые groov отправляет через этот API. Чтобы уменьшить coupling, production-модуль лучше перечисляет нужные события явно.

## Environment contract

Каждый запуск получает:

~~~text
GROOV_API_VERSION=1
GROOV_EVENT=play
GROOV_EVENT_JSON={...}
GROOV_SONG_ID=13
GROOV_SONG_FILE=/path/to/song.mp3
GROOV_CONFIG_DIR=/path/to/config/groov
GROOV_SETTINGS_FILE=/path/to/config/groov/settings.json
GROOV_DATA_DIR=/path/to/data/groov
GROOV_SONGS_DIR=/path/to/data/groov/songs
GROOV_CACHE_DIR=/path/to/cache/groov
GROOV_PLAYER_IPC=/path/to/cache/groov/player.sock
~~~

GROOV_SONG_ID и GROOV_SONG_FILE не устанавливаются, если song context отсутствует. GROOV_EVENT_JSON всегда содержит как минимум event.

`GROOV_SONGS_DIR` предназначен для модулей, которые создают или обновляют записи библиотеки. Модуль должен писать временный каталог и переименовывать его в numeric song directory только после полной подготовки файлов.

Для playback event:

~~~json
{
  "event": "play",
  "id": "13",
  "file": "/music/songs/13/song.mp3",
  "title": "Song",
  "artist": "Artist"
}
~~~

Для alias:

~~~text
GROOV_EVENT=command:lyrics
~~~

## Interpolation

В args и env заменяются:

| Placeholder | Значение |
|---|---|
| {event} | event name |
| {id} | song ID |
| {file} | путь к audio file |

Interpolation выполняется до передачи аргументов в Command; аргументы не объединяются в shell string. Это позволяет передавать путь с пробелами как один аргумент.

Если song context отсутствует, {event} заменяется, а {id}/{file} остаются без изменений.

## JSONL RPC protocol

RPC включается явно:

~~~json
{
  "name": "dashboard",
  "version": "1.0.0",
  "enabled": true,
  "rpc": true,
  "command": "python3",
  "args": ["dashboard.py"],
  "commands": [
    { "name": "dashboard", "args": [] }
  ],
  "events": ["play", "metadata"]
}
~~~

На каждый event или command groov запускает модуль, отправляет одну JSON request line и читает JSON response lines до завершения процесса. Request имеет форму:

~~~json
{
  "api_version": 1,
  "type": "event",
  "event": "play",
  "args": [],
  "song": {
    "id": "13",
    "file": "/music/song.mp3",
    "title": "Song",
    "artist": "Artist",
    "album": "Album",
    "description": "Note",
    "tags": ["rock"],
    "duration": 120
  }
}
~~~

Для command alias type равен command, а event имеет вид command:NAME. song может быть null.

Каждая непустая response line — JSON object. Поддерживаемые view types:

| type | Поля | Результат в TUI |
|---|---|---|
| status | text | строка состояния |
| error | text | ошибка в строке состояния |
| overlay | text или body, title | полноэкранная текстовая панель |
| paragraph | text, title | текстовый widget |
| list | lines, title | список строк |
| gauge | value, max, text, title | progress gauge |
| view | root или widget | declarative widget tree |
| palette | border, clear | временно изменить цвет границ или очистить изменение |
| canvas | id, z, rect, commands, modal, opaque | retained cell-based drawing layer |
| canvas_patch | id, updates, remove | изменить только named canvas commands |
| animation | id, fps, duration_ms, loop, frames | host-driven animation без frame-by-frame RPC |
| playback | action, song_id, offset_seconds, audio_filter | управлять текущим player process |
| close | нет | закрыть активный module view |
| rescan | нет | перечитать библиотеку без остановки player |
| redraw | нет | запросить новый render pass |

Пример RPC-модуля:

~~~python
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    song = request.get("song") or {}
    if request.get("type") == "event":
        print(json.dumps({
            "type": "status",
            "text": "playing: " + song.get("title", "unknown")
        }), flush=True)
    else:
        print(json.dumps({
            "type": "paragraph",
            "title": "Module response",
            "text": json.dumps(request, indent=2)
        }), flush=True)
~~~

`palette` сейчас принимает цвет `border` в шестизначном hex или базовом имени; `clear: true` возвращает цвет из активной темы. Неизвестные response types не ломают groov: они превращаются в диагностическое сообщение, а unknown fields сохраняются в parsed data. Это оставляет пространство для новых declarative widgets без поломки старого host.

При session: true процесс запускается один раз, принимает последовательность JSON requests и может отправлять streaming response lines. Request type input используется для передачи key events активному view. Session-модуль должен закрыть stdin и завершиться при остановке host.

В открытом module view host сохраняет общую модель ввода: `:` переключает TUI в COMMAND mode, а `:q`/`:close` закрывает view. `Esc` остаётся коротким отменяющим жестом. Модуль получает остальные клавиши через обычный `input` request и не обязан резервировать собственные варианты выхода.

### Управление playback

Модуль может вернуть `{"type":"playback", ...}`. Это общий контракт host, поэтому модулю не нужен доступ к PID player process и не нужно запускать второй проигрыватель. Все действия проверяются относительно текущего трека; поле `song_id` можно добавить как защиту от устаревшего ответа.

Поддерживаемые `action`:

| action | Поля | Поведение |
|---|---|---|
| `restart` | `song_id`, `offset_seconds`, `audio_filter` | перезапустить текущий трек; по умолчанию сохранить текущую позицию и активный фильтр |
| `set_filter` | `audio_filter` | применить фильтр с текущей позиции |
| `clear_filter` или `reset` | — | убрать фильтр и перезапустить текущий трек |
| `seek` | `offset_seconds`, `audio_filter` | перезапустить с указанной позиции |
| `pause` | — | поставить текущий backend на паузу |
| `resume` | — | продолжить текущий backend |
| `stop` | — | остановить воспроизведение |

`audio_filter` — строка фильтра для настроенного аудиобэкенда, максимум 4096 байт; host передаёт её как один shell-quoted аргумент. Для стандартного `ffplay` groov добавляет `-af` автоматически. Значения с переводами строк и NUL отклоняются. Модуль должен сам выбрать синтаксис, который поддерживает backend; например, speed/pitch-модуль может собрать цепочку FFmpeg `atempo`/`asetrate`, а эквалайзер — свою цепочку.

Для настоящего live-контроля backend может использовать `{player_ipc}` в `player_command`. Groov передаёт тот же путь модулю как `GROOV_PLAYER_IPC`; модуль сам говорит с endpoint в протоколе выбранного backend. Это позволяет сохранить общий API и не перезапускать процесс при каждом изменении параметра.

Пример независимого от конкретного модуля ответа:

~~~json
{
  "type": "playback",
  "action": "restart",
  "song_id": "13",
  "audio_filter": "atempo=0.82,volume=1.1"
}
~~~

Если модуль использует собственную команду backend вместо стандартного `ffplay`, добавьте `{audio_filter}` отдельным аргументом в `player_command` в место, где должны появляться параметры фильтра. Placeholder раскрывается в `-af 'FILTER'` при активном фильтре и в пустую строку без фильтра. Без этого placeholder фильтр можно использовать только с командой, содержащей `ffplay`.

Widget tree поддерживает paragraph, list, gauge, row, column, stack и вложенные children с flex/direction. Это совместимый declarative слой поверх Ratatui.

### Canvas и слои

RPC-модуль может вернуть `type: "canvas"`. Такой ответ сохраняется как слой с ключом `<module>::<id>` и заменяет предыдущую версию этого слоя. Слои рисуются после основного интерфейса в порядке `z`; поэтому модуль может сделать небольшой HUD, status bar, floating panel или полностью заменить содержимое экрана. `modal: true` направляет обычные клавиши в этот модуль, а `modal: false` оставляет библиотеку интерактивной под overlay. `mode: "replace"` включает оба свойства и очищает область слоя перед рисованием.

Геометрия `rect` и координаты команд задаются в cell или процентами от доступной области. Область без `rect` занимает весь терминал. Поддерживаемые команды:

| op | Поля | Результат |
|---|---|---|
| `text` | `x`, `y`, `text`, `width`/`max_width` | текст с clipping |
| `fill` | `x`, `y`, `width`, `height`, `symbol` | заливка области |
| `clear` | `x`, `y`, `width`, `height` | очистка области до host buffer |
| `cell` | `x`, `y`, `symbol` | один символ |
| `hline` / `vline` | координата, `width`/`height`, `symbol` | линия |
| `line` | `x1`, `y1`, `x2`, `y2`, `symbol` | произвольная линия |
| `rect` | `x`, `y`, `width`, `height` | рамка |

Каждая команда принимает `style` или короткие поля `fg`, `bg`, `modifiers`; цвета используют hex и базовые имена из тем. Пример немодального overlay:

~~~json
{
  "type": "canvas",
  "id": "now-playing-hud",
  "z": 40,
  "modal": false,
  "rect": {"x": "2%", "y": "2%", "width": "96%", "height": 5},
  "commands": [
    {"op": "fill", "x": 0, "y": 0, "width": "100%", "height": "100%", "bg": "#202030", "symbol": " "},
    {"op": "rect", "x": 0, "y": 0, "width": "100%", "height": "100%", "fg": "#89b4fa"},
    {"op": "text", "x": 2, "y": 2, "text": "Now playing", "fg": "#a6e3a1", "bold": true}
  ]
}
~~~

Чтобы удалить слой, отправьте тот же `id` с `"clear": true`. Максимум — 4096 команд в одном canvas-ответе; координаты всегда clipping-ятся host до терминала, поэтому ошибочный модуль не может записать за пределы buffer.

### Полное владение терминалом

Если нужен настоящий самостоятельный Ratatui/crossterm интерфейс, добавьте в manifest `"terminal": true` и alias в `commands`:

~~~json
{
  "name": "my-app",
  "enabled": true,
  "terminal": true,
  "command": "python3",
  "args": ["app.py"],
  "commands": [{"name": "my-app", "args": []}]
}
~~~

При `:my-app` groov выключает raw mode и alternate screen, запускает процесс с inherited stdin/stdout/stderr и ждёт его завершения. Процесс получает стандартные `GROOV_*` переменные, включая `GROOV_TERMINAL=1` и `GROOV_PLAYER_IPC`, поэтому может использовать любой собственный terminal UI, мышь, alternate screen и библиотеку виджетов. После выхода groov безопасно восстанавливает свой экран и продолжает playback.

Canvas и terminal mode дополняют друг друга: canvas подходит для overlays, HUD и взаимодействия с библиотекой, terminal mode — для приложения, которому нужен полный контроль над терминалом.

### Host-driven animation

`animation` позволяет отправить таблицу кадров один раз. Host сам выбирает активный кадр по monotonic clock, перерисовывает только изменившиеся terminal cells через Ratatui diff buffer и не ждёт следующий JSONL response. Поэтому session-модулю не нужно запускать `sleep`-поток для простых эффектов:

~~~json
{
  "type": "animation",
  "id": "spinner",
  "z": 60,
  "fps": 60,
  "loop": true,
  "duration_ms": 400,
  "rect": {"x": "50%", "y": 2, "width": 1, "height": 1},
  "frames": [
    {"at_ms": 0, "commands": [{"op": "text", "id": "glyph", "x": 0, "y": 0, "text": "|"}]},
    {"at_ms": 100, "commands": [{"op": "text", "id": "glyph", "x": 0, "y": 0, "text": "/"}]},
    {"at_ms": 200, "commands": [{"op": "text", "id": "glyph", "x": 0, "y": 0, "text": "-"}]},
    {"at_ms": 300, "commands": [{"op": "text", "id": "glyph", "x": 0, "y": 0, "text": "\\"}]}
  ]
}
~~~

`fps` ограничен диапазоном 1..240, `frames` — 1024 кадрами, а commands каждого кадра — 4096 операциями. `loop: false` оставляет последний кадр после завершения. Отправка нового animation с тем же `id` заменяет старый; `clear: true` удаляет слой.

### Incremental canvas patches

Для динамических данных, которые нельзя описать заранее, сначала отправьте обычный `canvas`, а затем меняйте только именованные команды:

~~~json
{
  "type": "canvas_patch",
  "id": "hud",
  "updates": [
    {"id": "score", "op": "text", "x": 2, "y": 1, "text": "score: 42"}
  ],
  "remove": ["old-message"]
}
~~~

`updates` заменяет команду с тем же `id` или добавляет новую, `remove` удаляет команды. Host сохраняет слой между кадрами, поэтому модулю не нужно повторно отправлять фон, рамки и неизменившиеся элементы.

### Host frame ticks

Session-модуль может указать в manifest `"fps": 60`. Host будет отправлять ему request:

~~~json
{"api_version":1,"type":"tick","event":"tick","frame":42,"dt_ms":16}
~~~

Модуль может использовать tick для физики, сетевых данных и интерактивных эффектов, а затем отправлять `canvas_patch`. `animation` предпочтительнее для заранее известных кадров: в этом случае между host и модулем нет JSONL round-trip на каждый кадр. Tick requests учитывают `events`; если events перечислены, добавьте `"tick"` или оставьте массив пустым.

## Примеры реализации

### Shell

~~~sh
#!/bin/sh
set -eu

case "$GROOV_EVENT" in
  play)
    printf 'now playing: %s\n' "$GROOV_SONG_ID" >> "$HOME/.cache/groov-scrobble.log"
    ;;
  stop)
    printf 'stopped: %s\n' "$GROOV_SONG_ID" >> "$HOME/.cache/groov-scrobble.log"
    ;;
esac
~~~

Манифест:

~~~json
{
  "name": "shell-scrobbler",
  "version": "1.0.0",
  "enabled": true,
  "command": "sh",
  "args": ["main.sh"],
  "events": ["play", "stop"]
}
~~~

### Python

~~~python
#!/usr/bin/env python3
import json
import os
from pathlib import Path

event = os.environ.get("GROOV_EVENT", "")
payload = json.loads(os.environ.get("GROOV_EVENT_JSON", "{}"))
log = Path.home() / ".cache" / "groov-python-module.log"
log.parent.mkdir(parents=True, exist_ok=True)
with log.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps({"event": event, "payload": payload}) + "\n")
~~~

Манифест:

~~~json
{
  "name": "python-inspector",
  "version": "1.0.0",
  "enabled": true,
  "command": "python3",
  "args": ["main.py"],
  "events": ["play", "metadata"],
  "commands": [
    { "name": "inspect-song", "args": [] }
  ],
  "keys": { "i": "inspect-song" }
}
~~~

### Rust

Rust-модуль — обычный отдельный Cargo project. Он не обязан зависеть от groov:

~~~sh
cargo new --bin my-groov-module
cd my-groov-module
cargo build --release
cp target/release/my-groov-module ~/.local/bin/
~~~

Манифест может запускать my-groov-module и передавать ему тот же environment contract.

## Управление модулями

В TUI:

~~~text
:modules
:modules list
:modules on NAME
:modules off NAME
:modules info NAME
:reload
~~~

В CLI:

~~~sh
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
~~~

`install` и `add` копируют каталог с `module.json` в `$XDG_CONFIG_HOME/groov/modules/<name>`; `--force` заменяет существующую копию. `remove` удаляет установленную копию и запись из settings. `enable`/`disable` и `on`/`off` сохраняют activation в settings.json, поэтому состояние переживает перезапуск. `:reload` заново читает manifests и settings, не останавливая текущий player process.

Info panel/command показывает:

- module name;
- version, description, author, homepage;
- enabled state;
- executable и args;
- source manifest path;
- working directory;
- events;
- aliases and alias args;
- key bindings;
- env;
- unknown custom keys.

## Тестирование модуля

Минимальная ручная проверка:

~~~sh
groov modules info NAME
groov modules off NAME
groov modules on NAME
groov
~~~

Для event hook создайте тестовую переменную в env:

~~~json
{
  "env": {
    "GROOV_MODULE_LOG": "/tmp/groov-module.log"
  }
}
~~~

Затем запустите трек и проверьте файл. Для command alias проверьте :alias; для key binding — клавишу в NORMAL mode.

В репозитории reference module проверяется тестом манифеста, а shell entrypoint можно запустить отдельно:

~~~sh
GROOV_HELLO_MODULE=1 \
GROOV_EVENT=play \
GROOV_SONG_ID=42 \
GROOV_MODULE_LOG=/tmp/hello.log \
sh examples/modules/hello/hello.sh
cat /tmp/hello.log
~~~

## Ошибки и отказоустойчивость

- invalid manifest пропускается во время discovery;
- отсутствующий executable приводит к статусу запуска, но не завершает groov;
- event module запускается detached;
- stdout/stderr модуля не попадают в экран;
- падение модуля не ломает player process;
- повторный :reload позволяет исправить manifest без рестарта;
- обычный detached module command получает закрытый stdin и не должен блокировать ожиданием ввода;
- `terminal: true` — явное исключение: такой alias получает inherited stdin/stdout/stderr и может работать интерактивно до выхода.

## Совместимость и версия API

GROOV_API_VERSION=1 — версия environment contract. Модуль должен игнорировать неизвестные переменные и неизвестные JSON-поля. Не полагайтесь на порядок ключей JSON.

При добавлении нового event:

1. не меняйте смысл существующего event;
2. добавьте его в документацию;
3. оставьте events: [] совместимым;
4. добавьте тест;
5. обновите reference module, если event нужен для примера.

При несовместимом изменении contract увеличьте API version и опишите миграцию.

## Границы и совместимость

Процессная граница сохраняется для безопасности и языковой независимости. Внутри обычного host UI модуль рисует через canvas protocol, который не зависит от Rust ABI и позволяет реализовать собственные виджеты на уровне terminal cells. Когда нужен прямой доступ к Ratatui, crossterm, mouse events или alternate screen, используйте `terminal: true`: в этом режиме host временно освобождает терминал и не ограничивает UI модуля своим layout или buffer.

Неизвестные canvas-команды игнорируются, неизвестные JSON-поля сохраняются, а старые `status`, `overlay`, `view` и `playback` ответы работают без изменений. Долгоживущие sessions и streaming notifications остаются совместимыми с `session: true`.
