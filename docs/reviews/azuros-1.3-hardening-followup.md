# Azuros · повторная проверка hardening 1.3

2026-11-10. **Повторный обзор завершён по сигналу интегратора о заморозке продуктового кандидата 1.3.0.** Финальная объединённая проверка последней пары изменений ещё выполнялась на момент фиксации этого текста; её результат сообщает интегратор отдельно. Это read-only re-review незакоммиченного кандидата относительно checkpoint `585df0c`, а не повторное заключение о старой 1.2.6. Исходный `azuros-1.3-readiness.md` сохранён неизменным. Продуктовый код, native приложения, runtime pointer, личные файлы и Clipboard при этой проверке не менялись.

## Результат повторного обзора

После свежего чтения исправлений **дополнительных подтверждённых блокирующих ошибок не осталось**. Несколько реальных контрпримеров были найдены и переданы интегратору в ходе этой проверки; они уже устранены в читаемом коде. Два из них независимо перепроверены исполняемыми synthetic probes после исправления. Это ещё не утверждение о прохождении всех объединённых тестов, готовности установленного приложения или Windows appearance.

Nein, наличие нового scheduler само по себе не считалось исправлением старого UI пути. Проверялись фактические submit/poll/commit места: что делает drop, что происходит с Clipboard во время foreground job, когда разрешается удаление и что действительно сохраняет Import.

## Закрытые контрпримеры

| Случай | Доказательство до исправления | Свежая проверка |
|---|---|---|
| Два подготовленных одинаковых blob | Два `BeginJob(IngestBytes)` с одинаковыми 20 bytes были готовы до первого poll. Первый succeeded, второй `prepared_conflict`; вместо двух arrivals оставался один | Независимый повтор после исправления: оба succeeded, ID 1/2, общий blob path, List содержит два события. В storage появился учёт проверенной concurrent publication; неизвестный/изменённый target не принимается вслепую |
| Import выдавал текущие bytes вместо записанной revision | Import стоял за fake blocked preview; reference изменён после submit. Job/item возвращали success, output содержал `different current bytes` | Независимый повтор: batch job succeeded, item `ok:false / source_changed`; destination отсутствует. Export проверяет hash фактически скопированных bytes и очищает неуспешный output |
| Clipboard по-прежнему ждал foreground job | UI `event_capture_tick` прекращал работу при `self.job.is_some()`, хотя отдельная Capture lane уже существовала | Теперь отдельный `capture_job`, отдельный drain и gate; foreground job больше не запрещает capture submission. Проверено чтением актуального execution path; полный UI regression остаётся в объединённой проверке интегратора |
| Классификация теряла inflight после restart schema 2 | Requeue находился внутри migration, которая при текущей версии возвращалась раньше | Отдельный `classification_runtime::recover_startup` вызывается при открытии writer независимо от миграции; добавлена fixture текущей schema с inflight rows |
| Idle source tick перечитывал всю classification history | Безусловный `SUM(state...) FROM classification_work` на каждом poll был O(history), даже без queued work | Счётчики состояний вынесены в маленькую таблицу с triggers; добавлена проверка одинакового SQL VM work на 100k/1m done rows |
| Matcher создавал повторный cross product | На каждый digest выполнялся JOIN всех matching clipboard × source. Даже уже сопоставленные 1000 clipboard/10 source требовали около 212000 VM steps, changes=0 | Source digest регистрируется один раз по `(digest,generation)`; visibility вычисляется indexed EXISTS для конкретного clipboard event. Повторное материальное перечисление всех пар убрано |
| Warmup существующего initializing QuickLook завершался раньше readiness | `start()==false` немедленно возвращал current state, включая ready:false | Initializing maintained instance теперь присоединяется к bounded readiness wait без повторного spawn; готовность не выводится из успешного создания процесса. Добавлен соответствующий fake-startup regression |
| Shell deadline не покрывал запись stdin | До deadline loop стоял blocking `write_all`; зависший до чтения helper мог удержать worker | Ввод пишет один tracked thread; supervisor контролирует общий deadline, убивает дочернее дерево через Job Object, reap/join выполняются. Автоматический sibling helper требует manifest protocol/hash, чтобы старый UI executable не был запущен как будто он поддерживает worker flag |

Финальные проверенные точки кода: `src/storage.rs:198`, `:370` (publication/dedupe); `src/lib.rs:630` (export); `src/bin/api_client.rs:34`, `:91` и `src/bin/clipboard_events_ui.rs:89` (отдельный capture job); `src/classification_runtime.rs:15`, `:47`, `:103` (startup recovery, digest registry, bounded counters); `src/catalogue_schema.rs:93`, `:106` (counter triggers и digest schema); `src/quicklook_backend.rs:140` (readiness); `src/shell_visual_worker.rs:139`, `:239`, `:339`, `:379` (attestation, supervisor, tracked write, kill/reap). UI worker entry вызывается до обычного startup в `src/bin/ui.rs:651`.

## Опасные границы, которые проверены повторно

- Generic `IngestPath` не выдаёт original deletion policy по строке provenance. Live policy выдаётся private source commit и сверяется с current configuration/projection; новые generic references не превращаются в originals при совпадении source/path (`src/folder_commit.rs:74`). Raw serialized candidates и screenshot pairs остаются отвергаемыми wire-командами (`src/lib.rs:282`, `:284`).
- `SetSettings` немедленно синхронизирует source generations; это не откладывается до следующего UI poll. A→B→A имеет отдельную проверку старого полномочия. Финальное изменение сохраняет это свойство, но не запускает native workers из простого сохранения preferences: `src/lib.rs:438`, `src/source_service.rs:10`, `:68`. Старт — явный Ensure/Poll либо reconfiguration уже запущенного runtime.
- Асинхронное original deletion теперь проходит prepare → dispatcher authorization → fenced worker effect → shelf receipt (`src/delete_jobs.rs:61`, `:124`, `:141`, `:263`, `:274`). ShareX удерживает проверенный deny-write/deny-replacement handle до disposition (`src/delete_selection.rs:28`, `:107`). Downloads использует injectable recycler; атомарная identity-транзакция с pathname-based Windows recycling не обещается. Save и confirmed Delete теперь отправляют BeginJob через `src/bin/ui.rs:552`, `:581`.
- Source workers больше не принадлежат widget lifecycle. В UI остались Ensure/Poll/Rescan clients; generation-tagged private receipts и retired-worker bound находятся в библиотеке.
- `ActionRow`/`KeyCaps` используют измеренные galleys для paint, stroke внутри frame, общий vertical center и ancestor clip (`src/bin/action_row.rs:11`, `src/bin/ui_style.rs:76`). Production-style tests проверяют actual bounds, а не только нахождение clip rect внутри viewport (`src/bin/action_row.rs:103`, `:230`). Подтверждённого нового дефекта этого компонента при чтении не получено.
- QuickLook transport ограничивает frame и I/O, проверяет pipe-server PID, executable identity, instance nonce и ACK ID/version. После неоднозначной записи выполняется lookup того же ID, не повторный Toggle и не CLI replay. Отдельный stock CLI compatibility mode не объявляется fast acknowledged transport.
- Миграция отвергает future schema и теперь сохраняет consistent pre-v2 SQLite checkpoint через `VACUUM INTO` до изменения исходной базы (`src/catalogue_schema.rs:20`, `src/lib.rs:100`). Это необходимая страховка rollback: старый 1.2.x binary нельзя направлять на upgraded live DB, поскольку его старый код перезаписывал schema version. Добавленная проверка — `tests/hardening_contracts.rs:428`.

## Что это заключение не снимает

1. **20 000 directory entries остаются явным source limit** (`src/sharex.rs:22`, `src/downloads.rs:11`, `src/folder_watch.rs:357`). ShareX/Downloads discovery и availability snapshot всё ещё all-or-error на большем дереве. Проверки каталога/idle classification на 100k/1m событий не доказывают поддержку 100k/1m файлов в source root. Если такой root становится целевым сценарием, нужна продолжимая discovery с cursor; этот предел нельзя называть уже устранённым.
2. Source revision verification для copy/export/delete остаётся строгой. Warm file visuals избавлены от лишних повторных проверок, но исходное чтение/hash большого reference остаётся I/O работой. Не следует объявлять warm cache нулевым I/O.
3. Cancellation не отменяет уже начатый внешний эффект. Заблокированный filesystem read не получает магического hard timeout от наличия Rust thread; ограничения admission и retiring work следует сохранять.
4. Windows Shell/QuickLook acceptance и реальный вид палитры проверяет владелица. ACK означает принятие/dispatch запроса, а не готовые пиксели. Этот re-review не запускал GUI Katran, installed QuickLook или Explorer.

## Фактически выполненная независимая проверка

Общие cargo tests/clippy здесь повторно не запускались, чтобы не блокировать `target/build` одновременно с интеграционными агентами. Были скомпилированы маленькие временные Rust probes, связанные с уже собранной библиотекой из `target/build/debug/deps`, затем выполнены только synthetic API сценарии. Native Clipboard/recycler/QuickLook в probes заменены либо вообще не вызывались.

По сообщению интегратора перед последними двумя изменениями прошли all-target/all-feature tests: 50 library, 39 UI и integration suites, а также Clippy с denied warnings. После добавления pre-v2 checkpoint и настройки «persist/revoke без неявного старта workers» интегратор запустил повторный общий прогон. Я прочитал финальные изменения; не приписываю себе запуск или окончательный результат этого прогона. Новые адресные regressions включают `tests/job_dispatch.rs:301`, `:361`, `src/bin/clipboard_events_ui.rs:227`, `src/classification_runtime.rs:114`, `tests/hardening_contracts.rs:357`, `:428`.

Артефакты: `C:/Users/lilya/AppData/Local/Temp/katran-followup-v9_xof9r/`.

- `probe.rs`, `output.txt`: первоначальный concurrent-dedupe counterexample.
- `probe-recheck.txt`: оба arrivals успешно сохранены после исправления.
- `import_probe.rs`, `import_output.txt`: первоначальный stale-reference Import counterexample.
- `import-fixed-recheck.txt`: per-item `source_changed`, `DESTINATION_EXISTS false` после исправления.

SQL cross-product измерялся отдельно на synthetic in-memory SQLite с точной прежней формой запроса, без файлов пользователя: already-matched 100 clips/1 source ≈3200 VM steps; 1000/1 ≈32000; 1000/10 ≈212000; во всех трёх случаях changes=0. Это доказательство прежней формы запроса, не latency benchmark нового production matcher.

## Финальный приёмочный шлюз

Интегратор завершает объединённые fmt/test/clippy, включая blocked foreground + capture, concurrent prepared duplicates, stale Import, delete authorization/cancellation/blocked recycler, schema restart, bounded idle/matching SQL work и helper timeout до чтения stdin. Затем — отдельный runnable build и ручная проверка владелицы. Явные остаточные пределы описаны в `docs/domain-jobs.md`; они не скрываются под заявлением о hard real-time или неограниченном source traversal. Полный исходный аудит остаётся историей проверенной 1.2.6; этот follow-up не заменяет build manifest и не разрешает публикацию на GitHub.
