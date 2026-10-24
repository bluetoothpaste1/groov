# Руководство разработчика groov

Этот документ описывает, как менять groov безопасно и как держать исходники, тесты и документацию синхронизированными.

## Принципы проекта

1. Библиотека пользователя не должна зависеть от версии интерфейса.
2. Metadata и settings должны оставаться human-editable.
3. Ошибка внешнего player или module не должна разрушать TUI.
4. Терминальный вывод должен работать через portable ANSI path.
5. Новая возможность должна иметь понятный командный/API-контракт.
6. Документация обновляется в том же изменении, что и код.

## Карта исходников

| Файл | Ответственность |
|---|---|
| src/main.rs | CLI dispatch, init, doctor, import, edit, list, theme и module registry |
| src/config.rs | Settings, ModuleConfig, ThemeConfig, JSON loading/writing, discovery |
| src/paths.rs | XDG path resolution и создание каталогов |
| src/library.rs | SongInfo, Song, scan, atomic metadata writes |
| src/player.rs | external audio process, placeholders, pause/stop |
| src/modules.rs | event hooks, command aliases, environment contract |
| src/tui.rs | app state, input dispatch, command parser, rendering, image worker |
| src/main.c | legacy VK CLI |
| src/vk.c | legacy VK transport/parser |
| include/groov/vk.h | legacy VK public C API |
| tests/test_vk.c | legacy C tests |
| examples/modules/hello | reference extension |
| packaging/groov.1 | installed manual page |
| scripts/check_docs.py | dependency-free documentation check |
| .github/workflows/ci.yml | Linux/macOS Rust and Linux legacy CI |
| .github/workflows/release.yml | tagged Linux/macOS binary artifacts |

## Архитектура приложения

### Startup

main разрешает AppPaths, затем:

1. читает или создаёт settings.json;
2. обнаруживает module manifests;
3. обнаруживает theme files;
4. сканирует songs;
5. запускает TUI.

CLI-команды list, scan, add и edit не запускают TUI. Команда tui передаёт Settings и список Song в App.

### TUI state

App разделяет:

- domain state: songs, visible, selected, tags, query;
- playback state: player, playing, queue, position, pause, repeat;
- input state: mode, input, prefix, help/module overlay;
- render state: image protocol, generation, pending worker request.

Режимов ровно два: Normal и Command. Если новая функция может быть реализована командой или overlay, не добавляйте новый mode без необходимости.

### Render loop

event loop:

1. принимает image results;
2. рисует только dirty frame;
3. читает crossterm Event;
4. обрабатывает key/resize;
5. проверяет завершение player process;
6. запускает следующий или повторный трек;
7. обновляет timeline.

Ratatui использует buffered diff renderer. Worker изображений получает generation number, поэтому устаревшая обложка не может перезаписать новую selection.

## Как менять код

### Новая CLI-команда

1. Добавьте branch в main::run.
2. Напишите parser аргументов с понятными ошибками.
3. Добавьте строку в print_help.
4. Добавьте раздел в USER_GUIDE.md и REFERENCE.md.
5. Если команда меняет settings или library, документируйте atomicity и side effects.
6. Добавьте unit/integration test.

### Новая TUI-команда

1. Добавьте command name в command_names.
2. Обработайте команду в App::execute_command.
3. Добавьте completion, если есть subcommands или names.
4. Обновите help overlay.
5. Обновите USER_GUIDE.md, MODULES.md или REFERENCE.md.
6. Напишите тест command behavior.

### Новое поле settings

1. Добавьте typed field в Settings.
2. Добавьте serde default.
3. Обновите Settings::default.
4. Проверьте старый settings.json без нового поля.
5. Добавьте поле в пример REFERENCE.md и ARCHITECTURE.md.
6. Добавьте пользовательскую команду, если поле меняется интерактивно.
7. Документируйте persistence и допустимые значения.

### Новое поле module manifest

1. Добавьте typed field в ModuleConfig с default, если поле optional.
2. Решите, нужно ли оно в runtime info.
3. Обновите merge semantics в discovery.
4. Обновите MODULES.md schema table и JSON example.
5. Обновите reference module, если поле полезно новичку.
6. Добавьте parse/discovery test.

### Новое событие module API

1. Выберите стабильное lowercase имя.
2. Определите точный момент отправки и song context.
3. Добавьте emit call.
4. Документируйте GROOV_EVENT и GROOV_EVENT_JSON.
5. Проверьте disabled modules и events filters.
6. Добавьте тест.
7. Обновите API version policy только если меняется существующий контракт.

## Сборка

Основная цель:

~~~sh
cargo build
~~~

Через Makefile:

~~~sh
make
~~~

Debug executable: target/debug/groov.

Legacy:

~~~sh
make legacy
~~~

Legacy target требует C compiler и libcurl. Rust target от него не зависит.

Release binary and local installation:

~~~sh
make release
make install PREFIX="$HOME/.local"
make uninstall PREFIX="$HOME/.local"
~~~

The install target copies the release binary and groov.1 manual page. It does not modify configuration or user data. Package maintainers can use target/release/groov and packaging/groov.1 as source artifacts.

## Проверки перед изменением

Минимальный набор:

~~~sh
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
make test
make docs-check
~~~

Если изменён TUI или terminal layer, дополнительно запустите smoke в PTY:

~~~sh
TERM=xterm-256color printf 'q' | script -q /tmp/groov-smoke target/debug/groov tui
TERM=linux printf 'q' | script -q /tmp/groov-smoke-linux target/debug/groov tui
TERM=screen-256color printf 'q' | script -q /tmp/groov-smoke-screen target/debug/groov tui
TERM=dumb printf 'q' | script -q /tmp/groov-smoke-dumb target/debug/groov tui
~~~

Если изменён module API, проверьте:

~~~sh
cargo test
groov modules
groov modules info hello
groov modules off hello
groov modules on hello
~~~

GitHub Actions repeats the Rust checks on Ubuntu and macOS and builds/tests the legacy C target on Ubuntu. A pull request is not ready until these jobs are green.

## Тестирование

### Unit tests

Unit tests находятся рядом с Rust-модулем в cfg(test). Подходят для:

- parser;
- normalization;
- path resolution;
- queue rules;
- manifest parsing;
- theme parsing;
- command state transitions.

### Filesystem tests

Используйте tempfile. Не читайте настоящий HOME и не изменяйте реальную user library. Создавайте AppPaths вручную под temporary directory.

### Module tests

Минимум должен покрывать:

- manifest parses;
- required metadata available;
- disabled state respected;
- module discovery finds manifest path;
- on/off writes settings;
- info includes source path;
- command alias/key action remains wired.

### TUI smoke

TUI требует PTY. Piping directly to stdin может дать ошибку raw mode, поэтому используйте script или другой pseudo-terminal runner.

## Atomicity и ошибки

Metadata writes используют temporary JSON и rename. Не заменяйте этот подход обычным truncate/write без причины.

Import использует staging directory и rename. При ошибке staging удаляется.

External process failures должны стать user-visible status или диагностикой, но не panic. Не используйте unwrap на пользовательском JSON, filesystem или process output.

## Совместимость

Сохраняйте:

- capitalized song metadata keys;
- relative media paths внутри song directory;
- XDG resolution;
- defaults для новых settings/module fields;
- GROOV_API_VERSION semantics;
- безопасный ANSI image fallback;
- CLI names, если нет явно объявленной breaking change.

Не удаляйте неизвестные settings/module fields. Flattened extra map нужен для forward compatibility.

## Документация как часть изменения

Это обязательное правило проекта:

> Любое изменение пользовательского поведения, формата данных, CLI, TUI-команды, settings-поля, module event, environment variable, placeholder или terminal behavior должно обновить документацию в том же patch.

Перед завершением изменения пройдите checklist:

- [ ] README содержит краткий user-facing пример.
- [ ] USER_GUIDE описывает использование и ограничения.
- [ ] REFERENCE описывает точное поле/команду/значение.
- [ ] MODULES обновлён, если затронут extension API.
- [ ] ARCHITECTURE обновлён, если затронут durable contract.
- [ ] пример или fixture обновлён, если он помогает понять API.
- [ ] тест проверяет новую behavior.
- [ ] help output соответствует документации.

Если изменение только внутреннее и не меняет контракт, это нужно явно отметить в commit/PR description.

## Релизный checklist

1. Запустить форматирование, clippy и все тесты.
2. Запустить make docs-check.
3. Проверить CLI help.
4. Проверить migration старого settings.json без новых полей.
5. Проверить module discovery на чистом XDG профиле.
6. Проверить TUI в нескольких TERM values.
7. Обновить docs и examples.
8. Проверить CREDITS и dependency changes.
9. Убедиться, что target/build/cache не попали в patch.
10. Убедиться, что Cargo.toml version, CHANGELOG.md и packaging/groov.1 согласованы.

Tagged releases use the `vX.Y.Z` convention. The release workflow packages Linux and macOS archives with the runner architecture in the filename. Each archive contains the binary, license, README, changelog, and man page.
