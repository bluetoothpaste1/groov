# Документация groov

Этот каталог — основной справочник проекта. Если короткого README недостаточно, ищите подробности здесь.

## С чего начать

- [Пользовательское руководство](USER_GUIDE.md) — установка, первый запуск, библиотека, команды TUI, настройки, темы и диагностика.
- [Разработка модулей](MODULES.md) — полный контракт дополнений: манифест, события, команды, key bindings, окружение, примеры и отладка.
- [Справочник конфигурации](REFERENCE.md) — точные поля `settings.json`, переменные окружения, форматы данных и CLI-команды.
- [Руководство разработчика](DEVELOPMENT.md) — устройство исходников, сборка, тесты, изменение API и обязательные правила обновления документации.
- [Архитектурные решения](ARCHITECTURE.md) — долговечные контракты хранения, расширений, рендера и выбора языка.
- [`basic_modules`](../basic_modules/) — готовые out-of-tree примеры `flame`, `easyadd` и `speedr`.
- [Changelog](../CHANGELOG.md) — история релизов и release notes.
- [License](../LICENSE) — лицензия MIT.

## Быстрые ссылки

```text
groov init                         создать конфигурацию и каталоги
groov                              открыть TUI
groov modules                      список модулей
groov modules info NAME            подробности модуля
groov modules install DIRECTORY    установить модуль
groov modules remove NAME          удалить модуль
groov theme list                   список тем
groov doctor                       диагностика
groov uninstall                    удалить все данные после подтверждения
make docs-check                    проверить ссылки и code fences
make release                       собрать release binary
make install PREFIX=~/.local       установить binary и man page
./install.sh                        собрать и установить release binary
```

Для первого расширения используйте [тестовый модуль hello](../examples/modules/hello/README.md).

Для declarative UI/RPC используйте [rpc-dashboard](../examples/modules/rpc_dashboard/README.md).

## Статус документации

Документация описывает текущий код репозитория. Источником истины для поведения являются исходники и тесты; при расхождении их нужно исправить вместе с этой документацией. Правило синхронизации описано в [DEVELOPMENT.md](DEVELOPMENT.md).
