# Разработка

Правила работы с Git, issue, PR и TDD находятся в [AGENTS.md](../AGENTS.md).
Контракт библиотеки описан в [architecture.md](architecture.md).

## Локальные проверки Rust

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --doc
```

Cargo запрещает `unsafe_code` для всех собственных целей пакета, включая тесты,
и требует rustdoc-комментарии для публичного API. Для просмотра документации
вместе с внутренними функциями:

```sh
cargo doc --locked --no-deps --document-private-items --open
```

В CI документация также собирается с `RUSTDOCFLAGS="-D warnings"`. Тесты
проверяют библиотеку на временных папках и реальные запуски CLI: группы,
фильтрацию, JSON, ошибки и сохранность исходных файлов.

## Единый CI

Workflow `.github/workflows/ci.yml` запускается для PR в `master` и push в `master`.
Для каждого события создаётся один pipeline. Push в рабочую ветку запускает
проверки через событие PR; отдельного push-pipeline для такой ветки нет.
Устаревшие запуски одного PR отменяются.

Проверяются fmt, Clippy, тесты и rustdoc на Windows и Linux. Проверка `PR rebase`
использует настоящий HEAD ветки PR: он должен включать актуальный `master`,
а новые коммиты не должны быть merge-коммитами. Итоговый check `CI` обязателен
для слияния и становится успешным только после всех проверок.

Защита `master` также требует актуальную базовую ветку, линейную историю и PR,
действует для администратора и запрещает force push и удаление `master`.
В настройках репозитория разрешён только Rebase and merge.

Тестирование проверки истории локально (Bash или Git Bash):

```sh
bash scripts/test-pr-base.sh
```

Тесты создают отдельные Git-репозитории в `target/ci-history-tests` и проверяют
актуальную ветку, устаревшую базу, rebase, merge-коммиты и неизвестные ссылки.

## Выпуск по запросу

Теги версий и релизы создаются только по явной просьбе пользователя. Обычный
PR, push в `master` или создание тега не пересобирают установщик и не выпускают релиз.

Для запрошенной версии:

1. Через issue и PR обновить версию в `Cargo.toml` и `Cargo.lock`, а также описание
   релиза в `packaging/windows/release-notes.md`.
2. После успешного CI и слияния через rebase создать и отправить тег `v<версия>`
   на соответствующем коммите `master`.
3. Вручную запустить единый workflow с этим тегом, например:

   ```sh
   gh workflow run ci.yml --ref master -f release_tag=v0.1.1
   ```

Значение `v0.1.1` — пример; нужен уже существующий тег запрошенной версии.
Workflow проверяет исходники указанного тега на Windows и Linux, соответствие
версии Cargo и принадлежность тега истории `master`, затем собирает и тестирует
установщик и публикует `.exe` вместе с `.sha256` в GitHub Releases.
Релиз с уже существующим именем не перезаписывается.

Ручной запуск без `release_tag` выполняет только проверки:

```sh
gh workflow run ci.yml --ref master
```

## Локальная сборка и проверка установщика

Для сборки требуются Windows, стабильный Rust с целью `x86_64-pc-windows-msvc`,
MSVC Build Tools / Windows SDK и [Inno Setup 6 или 7](https://jrsoftware.org/isdl.php).
Из корня репозитория:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows-installer.ps1
```

Версия читается из `Cargo.toml`; готовый `.exe` и его `.sha256` появляются в `dist`.
Runtime C/C++ линкуется статически для устанавливаемой программы. Сборка использует
`cargo build --release --locked --target x86_64-pc-windows-msvc`.
Если компилятор Inno Setup установлен в нестандартную папку, передайте
`-CompilerPath "C:\путь\к\ISCC.exe"`. Опция `-BootstrapCompiler` при отсутствии
компилятора загрузит Inno Setup 6.7.3 из официального релиза, проверит закреплённую
контрольную сумму и установит его для текущего пользователя.
Параметр `-ExecutionPolicy Bypass` действует только для этого процесса PowerShell.

Проверка установщика (после сборки):

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-windows-installer.ps1 -CompilerPath "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
```

Тест собирает отдельный установщик с уникальным AppId, временной папкой
и изолированным ключом реестра вместо рабочего `PATH`. Проверяются установка,
повторная установка, работа CLI, сохранение типа и сторонних записей `PATH`,
отключение опции и удаление. Рабочий `PATH` и установленная пользовательская
копия программы остаются без изменений. Диагностические файлы находятся
в `target/installer-smoke`.
