# Аудит поисковых грамматик vbuff (тема T1)

Дата: 2026-07-31. База: HEAD `db8d43c` (волна A закрыта).
Метод: чтение кода + компилируемый repro нормализации строк (`rustc` вне репозитория).
Статус: **только факты и карта**. Реализация реестра `FacetSpec` — предмет отдельной
задачи; здесь нет предложений по дизайну.

Все номера строк относятся к продакшн-коду на HEAD. Параллельно идущие агенты добавили
в `recall/query.rs` и `store/search.rs` только `#[cfg(test)]`-блоки (`git diff HEAD`:
+454 и +213 строк, все внутри `mod tests`), поэтому нумерация продакшн-строк совпадает
с HEAD и с рабочим деревом.

---

## 1. Проверка утверждений ревью T1

| # | Утверждение ревью | Вердикт | Что фактически |
|---|---|---|---|
| 1 | Три грамматики поиска | **Подтверждено и занижено — их пять** | Две текстовые грамматики (`recall/query.rs`, `store/search.rs`) плюс три нетекстовых фильтрующих словаря: `HistoryScope` (gui/experience.rs), `SearchScopeLock`/`SearchScope` (core/recall/memory.rs), `complete_query` (собственный список префиксов) |
| 2 | Словари не пересекаются | **Подтверждено** | `app kind tag device before after` vs `host color lang iso_date` — пересечение пустое |
| 3 | Каждый парсер молча пожирает чужие фасеты | **Подтверждено, но механика разная** | store: чужой фасет → literal-текст → `LIKE '%app:Chrome%'` → 0 строк. core: чужой фасет → literal-текст → substring-поиск → 0 строк. Ни один не сообщает об ошибке |
| 4 | `core/filter.rs` мёртв | **Устарело — файл удалён в волне A** | Файла нет, `mod filter` отсутствует в `crates/vbuff-core/src/lib.rs:10-31` |
| 5 | Живой in-memory исполнитель `gui/projection.rs` | **Подтверждено** | `filter_clips` (projection.rs:23) — единственный исполнитель на пути GUI |
| 6 | Спящий SQL `Store::search`, потребитель — только `src/ask.rs` | **Частично неверно** | `Store::search` НЕ спит: у него есть ровно один продакшн-вызов — `local_similarity_search_with` (store/lib.rs:1791), который использует его как лексический пре-фильтр кандидатов. `src/ask.rs:88` вызывает `local_similarity_search`, а не `search`. Цепочка: `ask.rs` → `local_similarity_search` → `local_similarity_search_with` → `Store::search` → `Store::search_page`. Прямых вызовов `search`/`search_page` вне store нет вообще |
| 7 | Expiry-фильтр в 3 копиях | **Подтверждено и занижено — 9 копий** | См. §4 |
| 8 | Одна копия expiry с carve-out `session_protected` | **Подтверждено, и carve-out иллюзорен** | Carve-out есть только в `search_page` (store/lib.rs:1547-1550). `purge_expired` (store/lib.rs:2618-2634) удаляет истёкшие строки **без** carve-out, и он запускается на каждом insert. То есть защита живёт до ближайшей вставки |
| 9 | `workflow/everyday.rs` — мёртвый in-memory двойник | **Подтверждено, но это не двойник expiry-фильтра** | Мёртв `SessionProtection` (everyday.rs:195-218) — двойник множества защищённых id, а не фильтра. Продакшн-вызовов нет (только собственный тест everyday.rs:589 и ре-экспорт workflow/mod.rs:21). Живой аналог — `AppState::session_protected` (gui/state.rs:39) + temp-таблица `session_protected` |
| 10 | Добавление фасета = ~8 точек в 3 файлах | **Подтверждено направление, цифра занижена: минимум 13 точек в 4 файлах** | См. §5 |
| 11 | `app:` с `to_ascii_lowercase` не матчит не-ASCII `source_app` | **Подтверждено, воспроизведено** | См. D-1 в §6 |
| 12 | `has_payment_number` пишется, но не в грамматике | **Подтверждено** | Пишется `core/facets.rs:34-37`, читателей нет ни в одной грамматике, ни в SQL, ни в тестах-потребителях |

Дополнительно **не заявленное в ревью, но найденное**:

- `SearchMiss` / `MissSuggestion` (core/recall/search.rs:260-302) — **мёртвая поверхность**:
  ноль потребителей вне vbuff-core. GUI вместо неё показывает `contextual_search_hint`
  (gui/experience.rs:304-317), построенный на другом словаре (`ContentKind`).
- `RecallSearchContext` в проде **всегда `default()`** (gui/projection.rs:35). Следствие:
  фасет `tag:` в GUI не матчит никогда (см. D-3), а `PinnedAlias`/`QueryPinned`/
  `DestinationAffinity` недостижимы.
- Три разных семантики сравнения `source_app` в живом коде (см. §3, строка `app`).
- `SearchSession` (store/lib.rs:156-194) — продакшн-вызовов нет, только тест lib.rs:4263.

---

## 2. Карта поверхностей поиска

### Поверхность A — GUI-попап (единственный интерактивный поиск)

```
пользовательский ввод  PopupApp.query
  ├─ PopupApp::filtered()                       gui/app.rs:311-319
  │    └─ projection::filter_clips              gui/projection.rs:23-85
  │         ├─ parse_natural_query              core/recall/query.rs:46      ГРАММАТИКА 1
  │         ├─ search_recall(.., ctx=default()) core/recall/search.rs:48     ИСПОЛНИТЕЛЬ (in-memory)
  │         │    └─ matches_filters             core/recall/search.rs:119
  │         ├─ clip_is_expired                  gui/projection.rs:87-91      EXPIRY-КОПИЯ
  │         └─ HistoryScope::matches            gui/experience.rs:30-37      ГРАММАТИКА 3 (не текстовая)
  ├─ complete_query(&self.query)                core/recall/search.rs:304    ГРАММАТИКА 5 (список префиксов)
  │    вызывается дважды за кадр: gui/app.rs:519 и gui/app.rs:1172
  └─ broaden_query(&self.query)                 gui/app.rs:3141-3145
       └─ parse_natural_query (третий разбор той же строки)
```

Источник клипов для A: `src/history.rs::refresh_snapshot` (history.rs:425-441)
→ `Store::load_recent` → `Store::list` → `Store::list_with_archive`
(store/data_lifecycle.rs:403-436) → merge с volatile
(`merge_volatile`, history.rs:443-462, фильтр `is_expired` history.rs:465-469).

**Поиск в GUI никогда не доходит до SQL.** Он видит только те клипы, что уже
загружены в `AppState::clips` (лимит `snapshot_limit`).

### Поверхность B — CLI `vbuff ask`

```
src/ask.rs:85-111  run()
  └─ Store::local_similarity_search             store/lib.rs:1772
       └─ local_similarity_search_with          store/lib.rs:1776-1834
            ├─ Store::search                    store/lib.rs:1517-1519
            │    └─ Store::search_page          store/lib.rs:1521-1631      ИСПОЛНИТЕЛЬ (SQL)
            │         ├─ search::parse_query    store/search.rs:39-54       ГРАММАТИКА 2
            │         ├─ SearchPlanner::use_fts store/search.rs:15-17       (LIKE vs FTS5)
            │         └─ SQL expiry + carve-out store/lib.rs:1547-1550      EXPIRY-КОПИЯ
            ├─ fallback Store::list, если search пуст                       store/lib.rs:1793
            ├─ retain(ai_allowed && !sensitive)                             store/lib.rs:1795
            └─ переранжирование по эмбеддингам (результат SQL-ранжирования отбрасывается)
```

### Поверхность C — IPC

`HistoryQuery` (ipc/integration/query.rs:11-42) — **контракт без исполнителя**.
`validate()` проверяет только длину/`\0`/лимит; ни один парсер к нему не привязан.
`rank_launcher_candidates` (query.rs:135-166) ранжирует уже готовые кандидаты
и строку запроса не разбирает вовсе.

### Поверхность D — «жёсткие» фильтры без текста

- `HistoryScope` (gui/experience.rs:10-59) — комбобокс и чипы в попапе
  (gui/app.rs:1232-1281). Варианты `All | Kind(ContentKind) | Snippets | Source(String)`.
  Применяется **в конъюнкции** с текстовой грамматикой 1.
- `SearchScopeLock` / `SearchScope` (core/recall/memory.rs:238-292) —
  `All | App | Device | Collection`. **Ноль потребителей вне vbuff-core.**

---

## 3. Таблица фасетов

Легенда «грамматика»: **G1** = `core/recall/query.rs`, **G2** = `store/search.rs`,
**G3** = `HistoryScope`, **G4** = `SearchScopeLock`, **G5** = `complete_query`.

| Фасет | Грамматика | Нормализация запроса | Нормализация данных | Семантика | Где исполняется | Достижимо в UI |
|---|---|---|---|---|---|---|
| `app:` | G1, G5 | `to_ascii_lowercase` (query.rs:190) | `to_lowercase` (search.rs:125) | подстрока | core/recall/search.rs:121-126 | да, **сломан на не-ASCII (D-1)** |
| `kind:` | G1, G5 | `to_ascii_lowercase` + синонимы (query.rs:267-270) | — (enum) | равенство | core/recall/search.rs:120 | да |
| `tag:` | G1, G5 | `to_ascii_lowercase` (query.rs:190), затем `normalize_label` (memory.rs:404) | `normalize_label` при записи (memory.rs:321) | равенство | core/recall/search.rs:134-137 | **нет — всегда 0 результатов (D-3)** |
| `device:` | G1, G5 | `to_ascii_lowercase` (query.rs:190) | нет | `eq_ignore_ascii_case` (search.rs:132) | core/recall/search.rs:127-133 | да, **инвертированный дефект на не-ASCII (D-2)** |
| `before:` | G1, G5 | `to_ascii_lowercase` + `%Y-%m-%d` / `today` / `yesterday` / `last-*` (query.rs:208-225) | — | `created_at < value` | core/recall/search.rs:138-140 | да |
| `after:` | G1, G5 | то же | — | `created_at >= value` | core/recall/search.rs:141-143 | да |
| `from <app>` | G1 | `to_ascii_lowercase` (query.rs:71) | как `app:` | как `app:` | как `app:` | да |
| `today` / `yesterday` / `last <dur>` / `before lunch` | G1 | голые слова (query.rs:82-112) | — | сдвиг `before`/`after` | как `before`/`after` | да |
| `urls` `links` `images` `code` `files` `colors` … | G1 | `natural_kind` (query.rs:253-262) | — | как `kind:` | как `kind:` | да, **не в `complete_query`** |
| `host:` | G2 | `to_lowercase` (store/search.rs:45) | `to_lowercase` (facets.rs:21) | равенство в `clip_facets` | store/lib.rs:1580-1591 | **нет** — только через `ask` |
| `color:` | G2 | `to_lowercase` | `to_lowercase` (facets.rs:24) | равенство | store/lib.rs:1580-1591 | **нет** |
| `lang:` | G2 | `to_lowercase` | статический слаг (facets.rs:29) | равенство | store/lib.rs:1580-1591 | **нет** |
| `iso_date:` | G2 | `to_lowercase` | **verbatim, без нормализации** (facets.rs:32) | равенство | store/lib.rs:1580-1591 | **нет, и сломан (D-4)** |
| `has_payment_number` | **ни одной** | — | `"true"` (facets.rs:34-37) | — | **нигде** | **нет (D-5)** |
| свободный текст | G1 | `to_lowercase` (search.rs:53) | `to_lowercase` (search.rs:171) | подстрока + опечатки (расстояние 1) | core/recall/search.rs:166-218 | да |
| свободный текст | G2 | нет | нет | `LIKE '%…%'` или FTS5-фраза | store/lib.rs:1557-1578 | только через `ask`, **два яруса расходятся (D-6)** |
| `HistoryScope::Kind` | G3 | — | — | равенство | gui/experience.rs:33 | да (комбобокс) |
| `HistoryScope::Source` | G3 | **нет вообще** | нет | **точное `==`** (experience.rs:35) | gui/experience.rs:35 | да (чипы) |
| `HistoryScope::Snippets` | G3 | — | — | `pinned \|\| favorite` | gui/experience.rs:34 | да |
| `SearchScope::App` | G4 | — | нет | `eq_ignore_ascii_case` (memory.rs:280) | core/recall/memory.rs:273-291 | **нет (мертво)** |
| `SearchScope::Device` | G4 | — | нет | `eq_ignore_ascii_case` (memory.rs:286) | то же | **нет (мертво)** |
| `SearchScope::Collection` | G4 | `normalize_label` | `normalize_label` | равенство | то же | **нет (мертво)** |

**Три семантики для одного поля `source_app`, все живые:**
подстрока после смешанной нормализации (`app:`), точное байтовое равенство
(`HistoryScope::Source`), `eq_ignore_ascii_case` (`SearchScope::App`, мёртвая).

---

## 4. Где живёт фильтр истечения

Девять независимых реализаций одного предиката `expires_at`:

| # | Файл:строка | Форма | Carve-out `session_protected` |
|---|---|---|---|
| 1 | crates/vbuff-store/src/lib.rs:1547-1550 | SQL, `search_page` | **да** |
| 2 | crates/vbuff-store/src/data_lifecycle.rs:419 | SQL, `list_with_archive` (кормит GUI) | нет |
| 3 | crates/vbuff-store/src/data_lifecycle.rs:449 | SQL, `latest_by_recency` | нет |
| 4 | crates/vbuff-store/src/data_lifecycle.rs:475 | SQL, `get_clip` | нет |
| 5 | crates/vbuff-store/src/data_lifecycle.rs:1013 | SQL, дедуп при импорте | нет |
| 6 | crates/vbuff-store/src/lib.rs:2610 | SQL, `count()` | нет |
| 7 | crates/vbuff-store/src/lib.rs:2622 | SQL, `purge_expired()` — **DELETE** | нет |
| 8 | crates/vbuff-gui/src/projection.rs:87-91 | Rust, `clip_is_expired` (вызовы projection.rs:40, app.rs:734) | нет |
| 9 | src/history.rs:465-469 | Rust, `is_expired` (7 вызовов: 84, 183, 218, 391, 402, 412, 416, 449) | нет |

Плюс не-фильтрующие потребители того же поля: `expiry_label`
(core/workflow/everyday.rs:220-236, живой, рендерится в gui/app.rs:2722) и
«скоро истекут» — store/lib.rs:1375.

Расхождения:
- `search_page` (#1) отдаёт session-protected истёкший клип, `list_with_archive` (#2)
  и `count()` (#6) — нет. GUI кормится из #2, значит carve-out в #1 не виден никому,
  кроме `vbuff ask`.
- `purge_expired` (#7) удаляет строку **безусловно**, включая session-protected.
  Он стоит первым оператором `Store::insert` (store/lib.rs:966), а также идёт
  с тика обслуживания (src/history.rs:252) и при импорте
  (store/data_lifecycle.rs:1004), поэтому окно, в котором carve-out #1 что-то
  даёт, закрывается первой же копией в буфер.
- `clip_is_expired` (#8) и `is_expired` (#9) — побайтово один и тот же предикат
  в двух крейтах; #9 берёт `Utc::now()` внутри себя (недетерминированно на каждом
  вызове), #8 принимает `now` параметром.

---

## 5. Стоимость одного нового фасета

Метрика: добавить один фильтрующий фасет с паритетом к существующим
(разбор, фильтрация, объяснение матча, автодополнение, отпечаток запроса).

### Ярус 1 — минимум, чтобы фасет заработал в GUI: **13 точек, 4 файла**

| # | Файл:строка | Роль | Ловит ли компилятор |
|---|---|---|---|
| 1 | crates/vbuff-core/src/recall/query.rs:10-19 | поле в `NaturalQuery` | нет (`derive(Default)`) |
| 2 | crates/vbuff-core/src/recall/query.rs:26-33 | `has_filters()` | нет |
| 3 | crates/vbuff-core/src/recall/query.rs:60-63 | `matches!` — белый список ключей в `parse_natural_query` | нет |
| 4 | crates/vbuff-core/src/recall/query.rs:164-179 | ветка `apply_facet` (второй, независимый список ключей) | нет |
| 5 | crates/vbuff-core/src/recall/query.rs:280-307 | `fingerprint()` — иначе два разных запроса дают один отпечаток | нет |
| 6 | crates/vbuff-core/src/recall/search.rs:9-21 | вариант `MatchExplanation` | нет |
| 7 | crates/vbuff-core/src/recall/search.rs:119-144 | `matches_filters` — собственно фильтр | нет |
| 8 | crates/vbuff-core/src/recall/search.rs:146-164 | `filter_explanations` | нет |
| 9 | crates/vbuff-core/src/recall/search.rs:260-268 | вариант `MissSuggestion` | нет |
| 10 | crates/vbuff-core/src/recall/search.rs:276-302 | `SearchMiss::for_query` | нет |
| 11 | crates/vbuff-core/src/recall/search.rs:328 | список префиксов в `complete_query` | нет |
| 12 | crates/vbuff-gui/src/projection.rs:93-108 | массив приоритетов `preferred_match_explanation` | **нет** (массив, не `match`) |
| 13 | crates/vbuff-gui/src/app.rs:3111-3123 | `match_explanation_label` | **да** (исчерпывающий `match`) |

Из тринадцати точек компилятор ловит **одну**. Пропуск #5 портит query-пины;
пропуск #12 делает объяснение матча невидимым; пропуск #11 — фасет не автодополняется.
Пункты #9-10 сегодня бессмысленны сами по себе (`SearchMiss` мёртв), но останутся
в списке до её удаления или подключения.

### Ярус 2 — тот же фасет в SQL-движке: **+3 точки и миграция**

| # | Файл:строка | Роль |
|---|---|---|
| 14 | crates/vbuff-store/src/search.rs:44 | белый список ключей `parse_query` |
| 15 | crates/vbuff-core/src/facets.rs:15-41 | `extract_facets` — писатель в `clip_facets` (только если значение не колонка `clips`) |
| 16 | crates/vbuff-store/src/lib.rs:1580-1591 | ветка SQL (только если значение — колонка `clips`, а не строка `clip_facets`) |

Плюс бэкфилл: `clip_facets` заполняется **только на вставке**
(store/lib.rs:1106-1111 и 1180-1185), существующие строки нового фасета не получат.

### Ярус 3 — паритет с «жёсткими» фильтрами: **+7 точек**

| # | Файл:строка | Роль |
|---|---|---|
| 17 | crates/vbuff-core/src/recall/memory.rs:238-244 | вариант `SearchScope` |
| 18 | crates/vbuff-core/src/recall/memory.rs:273-291 | `SearchScopeLock::matches` |
| 19 | crates/vbuff-gui/src/experience.rs:10-16 | вариант `HistoryScope` |
| 20 | crates/vbuff-gui/src/experience.rs:30-37 | `HistoryScope::matches` |
| 21 | crates/vbuff-gui/src/experience.rs:39-46 | `HistoryScope::label` |
| 22 | crates/vbuff-gui/src/experience.rs:48-58 | `HistoryScope::from_jump_key` |
| 23 | crates/vbuff-gui/src/app.rs:1232-1281 | комбобокс и чипы |

**Итого: 13 / 16 / 23** в зависимости от требуемого охвата (ревью оценивало ~8).

---

## 6. Подтверждённые дефекты

Repro D-1…D-4 проверены компилируемой программой на тех же выражениях, что стоят
в коде; вывод приведён дословно.

### D-1. `app:` не матчит не-ASCII имя приложения, набранное как оно выглядит

Запрос: `to_ascii_lowercase` (core/recall/query.rs:190).
Данные: `to_lowercase` (core/recall/search.rs:125). Асимметрия.

```
source_app=Übersicht  typed=app:Übersicht  query=Übersicht  clip=übersicht  MATCH=false
source_app=Übersicht  typed=app:übersicht  query=übersicht  clip=übersicht  MATCH=true
source_app=Яндекс     typed=app:Яндекс     query=Яндекс     clip=яндекс     MATCH=false
source_app=Яндекс     typed=app:яндекс     query=яндекс     clip=яндекс     MATCH=true
source_app=Chrome     typed=app:CHROME     query=chrome     clip=chrome     MATCH=true
```

Воспроизведение в приложении: скопировать что-нибудь из приложения с не-ASCII
заглавной в имени (`Übersicht`, `Яндекс.Браузер`, `Überschrift`), затем набрать
`app:Übersicht`.

Эффект для пользователя: фасет по приложению молча даёт ноль результатов ровно
на том написании, которое пользователь видит в UI и получает из автодополнения
списка приложений. Работает только полностью строчный ввод. ASCII-имена не затронуты,
поэтому дефект невидим на англоязычных стендах.

### D-2. `device:` сломан зеркально к `app:`

`eq_ignore_ascii_case` (core/recall/search.rs:132) против `to_ascii_lowercase`
на стороне запроса:

```
origin_device=Ноутбук  typed=device:Ноутбук  MATCH=true
origin_device=Ноутбук  typed=device:ноутбук  MATCH=false
origin_device=Laptop   typed=device:LAPTOP   MATCH=true
```

Эффект: для не-ASCII имён устройств `app:` требует строчный ввод, а `device:` —
ровно наоборот, точное написание. Два соседних фасета в одной строке запроса
ведут себя противоположно, и ни одно правило не работает для обоих.

### D-3. `tag:` в GUI не матчит никогда

`matches_filters` (core/recall/search.rs:134-137) требует `Some(&ClipTags)`,
а `filter_clips` передаёт `RecallSearchContext::default()` (gui/projection.rs:35),
где `tags: None`. `Option::is_some_and` на `None` даёт `false`, значит предикат
отбраковывает любой клип.

`ClipTags` не сконструирован нигде в `src/` и `crates/vbuff-gui/` (проверено
grep по обоим деревьям).

Эффект: `tag:` присутствует в грамматике, в автодополнении
(core/recall/search.rs:328) и в объяснениях матча (gui/app.rs:3116), но всегда
возвращает пустой список. То же касается `PinnedAliases`, `PasteAffinity`,
`QueryPinSet` — соответствующие им `MatchExplanation` недостижимы.

### D-4. `iso_date:` не может совпасть при наличии времени

Писатель кладёт значение **verbatim** (`facet("iso_date", trimmed)`, core/facets.rs:32),
читатель приводит значение запроса к нижнему регистру (store/search.rs:45).
Регулярка facets.rs:66 допускает `T` и `Z`:

```
stored=2026-07-31T12:00:00Z  queried=2026-07-31t12:00:00z  SQL_EQUAL=false
```

Эффект: `iso_date:` работает только для голых дат `YYYY-MM-DD`; любая метка
времени с `T`/`Z` не находится никогда. Через UI недостижимо в принципе (D-7).

### D-5. `has_payment_number` пишется, но не читается

Писатель: core/facets.rs:34-37 — строка в `clip_facets` на каждой вставке,
где Luhn-валидная последовательность из 13-19 цифр и клип не sensitive.
Читателей нет: ключа нет ни в `store/search.rs:44`, ни в `core/recall/query.rs:62`,
ни в SQL, ни в каком-либо экспортёре (grep по всему репозиторию, исключая `.git`,
даёт только сам писатель, его тест и текст ревью).

Эффект: приватностно-чувствительный признак «в этом клипе есть номер карты»
персистится в БД без единого потребителя. Пользователь не может им
воспользоваться, а признак присутствует в дампе БД.

### D-6. Ярусы LIKE и FTS расходятся на не-ASCII регистре на одной и той же БД

`use_fts` (store/search.rs:15-17) переключает ярус по числу строк (порог 250)
или по p95-латентности. LIKE наследует ASCII-only фолдинг SQLite; FTS5 настроен
как `unicode61 remove_diacritics 2` (store/lib.rs:653-654).

Независимо подтверждено характеризационным тестом
`like_and_fts_tiers_disagree_on_non_ascii_case_folding`
(store/lib.rs:4142-4161, добавлен параллельным агентом в рабочем дереве, не на HEAD):
`ПРИВЕТ` не находит `привет мир` на 1 строке и находит на 261.

Эффект: результат одного и того же запроса меняется по мере роста истории,
без действий пользователя. Плюс `remove_diacritics 2` делает FTS-ярус
нечувствительным к диакритике, а LIKE-ярус — чувствительным.

### D-7. Единственный интерактивный поиск не видит ни одного store-фасета

`host:`, `color:`, `lang:`, `iso_date:` живут только в грамматике G2, а G2
достижима только через `vbuff ask`. В GUI такие токены проваливаются в свободный
текст: `host:docs.rs` ищется как литеральная подстрока `host:docs.rs` в содержимом.

Симметрично: `app:`, `kind:`, `tag:`, `device:`, `before:`, `after:` в `vbuff ask`
превращаются в `LIKE '%app:Chrome%'`.

Эффект: обе стороны молча дают ноль результатов вместо «неизвестный фасет».
Ошибку отличить от «ничего не нашлось» невозможно.

### D-8. Незавершённый фасет обнуляет список по мере набора

`app:` без значения → `apply_facet` → `value.is_empty()` → `QueryParseError::InvalidFilter`
(core/recall/query.rs:158-163) → `filter_clips` возвращает `Vec::new()`
(gui/projection.rs:30-34).

Эффект: пока пользователь набирает `app:chrome`, в момент, когда набрано `app:`,
список полностью пустеет и снова наполняется после первой буквы значения.
Автодополнение в этот момент предлагает `app:` — то, что уже набрано
(core/recall/search.rs:333-337).

В store-грамматике та же строка ведёт себя иначе: `host:` с пустым значением
не проходит guard `!value.is_empty()` (store/search.rs:44) и уходит в текст.

### D-9. Кавычки работают только в одной грамматике

G1 поддерживает кавычки (`tokenize`, core/recall/query.rs:128-150) и падает
с `InvalidSyntax` на незакрытой кавычке. G2 режет по `split_whitespace`
(store/search.rs:42) и кавычки не удаляет: `"release note"` собирается обратно
в текст **вместе с кавычками** и уходит в `LIKE '%"release note"%'`.

Эффект: фразовый поиск, который работает в GUI, в `vbuff ask` не находит ничего.

### D-10. Дубль фасета: ошибка против молчаливого пустого множества

G1 отвергает `app:a app:b` (`set_once`, core/recall/query.rs:183) с `InvalidFilter`,
GUI показывает пустой список.
G2 принимает `host:a host:b` и клеит два `AND EXISTS`
(store/lib.rs:1580-1591) — конъюнкция, выполнимая только для клипа с двумя
одинаковыми ключами и разными значениями.

Эффект: одинаково пустой результат по двум разным причинам, ни одна не объяснена.

### D-11. Тест, закрепляющий поведение, которого нет

`gui/projection.rs:129-142`, `malformed_structured_query_fails_closed`, утверждает,
что `"unknown:needle"` фильтруется fail-closed. Фактически `unknown:` не попадает
в белый список (query.rs:60-63), парсинг **успешен**, токен уходит в свободный
текст, и тест проходит лишь потому, что подстрока `unknown:needle` не встречается
в содержимом фикстур (второй клип sensitive и не сканируется — search.rs:167).

Эффект на разработку: тест создаёт ложную уверенность, что неизвестные фасеты
отвергаются. Реальный fail-closed срабатывает только на `QueryParseError`
(незакрытая кавычка, дубль фасета, пустое значение, `kind:` с неизвестным слагом).

### D-12. `session_protected`-carve-out в поиске нейтрализуется `purge_expired`

`search_page` (store/lib.rs:1547-1550) отдаёт истёкший клип, если он в
`session_protected`. `purge_expired` (store/lib.rs:2618-2625) удаляет такой клип
без исключений и стоит первым оператором `Store::insert` (store/lib.rs:966),
плюс вызывается с тика обслуживания (src/history.rs:252).

Эффект: обещание «клип, защищённый на сессию, переживёт свой TTL в поиске»
выполняется до следующей копии в буфер. К тому же путь, кормящий GUI
(`list_with_archive`, data_lifecycle.rs:419), carve-out не имеет вовсе — то есть
даже в это окно пользователь защищённый клип в попапе не увидит.

---

## 7. Сводка по расхождениям и пользовательскому эффекту

| Дефект | Достижим из GUI | Пользовательский эффект | Тихий отказ |
|---|---|---|---|
| D-1 `app:` не-ASCII | да | 0 результатов на видимом написании имени приложения | да |
| D-2 `device:` не-ASCII | да | правило, обратное D-1, в том же запросе | да |
| D-3 `tag:` без `ClipTags` | да | фасет всегда даёт 0 | да |
| D-4 `iso_date:` с временем | нет (только `ask`) | дата с временем не находится никогда | да |
| D-5 `has_payment_number` | нет | признак карты в БД без потребителя | — |
| D-6 LIKE vs FTS | нет (только `ask`) | результат меняется от размера истории | да |
| D-7 непересекающиеся словари | да | чужой фасет = поиск литерала | да |
| D-8 незавершённый фасет | да | список мигает в пустоту при наборе | да |
| D-9 кавычки | нет (только `ask`) | фразовый поиск не переносится между поверхностями | да |
| D-10 дубль фасета | да | пустой результат по двум разным причинам | да |
| D-11 ложный тест | — | ложная уверенность при рефакторинге | — |
| D-12 carve-out + purge | да | «защита на сессию» не переживает следующую копию | да |

Одиннадцать из двенадцати расхождений проявляются как «ничего не найдено»
без диагностики. Единственный канал обратной связи в GUI —
`contextual_search_hint` (gui/experience.rs:304-317), который смотрит на состав
истории и о запросе не знает ничего; `SearchMiss`, спроектированная именно
для этого, не подключена.

---

## 8. Что придётся решить (без предложения решения)

Открытые вопросы, которые аудит фиксирует, но не закрывает:

1. `Store::search` / `search_page` не имеют внешних потребителей, кроме
   пре-фильтра в `local_similarity_search_with`. Ранжирование SQL там
   отбрасывается. Оставлять ли SQL-поиск как поверхность вообще — вопрос,
   а не факт.
2. `SearchSession`, `SearchScopeLock`/`SearchScope`, `SearchMiss`/`MissSuggestion`,
   `PinnedAliases`, `PasteAffinity`, `QueryPinSet`, `everyday::SessionProtection` —
   API без продакшн-потребителей, но с тестами. Удалять или подключать — решение
   вне рамок этого документа.
3. `clip_facets` заполняется только на вставке. Любое изменение словаря фасетов
   требует бэкфилла, иначе новый фасет молча не видит старую историю.
4. Неизвестный ключ `foo:bar` неотличим от URL `https://example.test`
   (пин-тест core/recall/query.rs, `duplicate_and_unknown_filters_are_rejected`),
   поэтому «отвергать неизвестные фасеты» — не бесплатное решение.
