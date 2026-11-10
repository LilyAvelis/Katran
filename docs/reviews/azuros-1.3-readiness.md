# Azuros Claudapi · архитектурная готовность Katran к 1.3

Дата: 2026-11-10. Объект проверки — **текущее рабочее дерево 1.2.6**, включая изменённые и новые незакоммиченные файлы после принятой 1.0.7. Это не аудит одного `HEAD` и не аудит отдельного QuickLook-Latency-Lab. Путь проекта: `C:/Users/lilya/Lilia Avelis/Katran`; номера строк ниже относятся к проверенному дереву. Продуктовый код, настройки запуска и пользовательские данные не изменялись. Единственный добавленный файл проекта — этот отчёт.

Mein Gott, палитра снова требует инспекции. Но архитектурная проблема шире рамки клавиши: сегодня несколько важных гарантий существуют как знания автора адаптера, а не как ограничения системы. Маленький агент может добросовестно вызвать разрешённый API и получить неправильную классификацию или неожиданно сильное право удаления. Зелёные тесты при этом совершенно настоящие — просто часть из них доказывает более слабое утверждение, чем требовалось продукту.

## Вердикт

**Основа пригодна для дальнейшего развития, но 1.3 пока нельзя объявлять архитектурой для свободного добавления источников, больших каталогов и заменяемого UI.** Для принятого локального сценария 1.2.6 уже есть значительная полезная страховка. Для заявленного следующего масштаба нужны шесть ограниченных исправлений границ: SQL-проекция каталога; общий жизненный цикл источников; полное, продолжаемое сопоставление; явное владение и доверие к подготовленным результатам; отделение длительного I/O от интерактивного диспетчера; проверяемый контракт визуальных компонентов.

Подтверждённых P0 — безусловной текущей катастрофы или потери произвольных личных файлов — **не обнаружено**. Ниже P1 означает препятствие целям 1.3, а не утверждение, что принятая владелицей 1.2.6 целиком неработоспособна. Не следует начинать с переписывания приложения или механического разрезания больших файлов.

| Приоритет | Наблюдение | Практическое следствие |
|---|---|---|
| P1 · F01 | `BrowseShelf` перечитывает всю историю до ограничения страницы | Страница 1 000 занимает около 2,44 с уже при 100 000 синтетических событий в debug; вызов выполняется и на UI-потоке |
| P1 · F02 | Весь `Service` передаётся одному заданию; часть тяжёлых операций вызывается синхронно | Медленный файл задерживает ввод/Clipboard; два зависших Shell provider останавливают все новые visuals |
| P1 · F03 | Жизненный цикл источников находится в GUI; CLI использует иной ingest/receipt-путь | Замена UI требует повторить продуктовую оркестрацию; отказ источника не имеет общего восстановления/статуса |
| P1 · F04 | Лимит 128 в matcher не сопровождается курсором | Старые изображения могут никогда не пройти классификацию, сколько ни повторять обработку |
| P1 · F05 | Строка `source` и сериализованные worker-результаты принимаются как полномочия/доказательства | Ошибка адаптера может разрешить удаление оригинала, записать ложную revision или скрыть несвязанную картинку |
| P1 · F06 | Палитра измеряет один прямоугольник, рисует другой; тест проверяет только clipping | Реальная обрезка рамок остаётся при зелёной headless-проверке; исправление предшествующей регрессии не гарантирует новое состояние |
| P2 · F07–F11 | Миграции, каталог действий, диагностика, установка, нативные границы | Пока контролируются соглашениями и ручным знанием; требуют отдельных контрактов до расширения |

## Что уже сделано правильно

- Один `Service::execute` и типизированный `Command` — хорошая центральная граница. GUI не пишет SQLite и не реализует собственное удаление файлов. Исчерпывающий Rust `match` полезнее автоматической регистрации плагинов.
- SQLite WAL, транзакционное сохранение blob metadata после durable записи байтов, монотонные ID, защита legacy references при очистке и явные отсутствующие объекты защищают реальные данные. Эти решения и их тесты следует сохранить: `src/storage.rs:33`, `:137`; `src/delete_shelf.rs:18`.
- `DeleteEvents` действительно остаётся shelf-only. Отдельный `DeleteSelection` учитывает исключение владелицы для Screenshots и recycling Downloads; проверяет containment, тип файла и записанный hash. Группы подтверждаются по сохранённым ID, ошибки оригиналов возвращаются поэлементно: `src/delete_selection.rs:6`, `:29`, `:68`; `src/downloads.rs:325`.
- Copy/drag готовят независимые snapshots, проверяют содержимое, не используют hard links к owned blobs, сохраняют предыдущую paste-партию до успешной публикации. Двухфазный drag и одноразовый token — удачный образец небольшой полезной границы: `src/copy_out.rs:138`, `:248`, `:314`; `src/drag_session.rs:23`, `:100`.
- Clipboard listener принят владелицей; ограниченные retries и отсутствие тихого перехода на polling закреплены кодом и тестами. Оставлять текущий event backend по умолчанию: `src/capture_events.rs:8`, `src/bin/clipboard_events_ui.rs:94`.
- Реальные ограничения описаны честно: prepared hashes доверяют локальному worker; Shell provider может зависнуть; cache hit не означает отсутствие чтения originals. Это полезные документы, но документированное ограничение всё равно остаётся ограничением для новой цели.
- Bounded очереди и texture cache, fake QuickLook launcher, fake Shell provider, fake recycler и synthetic directories — правильное направление. Katran не зависит от Afio во время выполнения; повторно использовать его геометрию/визуальный язык допустимо.

## Что проверено фактически

Выполнены без запуска native приложения и без чтения/замены содержимого пользовательского Clipboard:

```powershell
cargo test --offline --features ui --target-dir target/build
cargo clippy --offline --all-targets --features ui --target-dir target/build -- -D warnings
cargo fmt --check
```

Результат: **139 тестов passed**, в том числе 38 тестов UI binary; один явный latency-тест ignored; doc-tests — 0. Clippy и fmt прошли. Полный вывод тестов был прочитан. Штатные тесты могут запросить Clipboard sequence number; это не чтение содержимого. Были запущены только headless test binaries, CLI `serve-stdio` и scratch probes, не GUI Katran и не QuickLook.

Дополнительно выполнены четыре синтетических эксперимента вне дерева проекта: фактическое удаление временного файла через `DeleteSelection`; принятие ложного prepared hash; ложная пара классификации и недостижимая 129-я пара; измерение реальных shapes палитры с production style. Изменение `user_version` проверялось на временной SQLite. Личные Screenshots/Downloads и Recycle Bin не использовались.

Скриншот владелицы `C:/Users/lilya/AppData/Local/Temp/codex-clipboard-bf1495ca-fda1-4ddb-a22d-d017828d6bca.png` просмотрен как файл. Это свидетельство её наблюдения на Windows, **не моя повторная ручная проверка установленного окна**. Я не приписываю картинке недоказанное пересечение label/keycap regions: отдельно доказана обрезка рамок и разные вертикальные якоря.

### F01 · P1. Пагинация API не ограничивает работу над каталогом

**Доказательство.** `src/shelf_tabs.rs:21`–`:31` запрашивает все страницы истории с пустым query. Каждый `Event` вызывает `path.is_file()` в `src/storage.rs:259`. Затем создаётся дополнительная копия `identity_events`, выполняются lookup классификации, проверки root, для ShareX — дополнительный `canonicalize`. Фильтр tab/query/after и `.take(limit)` приходят только в конце: `src/shelf_tabs.rs:60`, `:103`. `known_clipboard_ids` дополнительно делает SQL lookup для каждого clipboard event: `src/screenshot_identity.rs:99`.

`ScreenshotMatchInputs` аналогично собирает всю историю, а не инкрементальную очередь: `src/folder_commit.rs:134`. UI выполняет `refresh`/`load_more` синхронно через `call`: `src/bin/api_client.rs:136`, `:161`. При обновлении уже загруженных страниц каждая заново проходит весь каталог. В render дополнительно клонируется весь загруженный `Vec<Value>` и повторно отправляется полный `SetVisible`: `src/bin/ui.rs:414`.

**Локальное измерение.** Windows x86_64, debug CLI из `target/build/debug`, реальный stdio request/response; три повтора, существующий однобайтовый synthetic file, много событий одного cached blob. SQL использован только для быстрого создания большой тестовой истории. Измерение включает serialization и transport, исключает создание fixture и открытие сервиса. Это тёплая локальная файловая система; на машине параллельно велась другая ограниченная работа. Нативные Shell handlers не вызывались.

| История | `BrowseShelf`, page 1 000 | `SearchPage`, пустой query, page 1 000 | `SearchPage`, отсутствующий query |
|---:|---:|---:|---:|
| 20 000 | 490,30 / 483,51 / 524,81 мс | 37,62 / 35,90 / 35,53 мс | 42,42 / 42,03 / 41,70 мс |
| 100 000 | 2 441,14 / 2 440,27 / 2 426,26 мс | 37,21 / 35,30 / 36,66 мс | 207,26 / 207,00 / 207,61 мс |

**1 000 000 не измерялся.** Линейная экстраполяция данного debug-сценария даёт порядок десятков секунд для Browse, а не обещание конкретной цифры. Release и другой диск изменят коэффициент, но не `O(history)` на каждую страницу. Миллион также увеличивает временные `Event`/JSON-копии; количественная оценка peak RSS не проводилась. Разные пути уже демонстрируют, что ограничить работу до SQL `LIMIT` возможно.

`SearchPage` лучше для пустого запроса: ID keyset применяется в SQL. Но substring search вызывает Rust lowercase/contains и JSON extraction на кандидатах (`src/storage.rs:233`); отсутствие совпадений сканирует историю. Индекс `events_blob_hash` (`:72`) не решает membership/path/search. Нельзя обещать индексируемый произвольный substring обычным B-tree.

**Минимальная граница.** Разделить arrival history и актуальную source projection. В SQLite хранить source configuration/generation, canonical path key, latest arrival, availability и классификацию. Применять tab/filter/cursor в одном query до `LIMIT`; filesystem reconciliation обновляет projection вне browse. Переиспользовать одну реализацию search semantics для `SearchPage` и `BrowseShelf`, которые сейчас фильтруют по-разному в SQL и Rust. Не потерять скрытую selection и адресуемость старых ID.

**Приёмка.** Для 20k/100k/1m synthetic arrivals измерять page cost, количество materialized rows и filesystem calls. Обычная страница не должна stat/canonicalize unrelated history. Проверить mixed sources, одинаковые paths, переименования, missing/restored original, query + tab + page boundary, смену root между страницами. Для search отдельно установить продуктовый контракт substring/FTS: токеновый FTS не является незаметной заменой substring.

### F02 · P1. Ограничена длина очереди, но не время владения интерактивным сервисом

**Доказательство.** `start_job` забирает весь `Service` через `self.api.take()` и перемещает в отдельный thread (`src/bin/api_client.rs:34`, `:40`). До возврата результата новых domain-команд нет; GUI отключается. Это сохраняет одного writer, но не даёт приоритет Clipboard во время уже начавшегося длинного задания. Очередь copy и отдельные очереди tab/density смягчают только некоторые вводы.

Другие тяжёлые пути вообще выполняются в render: drop → `IngestPath` → полный hash (`src/bin/ui.rs:273`, `src/lib.rs:437`, `:502`); подтверждённое удаление (`src/bin/ui.rs:510`); `Import` (`:540`); `RunAction` open/QuickLook (`src/bin/api_client.rs:255`), где `quicklook::preview` читает весь reference перед запуском (`src/quicklook.rs:28`). Большой архив или медленное сетевое Downloads могут заморозить UI до native CLI transport. Измерения отдельной QuickLook-Latency-Lab этого пути не доказывают и сюда не переносятся.

В visuals содержимое читается повторно. Для тёплого reference-image `PreviewFile` проверяет hash в `file_visual::validate_source` и снова в `preview::render`; для тёплого non-image file visual — также финальная проверка после cache hit: `src/file_visual.rs:71`, `:74`, `:102`, `:103`; `src/preview.rs:58`. Это два/три полных прохода независимо от маленького thumbnail. Гарантию freshness нельзя просто удалить ради красивого latency.

Два visual workers (`src/bin/images.rs:95`) делят image decoding и Shell extraction. `IShellItemImageFactory::GetImage` синхронный (`src/file_visual.rs:343`), deadline нет. Если два provider вызова не возвращаются, bounded queue 64 не создаёт третий работоспособный worker. Pending cancellation не отменяет in-flight call. Это подтверждённое отсутствие механизма, **не утверждение, что установленный handler сейчас зависает**.

**Минимальная граница.** Оставить single SQLite writer, но дать ему короткие команды/commit. Подготовку bytes/hash/export делать вне владения interactive service; возвращать подготовленный проверяемый результат и явный job status/cancellation. Один bounded scheduler с очередями user transfer / capture / discovery и гарантированным продвижением каждой активной группы лучше множества специальных `pending_*`. Недостаточно перенести синхронный метод в ещё один UI thread, который снова уносит весь Service.

Для thumbnails сначала разделить decode и Shell lanes, учесть in-flight IDs и timeout state. Если требуется жёсткое восстановление после стороннего COM hang, оправдан **маленький перезапускаемый helper process только для Shell extraction**: Rust thread безопасно принудительно остановить нельзя. Это локальная нативная граница, не повод строить микросервисы.

**Приёмка.** Fake blocked hash/export/provider, несколько источников, активный ввод, capture, отмена, закрытие и смена конфигурации. Новое быстрое действие получает явный accepted/queued/busy и не пропадает в `Value::Null`. Длительный cosmetic provider не забирает все slots у image preview. Собрать I/O counters на cold/warm reference sizes и не объявлять cache hit нулевым чтением. Решение об ослабленной metadata freshness для косметики должно быть явным; deletion/copy сохраняют строгую проверку содержимого.

**Новая согласованная цель — быстрый QuickLook с совместным запуском.** Текущее production поведение — найти executable и `spawn` при запросе; `requested:true` означает создание процесса, не readiness/render (`src/quicklook.rs:46`, `:129`). Эксперимент внешней лаборатории не считается уже интегрированным кодом. Для следующего этапа нужен небольшой domain runtime owner: `Stopped/Starting/Ready/Failed`, idempotent single-flight `EnsureReady`, обнаружение уже работающего backend, asynchronous warmup при старте Katran и ограниченная очередь первых preview requests. Несколько одновременных запросов должны запускать один backend; успешный spawn не равен готовности. Транспорт/handshake нужно выбрать по проверенной совместимости; прежний retired pipe probe не возвращать ради проверки readiness. Отсутствие backend/таймаут/краш должны быть видны через API, а не задерживать SQLite writer. Отдельно определить shutdown ownership: нельзя считать чужой уже работавший QuickLook disposable child Katran. Приёмка — fake clock/process/transport: existing instance, concurrent ensure, cold startup, readiness timeout, crash/retry, queued request и отсутствие дубликатов; затем отдельно проверенный runtime transport и ручная Windows-проверка владелицы. Это рекомендация будущего контракта по новой задаче, не результат выполнения интеграции в данном аудите.

### F03 · P1. Источники пока не являются независимым доменным подсервисом

**Доказательство.** Configuration fingerprint, запуск watcher, drain/fairness, pending queue, dirty matching, запуск classification и refresh находятся в `src/bin/source_jobs.rs:11`, `:24`, `:44`, `:129`, `:162`. Именно UI определяет, когда продукт обнаружит и классифицирует файлы. CLI `ScanSharex`/`ScanDownloads` реализуют другую оркестрацию в `src/shelf_tabs.rs:113` и `src/downloads.rs:115`: другие receipt tables (`sharex_seen`/`downloads_seen` против `folder_commits`), отдельные availability tables, несколько повторных hash reads и SQL savepoints. Формально всё проходит через dispatcher; практически правило источника всё ещё требуется заново воспроизвести новому клиенту.

Отказ запуска также не получает общего recovery. Setup делается только при изменении строки configuration (`source_jobs.rs:44`). Если absolute root временно отсутствует при старте, ошибка показывается, но при появлении папки settings fingerprint тот же и setup не повторится. Ошибка native watcher приводит к `due = None` (`src/folder_watch.rs:166`–`:178`), отсутствует supervisor restart. `try_recv().ok()` не различает пустой канал и завершившегося producer (`:120`). F5 делает shelf refresh, а не восстановление источника. Это вывод по прямому control flow, отдельный цикл удаления/возвращения реального диска не запускался.

Есть и явный масштабный потолок: оба scanner считают **все** directory entries и возвращают ошибку после 20 000 (`src/sharex.rs:22`, `:60`; `src/downloads.rs:11`, `:47`). Состояние не продвигается, частичный результат не публикуется. Это честно документированный all-or-error предел: папка с 20 001 записью может не импортировать ни одного нового файла. Native notification всё равно вызывает полный recursive scan; частые изменения большого дерева умножают metadata I/O.

Drop watcher не блокирует GUI, hash проверяет stop между 64 KiB reads — это положительные решения. Но старые setup/matching threads после reconfiguration не имеют общего generation/cancel contract; создание двух configured workers не означает глобальную границу retired work при серии быстрых изменений. Один blocked filesystem read остаётся непредсказуемым; это нужно показывать как retiring/failed state.

**Минимальная граница.** Обычный `sources`/`SourceSupervisor` в библиотеке, запускаемый headless host и UI одинаково. Он владеет configuration generations, health, native watcher lifecycle, bounded discovery cursor, receipts, retries/restart и очередью reconciliation. UI подписывается на typed status/changes и отправляет команды. `Scan*` становятся explicit reconciliation clients того же coordinator, а не вторым алгоритмом приёма. Не менять сам Clipboard listener на polling.

**Приёмка.** Synthetic missing-root-at-start → root appears → восстановление по документированному действию; watcher error/overflow; новый файл на фоне initial history; 20 001 и 100k entries; разные source roots; быстрая смена A→B→A; старые candidates не коммитятся в новую generation; закрытие не ждёт blocked I/O и количество retired tasks ограничено. Сравнить CLI и GUI receipts после restart, deletion и возврата старых bytes.

### F04 · P1. `take(128)` не является законченной пакетной обработкой

**Доказательство.** `prepare_matches` выбирает первые 128 ShareX и первые 128 clipboard событий (`src/screenshot_identity.rs:26`, `:38`). `ScreenshotMatchInputs` отдаёт их из descending history. UI не хранит cursor/coverage и не планирует следующую пачку; тот же сериализованный snapshot вообще пропускается (`src/bin/source_jobs.rs:162`–`:177`). На неизменившейся истории старые записи недостижимы постоянно; добавление новых отодвигает их дальше.

**Выполненный контрпример:** 128 red ShareX events, затем 129-й blue ShareX event, затем blue clipboard event; реальные synthetic PNG с правильными content hashes. `prepare_matches` вернул 0 pairs, повторный вызов — опять 0. Вход из 130 событий не является чрезмерной нагрузкой. Указание комментария «caller schedules further batches» не реализовано вызывающим кодом.

Первые тесты `tests/screenshot_identity.rs:2`, `:44` проверяют также старый полноисторический `matched_clipboard_ids`, который production UI уже не вызывает; новый bounded путь проверен только маленькой fixture. Так можно получить зелёный тест полноты не того execution path.

**Минимальная граница.** Persisted очередь непроверенных revisions или cursor по двум сторонам, coverage state и гарантированный drain до idle. Индекс digest → source identity позволяет добавлять один source/clipboard revision без повторного сравнения последних 128. Ограничивать одновременно bytes, concurrent decode и размер шага; число items не ограничивает суммарную работу с 128 файлами по 256 MiB. Нынешний RAM cache 1 024 digest entries ограничен (`src/screenshot_identity.rs:201`), но не заменяет очередь полноты. Legacy DB pixel cache/match tables требуют lifecycle/retention; там нет общей очистки удалённых event IDs.

**Приёмка.** 129/257 на каждой стороне, совпадение только в последнем элементе; continuation до idle, restart, root switch, source deletion, изменение bytes, новое событие во время backlog. Измерять processed/unprocessed/failed, чтобы «нет совпадения» отличалось от «никогда не рассматривали». Сохранять утверждённую политику: исторические известные ShareX clipboard copies могут оставаться скрытыми даже после исчезновения original; текущие тесты явно этого требуют. Это не новая найденная ошибка.

### F05 · P1. Происхождение, владение и доказательство worker нельзя объединять строкой

**Доказательство A — destructive authority.** Generic `IngestPath` принимает любую непустую source string до 256 bytes (`src/lib.rs:437`, `:519`). `DeleteSelection` выбирает original deletion по `event.source == "sharex" | "downloads"` (`src/delete_selection.rs:16`, `:40`, `:46`). Выполненный stdio probe:

1. Создана отдельная временная ShareX directory, выставлена в synthetic settings.
2. В ней записан `not-scanned.txt` с synthetic bytes; scanner не вызывался.
3. `IngestPath {source:"sharex"}` успешно создал reference.
4. `DeleteSelection {ids:[id],confirmed:false}` вернул `deleted:1`; временный original действительно исчез.

Это **не обход configured containment** и не удаление произвольного файла вне root. API выполняет явно переданную строку. Риск для расширения в другом: адаптер, которому нужен label «ShareX», получает право permanent deletion даже для неотобранного scanner типа. Domain не отличает обычную external reference от проверенного live-source member. Новому источнику/агенту приходится помнить скрытую силу имени.

**Доказательство B — prepared hash.** `CommitFolderCandidates` — публичная serde command (`src/api.rs:35`). `src/folder_commit.rs:42` проверяет root/containment, формат hash и size/mtime, но не связывает hash с результатом конкретного worker. Probe с правильными metadata и 64 нулями вместо hash успешно создал `revision:"ref:000…"`. Это уже прямо описано в `docs/latency.md` как доверие локальному worker. Freshness checks при preview/export/delete обычно остановят действие, а не удалят чужие bytes; следствие — ложная identity, отказ операций и неверная deduplication, а не доказанная произвольная потеря файлов.

**Доказательство C — classification.** `CommitScreenshotMatches` принимает caller-supplied пары и проверяет только existence, revisions и source names (`src/screenshot_identity.rs:54`). Ни pixel evidence, ни принадлежность source текущей configuration внутри commit не проверяются. Scratch API probe передал корректные IDs/revisions разных red/blue PNG: `matched:1`, Images visible count 1→0. Ошибка worker может тихо скрыть несвязанную картинку без нарушения формата API.

**Минимальная граница.** Разделить `SourceId`/origin label и `StorageRef`/ownership/delete policy. Для live reference хранить source configuration ID + generation + canonical path identity + recorded revision + разрешённую политику удаления; generic ingest создаёт shelf-only external reference. Проверенный source commit создаёт live membership. Не превращать каждый caption в новый enum variant, но destructive policy должна быть типом, а не совпадением имени.

Prepared commits должны принимать opaque receipt от зарегистрированного доменного worker с привязанными path/revision/configuration, а wire API — только безопасные input commands. Если prepared wire transport действительно нужен локальным клиентам, явно отделить trusted/internal command surface и проверяемую регистрацию producer; простое переименование поля в `trusted_hash` ничего не защищает. Classification commit должен получать результат собственного worker/проверяемый digest input, а не произвольное утверждение «эти два ID равны».

**Приёмка.** Generic reference с любым display source остаётся shelf-only; source commit имеет ровно оговорённую delete policy; forged/mismatched/expired receipts и root generations отвергаются; red≠blue нельзя скрыть публичной парой без проверки evidence. Проверить migration старых references, не выдавая destructive ownership всем legacy strings автоматически. Это защита от ошибочного адаптера и ошибочного агента; локальный CLI всё ещё работает с правами того же Windows user и не объявляется защищённым IPC.

### F06 · P1. Компонент палитры не владеет всей своей геометрией, а тест доказывает clipping

**Фактический дефект.** `paint_row_contents` вручную оценивает ширину keycaps как text width + 8 и gaps (`src/bin/palette.rs:179`). Рисует их другой helper, создающий `Frame` с margins и stroke (`src/bin/ui_style.rs:43`–`:64`). Реальный frame получается шире зарезервированного региона, поэтому правильное пересечение clipping отрезает его левую сторону. Это конкретная ошибка измерения компонента, не доказательство ошибки business API.

Scratch probe использовал текущие production `paint_row_contents`, `ui_style`, Windows fonts и SVG loader, без окна. При row `[8,8]–[388,46]`:

| Элемент | Зарезервированный clip X | Фактически нарисованный frame X | Обрезано слева |
|---|---|---|---:|
| `Ctrl` в `Ctrl Minus` | 308,5…379,0 | 304,8…333,9 | 3,7 pt |
| `Ctrl` в `Ctrl Plus` | 318,4…379,0 | 314,8…343,9 | 3,6 pt |
| `Tab` | 351,7…379,0 | 350,0…379,0 | 1,7 pt |

`Ctrl Minus` frame height = 32 pt и помещается в keys-area height = 32 pt. **Гипотеза «interact_size 32 раздувает высоту больше fixed row 38» этим опытом не подтвердилась.** Production style меняет соседние spacing, но сам этот дефект воспроизводится и с default style. Icon, label и keycaps получают разные top offsets/layout alignment (`src/bin/palette.rs:196`, `:201`, `:207`); общая вертикальная ось не задана. На скриншоте владелицы различие якорей видно, однако числовое пересечение label/keycap regions не установлено — они как раз disjoint.

**Почему тесты зелёные.** `scrolled_rows_keep_text_and_keycaps_inside_viewport` (`src/bin/palette.rs:250`) проверяет, что `clip_rect` лежит внутри viewport, offscreen текст невидим и какой-то label/keycap видим. Обрезанная рамка полностью удовлетворяет всем этим условиям. `long_shortcut_aliases_fit_a_narrow_row` проверяет только расчёт двух regions, а не содержимое. Default `Context` и синтетические короткие подписи дополнительно не воспроизводят всю production composition. Умножить logical coordinates на 1,5 — не самостоятельная проверка фактически получившихся glyph/frame bounds при scale.

**Минимальная граница.** Один `ActionRow`/`KeyCaps` владеет measurement **и** paint, получает readonly action presentation, возвращает clicked ID. Он измеряет реальную рамку/galley, задаёт общую ось выравнивания, сохраняет ancestor clip и содержит явную policy для слишком длинного binding: alias/ellipsis/tooltip. Fixed row height допустим при измеренной высоте контента; сам по себе `38.0` не виновник и не лекарство. Общая foundation полезна только если palette реально использует её контракт, а не три ad hoc child scopes.

**Приёмка.** Production fonts/style/SVG loader, реальные каталог и user overrides, 180/240/420 pt, 1×/1,5×/2×, scroll с partially/offscreen rows. Проверять все nonempty painted bounds (с учётом stroke) в назначенном region, отсутствие label↔keys overlap, полную рамку обычного shortcut, согласованные vertical centers/baselines и отсутствие paint за viewport. Отдельная snapshot/render artifact для ручного просмотра владелицей. Headless проверяет заявленные геометрические инварианты; окончательный Windows appearance проверяет владелица. Разделение файла на модули не заменяет ни одного из этих assertions.

### F07 · P2. Миграции и владение служебными таблицами ещё не имеют общей версии

`Service::open` безусловно пишет `PRAGMA user_version=1` (`src/lib.rs:83`), затем проверяет columns/schema текстом. На synthetic DB version 77 после открытия стала 1. Это не обнаруженная порча текущей базы 1.2.6, но более новый schema version нельзя превращать в известный перед проверкой совместимости. При schema rebuild копируется фиксированный список columns (`src/storage.rs:68`), поэтому безопасный будущий downgrade нельзя предполагать.

Новые `folder_commits`, `*_seen`, `*_availability`, screenshot cache/matches создаются лениво в разных action modules без FK к events и общего lifecycle. Исторические receipts иногда намеренно должны переживать удаление event, чтобы не воскресить shelf item; значит, просто добавить `ON DELETE CASCADE` ко всему тоже неправильно. Требуется различать receipt, cache и projection, назначить retention каждому.

Settings получили backward-compatible defaults — это хорошо. Corrupt JSON не сбрасывается молча. Общие preferences сохраняются одним SQLite statement. Shortcuts используют staging + sync перед rename (`src/shortcuts.rs:264`); явного краш-теста Windows replace и общей версии документа нет.

**Рекомендация/приёмка.** Один schema owner, ordered transactions `vN→vN+1`, отказ при неизвестной future version, migration fixtures принятой 1.0.7 и текущей 1.2.6, crash/reopen/idempotence. На каждой служебной таблице документировать owner, key, retained-after-delete policy, cleanup budget. Legacy hashes backfill в `storage::initialize` читает references при writer transaction (`src/storage.rs:85`); большую backfill-миграцию делать продолжимой, не пряча её в интерактивный startup.

### F08 · P2. Единый каталог есть, но расширение действий/типов всё ещё требует совпадений вручную

`actions::DEFS` уже объединяет labels, groups, bindings и palette visibility — прежняя рекомендация частично реализована. Но это positional tuple (`src/actions.rs:6`), отдельный string match `resolve` (`:98`), отдельные GUI special cases `copy`/`cleanup`/preview (`src/bin/api_client.rs:255`) и фильтры IDs в input routing (`src/bin/ui.rs:174`). Тест `tests/catalogue.rs:3` проверяет `resolve(...).is_some()`, не исполняет semantic prerequisites/result. Наличие icon и route не доказывает правильный объект или действие.

Пример предметного дрейфа: «Убрать с полки» в catalogue (`src/actions.rs:35`) теперь может немедленно удалить original. Для групп есть правильное предупреждение, для одиночного действия исходный label уже не описывает side effect. Это не повод отменить принятое immediate deletion; нужно, чтобы domain action presentation знал selection policy.

Новый image format сегодня требует согласовать extension sets как минимум в `src/sharex.rs:97`, `src/shelf_tabs.rs:224`, `src/quicklook.rs:9`, реальные image crate codecs и transfer pixel path. Поиск и tab classification также повторяются. Даже аккуратное добавление одного расширения может направить файл в native image preview без поддержки decoder или оставить его вне Images.

**Рекомендация/приёмка.** Named `ActionDef` + typed `ActionId`/route, явные prerequisites, side-effect class и result/intent variants. Небольшой capability/type policy определяет поддерживаемый decoder, preview backend и filters; GUI выбирает presentation, не смысл. Намерение «выбрать destination» может оставаться клиентским: собственно `Import` уже domain API. Не требовать от palette знать список бизнес-ID. Новый action должен пройти одинаковые request/result checks из CLI, shortcut и palette с fake native boundary.

### F09 · P2. Логи ограничены по диску, но не являются состоянием выполнения

`Logger` bounded/nonblocking и вращает четыре файла по 2 MiB — это хорошо (`src/diagnostics.rs:15`, `:16`, `:90`). Однако `Service::execute` логирует каждый UI read/query; большие values ограничиваются **после** построения и сериализации (`src/lib.rs:166`, `src/diagnostics.rs:92`). Повторный `SetVisible` и clones всей истории стоят CPU даже когда запись превращается в truncated summary. Периодический source tick просит repaint каждые 40 ms и читает Settings (`src/bin/source_jobs.rs:25`, `:37`). Лог от полезной ошибки может быстро вытесниться штатными frames; фактическая скорость rotation здесь не измерялась.

`FolderEvent::Error` превращается в изменяемую строку `self.message`, а не persistent source health. При panic/disconnected job GUI уже не владеет `Service` и предлагает restart (`src/bin/api_client.rs:64`); native call без return вообще не даёт такого сообщения. В `call` отсутствие api для большинства commands возвращает `Value::Null` (`:17`), смешивая busy/unavailable/error/no-result. Смешанный DeleteSelection прячет shelf cleanup result внутрь item (`src/delete_selection.rs:53`), а UI читает `cleanup_pending` только сверху (`src/bin/ui.rs:69`): warning о cleanup внутри mixed batch не агрегируется.

**Рекомендация/приёмка.** Typed `JobId/SourceStatus`, queue depth/in-flight age/error code/last successful commit; sampling/aggregation для frequent reads, summaries до построения больших JSON. Сохранять explicit per-item failures/warnings. Не нужен telemetry backend: локального structured status и bounded logs достаточно. Fake queue full, panic/disconnect, cancellation и nested cleanup warning должны давать различимые результаты, а не пустой UI result.

### F10 · P2. Immutable executable — хорошее начало; указатель и доказательство сборки ещё не атомарны

`scripts/build-working.ps1:8` именует installed build hash-ом binary и не перезаписывает уже работающий UI executable. Но manifest hashes считываются **после** build (`:15`–`:17`): правка source во время compilation может записаться как будто этот source точно породил binary. Manifest повторно переписывается даже для существующей hash directory. `current-version.txt` пишется прямым `Set-Content` (`:18`), затем обновляются stable launcher/registration (`:20`–`:23`); сбой после обрезания pointer или после переключения оставляет частичную установку. Launcher проверяет формат pointer и existence binary, не hash manifest (`src/bin/launcher.rs:10`). Эти failure windows проанализированы по коду; install script **не запускался**.

**Рекомендация/приёмка.** Зафиксировать source fingerprint перед сборкой и проверить неизменность после; записывать manifest один раз вместе с verified copied artifact; подготовить launcher/registration до финального atomic pointer replace в той же directory, сохранить previous pointer для rollback. На synthetic runtime проверить interruption на каждой стадии и запуск старого/нового complete build; не использовать рабочий pointer владелицы.

Документы также нуждаются в одном current contract: внизу README всё ещё сказано «no native folder event watchers» и fitted-only/no original zoom, хотя выше описаны watchers и zoom; `docs/tabs-and-sharex.md` начинает с отменённого polling/Ctrl J/H/неудаления originals и лишь позже оговаривает часть обновлений. Это прямой источник ошибок следующего агента, а не стилистическая претензия. Историю вынести/пометить так, чтобы текущий маршрут можно было прочитать один раз без reconciliation нескольких эпох.

### F11 · P2. Проверка original revision не делает delete атомарным с внешним writer

Проверки containment и hash действительно существуют. Однако ShareX делает `content_hash` и затем `remove_file` по pathname (`src/delete_selection.rs:132`, `:138`); Downloads аналогично проверяет, а потом отдаёт pathname recycler (`src/downloads.rs:379`, `:387`). Между этими действиями другой writer может заменить файл. Это **окно гонки по коду**, не воспроизведённая потеря личного файла и не основание заявлять P0. Canonicalization проверяет состояние на момент проверки; она не закрепляет identity на всё действие.

Для permanent deletion перспективная граница — Windows file identity/handle, проверка containment и revision этого открытого объекта и удаление именно его с допустимым share mode. Для recycling нужна отдельно документированная гарантия нативного boundary: path-based Shell operation, per-item result и состояние после abort. Простого наличия `FOF_ALLOWUNDO`/`FOFX_RECYCLEONDELETE` в коде недостаточно для моего утверждения, что любая внешняя Shell ситуация проверена; реальное recycling в этом аудите не запускалось.

Важный тестовый пробел: trait `Trash` есть, но dispatcher `DeleteSelection` жёстко создаёт `NativeTrash` (`src/delete_selection.rs:44`). Поэтому `tests/downloads.rs` проверяет fake нижней функции, а не полный mixed batch через dispatcher. Добавить dependency injection в Service, как уже сделано для drag/QuickLook, и проверить mixed cached/reference/ShareX/Downloads, failure/abort и изменения revision на fake boundary. Не тестировать эти случаи на personal files. После native успеха и перед SQLite commit остаётся crash window: повтор должен честно обработать уже missing original и завершить shelf deletion, как текущий код во многом уже умеет.

## Минимальная архитектура следующего шага

Не нужны plugin SDK, отдельная crate на action, async runtime ради моды или распределённая шина. Достаточно нескольких обычных Rust границ с одним writer:

```text
GUI / CLI / capture adapters / future Afio client
                     │ typed commands + typed results/status
                Service dispatcher
                  /          \
       short catalogue    validated commit/delete/export policy
          queries                 │
                  SQLite + owned blobs
                     ▲
    SourceSupervisor / bounded preparation + classification queues
                     │
       OS watcher / hash / decode / isolated Shell helper
```

`Event` сейчас одновременно arrival, resource identity, live source member, preview target и deletion authority. Разделять следует **эти понятия и их инварианты**, а не просто строки файлов:

| Понятие | Минимальный контракт |
|---|---|
| Arrival | Immutable ID, recorded revision, origin и detection time; старый ID остаётся адресуемым |
| Storage reference | Owned blob либо обычная external reference либо authorised live source member; delete policy не выводится из label |
| Source configuration | Stable ID + generation + root + enabled + health; late result несёт generation |
| Catalogue projection | Current membership/latest path revision/availability/classification, обновляемые coordinator, SQL page query без filesystem traversal |
| Prepared work | Producer-owned opaque receipt, bounded bytes/work, явное completed/failed/cancelled; не произвольное утверждение клиента |
| Interaction | Selection/cursor/query scope/view через API; rendering geometry принадлежит view, не меняет владение/действия |
| Presentation component | Measurement + paint + clipping + input mapping с проверяемыми bounds, возвращает intent/ID |

Для query cursor определить high-water/query fingerprint и поведение при новых arrivals и root changes. Нельзя silently продолжать страницу другой source generation. Для typed results достаточно небольших structs/enums внутри Rust с сохранением текущего wire shape или явной совместимой эволюцией API; не нужно начинать с новой версии протокола для каждого helper.

## Порядок работ и критерии завершения

1. **Зафиксировать контракты, затем чинить компоненты.** Записать storage/delete policy, trusted prepared surface, source status, query semantics и geometry assertions. Добавить именно воспроизведённые контрпримеры F04/F05/F06. Ограничить patch scope; не вносить одновременный общий refactor. Ручной screenshot check остаётся отдельным evidence.
2. **Укрепить опасные границы.** Ownership/membership и receipts; fake recycler на уровне dispatcher; mixed-batch outcomes и stale generations. Приёмка — generic ingest не приобретает destructive rights, неверная классификация не коммитится как факт.
3. **Сделать каталог действительно постраничным.** Migration source projection + SQL filters + инкрементальные availability/classification updates. Приёмка — fixed page work на большой synthetic истории, сохранение старых ID и accepted hidden-selection behavior; query semantics не меняются незаметно.
4. **Вынести source orchestration из UI в один coordinator.** CLI/GUI используют одинаковые receipts/recovery; matcher drain до idle. Приёмка — 129/257, missing/recovered root, overflow, reconfigure и backlog fairness; bounded retired work.
5. **Ограничить latency, а не только memory.** Убрать synchronous long I/O из render, typed job status, cancellation и lane separation; Shell helper только при требовании hard recovery. Измерять write→commit→query→texture/presentation отдельно. Green headless не означает measured screen latency.
6. **Завершить agent guardrails и release discipline.** Named action/type policies, ordered migrations, atomic runtime pointer, current docs. Повторить объединённые fmt/test/clippy после интеграции. Release build и ручные Windows checks выполняются отдельным согласованным этапом; этот аудит не переключает runtime и не публикует на GitHub.

## Рецепты расширения после укрепления границ

| Расширение | Что меняется | Что не должно меняться | Обязательная приёмка |
|---|---|---|---|
| Новый тип файла | Capability/type policy: распознавание, decoder/backend, фильтр. При необходимости native preview adapter и synthetic fixtures | Ingest/copy/drag/delete ownership, SQLite path в widget, отдельный список extensions в каждом view | Unsupported bytes, extension/content disagreement, missing/stale revision, fallback; одинаковая классификация CLI/GUI и отсутствие изменений original |
| Новый источник | Source adapter + config/default/migration + coordinator registration; согласованный receipt/membership/delete policy | Private scanner внутри UI; прямой INSERT из adapter; `source` label как право удаления; второй dispatcher | Initial/restart dedupe, changes/revert, temporary file/read failure, >batch-size backlog, overflow, generation/cancel, missing/restored root, per-item failure |
| Новое действие | Domain command/result/error, ActionDef/route и prerequisite/side-effect metadata; UI icon или безопасный fallback | Business-ID special cases в palette/input loop, другая реализация для shortcut, файловые операции в component | API сначала; CLI/palette/shortcut вызывают одну semantics; exact selection/revision, partial results, fake native boundary, conflict/persistence |
| Новый view | View/interaction commands при необходимости; renderer из readonly presentation + geometry; навигация переводит геометрию в API параметры | Storage model, ownership/source membership, отдельная selection коллекция, скрытые destructive semantics | Hidden selection/tab/preview behavior, keyboard/range navigation API, resize/density/DPI/clipping, owner visual acceptance |
| Новый фильтр | Typed query predicate и SQL/projection support, semantics/cursor tests | Полный history fetch с локальным `.filter()` после LIMIT; иной поиск в GUI | Matches beyond first page, mixed predicates, Unicode/literal substring, no-match worst case, cursor+new arrivals/root switch |

До появления общего capability policy новый формат сейчас затронет несколько существующих файлов — это надо явно назначить интегратору, а не поручать агенту угадывать полный список. Точно так же новый source сегодня требует менять `FolderSource`, `ShelfTab`/tab order и UI fingerprint; отсутствие этих точек в задаче агенту будет ошибкой постановки границы, а не доказательством его некомпетентности.

## Владение файлами при работе агентов

| Владелец | Модули/граница | Что передаёт интегратору |
|---|---|---|
| Один интегратор API/schema/catalogue | `src/api.rs`, `src/lib.rs`, schema migrations в `storage`, `actions`, shared type/ownership/query contracts | Wire compatibility, migrations, единый route, объединённые acceptance checks |
| Агент источников | `folder_watch`, scanner adapters, coordinator после согласования его I/O; отдельные synthetic tests | Proposed command/config hooks, generation/receipt contract, recovery/status evidence |
| Агент классификации | `screenshot_identity` и собственные tests | Complete drain/cursor, digest/cache budget и source membership inputs; без shared schema edits одновременно |
| Агент catalogue/query | Query implementation/projection tests после schema agreement | Measured 20k/100k/1m cost и selection-compatible paging; не меняет renderer |
| Агент visuals/native boundary | `file_visual`, `preview`, `images` только при явном совместном ownership; fake provider tests | Bounded I/O/in-flight/timeout contract и native limits; не меняет delete policy |
| Агент UI components | `palette`, `ui_style`/foundation в одном согласованном участке, component tests | Production-style geometry/render artifact и emitted intents; не трогает dispatcher/storage |
| Интегратор установки/документов | build script, launcher, current README/development map | Synthetic interruption tests, source manifest integrity и current contract |

Один и тот же файл не назначать двум исполнителям. Shared `Command`/schema/action snippets интегратор переносит последовательно. В каждом поручении нужны входы, outputs, error semantics, разрешённые свои файлы и негативный acceptance case. Сначала synthetic API proof, потом GUI client; затем общая проверка и отдельное наблюдение владелицы. Нельзя объявлять «готово», если тест проверял helper, а в runtime используется другая ветка.

## Воспроизводимые probes и сохранённые материалы

Scratch artifacts созданы вне проекта и могут исчезнуть после очистки Temp:

- `C:/Users/lilya/AppData/Local/Temp/katran-audit-dwft6u1z/probe.rs` и `output.txt` — production palette measurement. Probe включает реальные `action_icons`, `ui_fonts`, `ui_foundation`, `ui_style`; `paint_row_contents` и geometry functions взяты из текущего `palette.rs`; `textures_delta.clear()` перед teardown. Сравнивает default/production style и prints actual frame/text bounds.
- `C:/Users/lilya/AppData/Local/Temp/katran-api-audit-0sl69idy/provenance.json` — полные replies temporary ingest/delete/forged candidate.
- Там же `identity_probe.rs`, `identity_output.txt` — реальные PNG, 129-й source и ложная red/blue pair.
- Там же `scale.json` — все девять samples на размер/операцию из таблицы F01. Fixture DB находится в подпапке `scale`; её paths ведут только на synthetic one-byte file.

Минимальный повтор destructive/provenance probe через CLI приведён ниже. **Он создаёт собственный Temp и удаляет только собственный synthetic original; не подставлять личный root.** Перед повтором собрать CLI в безопасный target: `cargo build --offline --bin katran --target-dir target/build`. Нативное приложение не запускается.

```python
import json, pathlib, subprocess, tempfile
repo = pathlib.Path(r"C:/Users/lilya/Lilia Avelis/Katran")
tmp = pathlib.Path(tempfile.mkdtemp(prefix="katran-review-"))
shots = tmp / "shots"
shots.mkdir()
p = subprocess.Popen(
    [str(repo / "target/build/debug/katran.exe"), "serve-stdio", str(tmp / "data")],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8")
def call(command):
    p.stdin.write(json.dumps({"api_version": 1, "request_id": "probe",
                              "command": command}) + "\n")
    p.stdin.flush()
    reply = json.loads(p.stdout.readline())
    assert reply["ok"], reply
    return reply["result"]
s = call({"type": "get_settings"})
s.update(sharex_folder=str(shots), downloads_enabled=False)
call({"type": "set_settings", "settings": s})
original = shots / "not-scanned.txt"
original.write_bytes(b"synthetic original")
e = call({"type": "ingest_path", "path": str(original), "source": "sharex"})
print(call({"type": "delete_selection", "ids": [e["id"]], "confirmed": False}))
assert not original.exists()                 # observed: source name grants policy
candidate = shots / "prepared.png"
candidate.write_bytes(b"synthetic prepared")
m = candidate.stat()
canonical = lambda path: "\\\\?\\" + str(path.resolve())
r = call({"type": "commit_folder_candidates", "items": [{
    "source": "sharex", "root": canonical(shots), "path": canonical(candidate),
    "size": m.st_size, "modified": {"secs_since_epoch": m.st_mtime_ns // 10**9,
        "nanos_since_epoch": m.st_mtime_ns % 10**9}, "content_hash": "0" * 64}]})
print(r)                                    # observed: ref:000... accepted
p.stdin.close()
p.wait()
print("Synthetic artifacts:", tmp)
```

Для regression-проверки палитры минимальный недостающий assertion внутри существующего headless harness: для каждого keycap `Shape::Rect` с `fill == Color32::from_white_alpha(12)` проверить `keys_region.contains_rect(rect.expand(stroke_margin))`, а не только `viewport.contains_rect(clipped.clip_rect)`. На текущем `Ctrl Minus` обычной ширины он должен обнаружить выступ слева, приведённый в F06. Затем проверять отсутствие пересечения actual painted label/keycaps и единый vertical anchor; один такой assertion не заменяет полный production render.

Архитектурный критерий 1.3 прост: следующему агенту не нужно помнить секретное значение строки `source`, угадывать потерянную 129-ю пачку или считать обрезанный прямоугольник доказательством хорошего layout. Это должны запрещать API, scheduler и проверяемый контракт компонента.
