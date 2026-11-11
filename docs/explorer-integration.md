# Explorer: открыть папку в Katран

Этот необязательный локальный скрипт добавляет команду **«Открыть в Katран»**
для выделенной папки и для фона открытой папки. Для работы нужен собранный
`runtime\Katran.exe` с поддержкой `--open-folder`; скрипт не запускает его и не
проверяет наличие готовой сборки. Команда использует обычный профиль приложения.

До установки можно посмотреть план. В PowerShell из каталога Katran:

```powershell
.\scripts\register-explorer.ps1 -Mode Plan -NilesoftRoot 'C:\path\to\portable-shell'
.\scripts\register-explorer.ps1 -Mode Install -NilesoftRoot 'C:\path\to\portable-shell'
```

`-Launcher` при необходимости указывает абсолютный путь к другому `Katran.exe`.
Изменение расположения исполняемого файла требует повторного `Install`. Путь
launcher с кавычками, апострофом, `@` или переводом строки отклоняется, поскольку
он вставляется в конфигурацию Nilesoft как строковый литерал.

Для Nilesoft скрипт создаёт помеченный `imports\katran.nss` и добавляет один
помеченный импорт в `shell.nss`. Остальные байты конфигурации сохраняются,
включая UTF-8 BOM и переводы строк. Перед каждым изменением `shell.nss` создаётся
backup с суффиксом `.katran-backup-<UTC>`. Повторная установка идемпотентна;
существующий чужой файл или непомеченный импорт с тем же именем не заменяется.
При необходимости отмены:

```powershell
.\scripts\register-explorer.ps1 -Mode Uninstall -NilesoftRoot 'C:\path\to\portable-shell'
```

Удаляются только помеченные импорт и файл; backup сохраняется. После изменения
конфигурации может потребоваться её перезагрузка в Nilesoft. Скрипт не
перезапускает Explorer. Синтаксис импортов и пунктов описан в документации
[Nilesoft configuration](https://www.nilesoft.org/docs/configuration) и
[properties](https://nilesoft.org/docs/configuration/properties).

Без `-NilesoftRoot` используются два статических глагола в
`HKCU\Software\Classes\Directory\shell\Katran.OpenFolder` и
`HKCU\Software\Classes\Directory\Background\shell\Katran.OpenFolder`:

```powershell
.\scripts\register-explorer.ps1 -Mode Plan
.\scripts\register-explorer.ps1 -Mode Install
.\scripts\register-explorer.ps1 -Mode Uninstall
```

Администратор не нужен. Чужие ключи с теми же именами не перезаписываются;
удаляются только ключи с маркером этой регистрации. Windows 11 может показать
статические глаголы в классическом расширенном меню; появление в первом
современном меню не гарантируется.

Синтетическая проверка без реестра, запуска приложения и управления Explorer:

```powershell
.\scripts\test-register-explorer.ps1
```

## Передача в приложение

`Katran.exe --open-folder <absolute-directory>` отделяет целевую папку от
профиля данных. Launcher выбирает обычный portable/installed профиль и передаёт
путь без запуска командной оболочки. Новый процесс публикует небольшой запрос
в `launch-requests` профиля до проверки single-instance, затем будит существующее
окно штатным событием активации. Первое окно читает запрос при запуске; уже
работающее — при активации, когда dispatcher свободен.

Потребитель вызывает существующий `ChooseFolder`, поэтому действуют те же
проверки и история, что в Ctrl+Shift+K. Исходная папка не импортируется и не
становится отдельным профилем. Папки-ссылки разрешены; каталог очереди и её
файлы не могут быть reparse points. Очередь ограничена32слотами и64КиБ на
запрос. Прерванная запись может оставить занятый слот: автоматического удаления
оставшихся lock/tmp/claim файлов нет. Ошибки отдельных запросов показываются
в статусе и не отменяют корректные запросы. После обновления бинарника нужен
один обычный полный выход через трей; предыдущая версия не знает этот протокол.

## Follow-up: retained requests and shelf filters

The initial launch receiver only drained on an explicit folder argument or a
new activation event. Requests sent before an older process was replaced could
remain queued after an ordinary restart. The primary now drains at every startup.
Explorer and Ctrl+Shift+K both use `open_working_folder`: ChooseFolder validates
the path and records history, then the client clears the previous shelf search
and refreshes the live Folder listing. The folder-launch diagnostic records only
received/opened/failed counts. A synthetic regression verifies visible files,
cleared search, history and unchanged original data.

Actions (Ctrl+K) now includes **Перезапустить Katran**. Restart waits for active
operations, starts the current installed build from the version pointer with the
same profile, and requests a real exit through the existing tray-aware shutdown.
The successor waits for the old process to exit before acquiring its instance
lock. Restart-only arguments are removed before ordinary launch parsing, and
one-shot Explorer folder arguments are not replayed. No process is killed.
