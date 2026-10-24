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
- отправлять JSONL RPC-ответы, из которых groov рендерит status, overlays, paragraphs, lists и gauges;
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
~~~

GROOV_SONG_ID и GROOV_SONG_FILE не устанавливаются, если song context отсутствует. GROOV_EVENT_JSON всегда содержит как минимум event.

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

Неизвестные response types не ломают groov: они превращаются в диагностическое сообщение, а unknown fields сохраняются в parsed data. Это оставляет пространство для новых declarative widgets без поломки старого host.

Текущий RPC — request/response foundation с отдельным процессом на сообщение. Долгоживущие sessions, streaming notifications и произвольные custom Ratatui widgets требуют следующей версии protocol; v1 уже задаёт безопасную точку расширения для них.

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
groov modules on NAME
groov modules off NAME
groov modules info NAME
~~~

on/off сохраняют module config в settings.json. Поэтому activation переживает перезапуск. :reload заново читает manifests и settings, не останавливая текущий player process.

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
- module command не должен блокировать ожиданием stdin: stdin закрыт.

## Совместимость и версия API

GROOV_API_VERSION=1 — версия environment contract. Модуль должен игнорировать неизвестные переменные и неизвестные JSON-поля. Не полагайтесь на порядок ключей JSON.

При добавлении нового event:

1. не меняйте смысл существующего event;
2. добавьте его в документацию;
3. оставьте events: [] совместимым;
4. добавьте тест;
5. обновите reference module, если event нужен для примера.

При несовместимом изменении contract увеличьте API version и опишите миграцию.

## Ограничения текущего API

Модульный API расширяет команды, key bindings, process hooks и интеграции, а JSONL RPC уже позволяет возвращать декларативные status, overlay, paragraph, list и gauge views. Произвольный Rust Ratatui widget напрямую не загружается, а RPC-сессия сейчас request/response и запускает отдельный process на сообщение. Долгоживущие sessions, streaming notifications и custom widget schema можно добавить совместимо в следующей версии protocol; v1 оставляет для этого явные type/data границы.
