# Патч-план: топ-10 критичных находок дизайн-ревью

Дата: 2026-07-26. Каждая находка подтверждена чтением кода на текущем HEAD отдельным проектировщиком. Порядок исполнения — в конце документа.

---

## БАГ-1. Мёртвая клавиатурная навигация в попапе — S (<1 дня)

**Подтверждение.** `history_search_id()` (`crates/vbuff-gui/src/app.rs:3232`) возвращает «сырой» `Id::new("vbuff_history_search")`, но TextEdit создан с `.id_source(...)` (app.rs:1083) — в egui 0.35 это `id_salt`, финальный id = `ui.id.with(salt)`, никогда не равен сырому. Оба гейта (автодополнение app.rs:509-525, шорткаты app.rs:534-538) всегда false; фокус при открытии всегда в поиске → мертвы стрелки, Home/End, PgUp/PgDn, Enter, Cmd+1..9. Тот же корень ломает a11y-ноду `EditableComboBox` (app.rs:939).

**Фикс — одна строка:** `.id_source(history_search_id())` → `.id(history_search_id())` (app.rs:1083). Глобальный Id безопасен: единственный TextEdit с этим id, коллизий нет (grep чист).

**Тесты.** Unit: контрактный тест «`TextEdit::id(...)` → `memory.focused() == history_search_id()`» (упал бы на старом коде); тест-пин `ui.id.with(x) != x`. Integration (kittest): фокус на стабильном id; ArrowDown+Enter → `UiAction::Paste(clip[1])`; End → Paste последнего; Cmd+Num2 → quick-select.

**Риски.** Восстановление ветки автодополнения меняет приоритет стрелок при видимых подсказках — задокументированный дизайн, упомянуть в changelog. Независим, мержить первым.

---

## БАГ-2. Приватность fail-open вместо fail-closed — M (1–2 дня)

**Подтверждение.** `policy.rs:248` матчит `excluded_apps` против `provenance.app_id.unwrap_or_default()`; arboard никогда не заполняет provenance (`clipboard.rs:58-61` → `..Default::default()`) → правила мертвы, захват разрешён. `concealed: bool` дефолтит в `false` и трактуется как «доказано чисто», хотя означает «неизвестно».

**Ключевая идея:** неизвестность provenance имеет значение только когда вооружены source-правила (`excluded_apps`/`source_rules` непусты). Без правил поведение дефолтных пользователей не меняется вообще.

**План.**
1. `vbuff-types`: `ProvenanceConfidence { Proven, #[default] Unknown }`, `ConcealmentSignal { Concealed, Clear, #[default] Unknown }` — дефолты fail-closed.
2. `vbuff-platform/traits.rs`: `CapturedClipboard`: `concealed: bool` → `concealment` + `provenance_confidence`; трейт `ClipboardBackend::evidence()` с fail-closed дефолтом. arboard компилируется без правок логики (Unknown/Unknown автоматически).
3. `policy.rs`: новая ручка `UnknownEvidencePolicy { #[default] Guard, Skip, Allow }`. Логика: `Concealed` → skip (как сейчас); concealment Unknown → без изменения поведения (иначе вся история маскируется — приложение бесполезно; остаточный риск закрыт content-детекторами и strict-режимом — осознанный задокументированный tradeoff); provenance Unknown + правила вооружены → Guard: захват как `sensitive + sync_eligible=false + ai_allowed=false + TTL + "Masked: system privacy hint"`. Новые `DropReason::{ProvenanceUnknown, ConcealmentUnknown}`; `is_recoverable(ProvenanceUnknown) = true` — подключает уже существующий skipped-recovery (явное согласие пользователя).
4. `src/config.rs`: ключ `unknown_evidence_policy = guard|skip|allow` (дефолт guard), включая `CONFIG_KEYS`.
5. UI: `CaptureHealth::DegradedSourcePrivacy` (аддитивно, snake_case) — popup/трей/empty-state подхватят через `label()` без правок GUI. Стартовый notice при активной деградации.
6. Store: `StoredMetadata` += `provenance_confidence` (`#[serde(default)]` — без миграции схемы); `ClipMeta` += поле.

**Тесты.** Unknown+armed+Guard → capture с sensitive-флагами, правила не вычисляются; Unknown без правил → обычный захват во всех режимах (страж дефолтного UX); Proven+armed → прежний матчинг; serde-старые БД → Unknown.

**Риски.** Не шипить дефолтный `excluded_apps` до нативного provenance-бэкенда (иначе вся история уйдёт в маскированный guard). UX-сюрприз для вооружённых пользователей — намеренный, объяснить notice'ом.

---

## БАГ-3. Секреты в открытом SQLite — M (1–3 дня)

**Подтверждение (уточнено).** TTL у capture-детекта есть (600 c по умолчанию), но: (а) forced-sensitive правила дают `expires_after: None`; (б) dedup-bump стирает TTL при re-copy; (в) `clawback_sensitive` не чистит `normalized_hash` (lib.rs:2002-2017) — реклассифицированный клип коррелируется через `near_duplicate_group`; (г) `import_quarantine` хранит sensitive-пейлоады cleartext бессрочно (data_lifecycle.rs:881-919, без TTL; API из бинарника не вызывается). Grace bin, наоборот, шифруется (`lifecycle::seal_clip`) — чисто.

**План (точечно, сигнатуры не меняются).**
1. `Store::insert`: fail-closed потолок `SENSITIVE_TTL_CEILING = 24ч` для sensitive без `expires_at` (покрывает insert и dedup-bump). Merge TTL → монотонный минимум. Опционально: forced-sensitive ветка policy → `Some(secret_ttl)` (10 мин).
2. `clawback_sensitive` += `normalized_hash = NULL`; то же в миграционный scrub при open (lib.rs:744-752, без bump user_version); `near_duplicate_group` += фильтр `sensitive = 0` (defense-in-depth).
3. `stage_import`: отказ от sensitive-импорта (`Err`) до появления шифрования карантина; `purge_import_quarantine` (TTL 7 дней) с вызовом при open рядом с `gc_blobs()`. Задокументировать в data-contract-v3.md.

**Отклонено:** «не писать тело sensitive вообще» — ломает re-paste после рестарта; future work вместе с шифрованием БД.

**Тесты.** Bounded expiry для sensitive; dedup-bump не растягивает TTL; clawback обнуляет hash (прямой SQL); группа исключает sensitive; import Err для declared/detected sensitive; purge по возрасту; integration: scrub на open чистит stale hash.

---

## БАГ-4. Блокирующий wal_checkpoint(TRUNCATE) на каждом удалении — S (<1 дня)

**Подтверждение.** `scrub_deleted_pages` (lib.rs:2684-2694) вызывается синхронно из 14 путей удаления, включая горячий `History::insert → enforce_cap` (src/history.rs:54-60) — каждый захват при полной истории делает DELETE + TRUNCATE под мьютексом. Busy-чекпоинт → `Err` после успешного коммита → ложный фейл операции. `secure_delete=ON` — персистентный pragma, не трогаем. `maintenance_state` уже существует — миграция не нужна.

**План.**
1. Seed `pending_wal_scrub = 0` (`INSERT OR IGNORE`, идемпотентно).
2. `scrub_deleted_pages` → дешёвый инкремент счётчика, ошибку не возвращает. Все 14 call sites без изменений; удалить параметр `scrub_after` из `delete_with_grace_inner`.
3. Новый `pub fn scrub_wal_if_dirty() -> Result<bool>`: dirty → TRUNCATE; busy → `Ok(false)` + PASSIVE-fallback, не ошибка.
4. Вызов из `maintain_idle` (тик 60 c уже есть) + `MaintenanceSummary.wal_scrubbed`; `flush_for_shutdown()` — best-effort вызов из `main` после `app::run`.

**Честный tradeoff:** окно читаемости pre-delete WAL-фреймов растёт от 0 до ~60 с — задокументировать в doc-comment и problems-improvements-top-500.md (пункты 56/57 устарели — обновить).

**Тесты.** delete маркирует dirty; фоновый scrub чистит; **busy-регрессионный:** второе соединение держит read-транзакцию → `delete` Ok, scrub `Ok(false)`, после закрытия reader → `Ok(true)`; grace-flow при busy WAL.

---

## БАГ-5. Ротация ключей обновлений — M (1–1.5 дня)

**Подтверждение (шире описанного, три дефекта).** A: `UpdateKeyring::trust` (manifest.rs:154-161) перезаписывает любой key_id без проверки — активный ключ может подменить чужой (включая корневой), перезапись стирает `revoked_at_sequence`. B: `KeyRotation` без подписи нового ключа — нет proof-of-possession. C: `highest_accepted_sequence` и кейринг только в памяти — каждый рестарт = replay/даунгрейд-окно.

**План (всё в `vbuff-update`).**
1. `trust`: занятый key_id → `Err(DuplicateKeyId)`.
2. `revoke`: повторная ревокация сохраняет min(sequence) — нерушимо.
3. `KeyRotation.confirmation`: подпись Ed25519 нового ключа над доменным блобом с привязкой к `manifest_sequence`; schema bump `{1,2}`, `schema 1 + next_key` → Err (старые клиенты fail-closed); verify confirmation до `trust`.
4. Новый `state.rs`: `VerifierState { keyring, highest_accepted_sequence }`, `load` (битый файл → Err, никакого сброса к 0), `store` через `atomic-write-file` (паттерн из vbuff-store). Вызывающий зовёт `store()` после успешного `verify()`.
5. Ошибки: `DuplicateKeyId`, `RotationNotConfirmed`.

**Тесты.** Перезапись key_id → Err; ревокация переживает ротацию; атака на корневой ключ → Err, жертва работает; confirmation третьим ключом → Err; **рестарт:** verify(10) → store → load → повторный verify(10) → DowngradeOrReplay; end-to-end ротация через рестарты.

**Заметка:** семантика accept-before-install (sequence при принятии манифеста) — осознанный fail-closed, задокументировать.

---

## БАГ-6. Без трея Hide убивает резидентный процесс — S (<1 дня)

**Подтверждение (хуже описанного).** `hide_or_quit` (src/app.rs:328-335) завершает процесс без трея на: Esc, кнопку закрытия, **потерю фокуса** (клик мимо попапа!), OS close-request, и на выбор клипа в copy-only режиме — при этом при доступной автовставке тот же выбор клипа процесс не убивает. GUI прячет окно до обработки команды → «скрытие» и «смерть» визуально неотличимы, захват молча прекращается. Пути обратного вызова окна без трея существуют (хоткей, relaunch через single-instance) и уже обещаны текстами.

**Политика: Hide никогда не завершает процесс.** Выход — только явный `Quit` (попап-меню, трей).

**План (src/app.rs, ~40 строк).**
1. Удалить `can_hide_to_resident_surface`/`hide_or_quit`; close-request → всегда `CancelClose` + `request_hide` (флаг `quit_requested` пропускает явный Quit).
2. `AppCommand::Hide` → `request_hide`; унифицировать обе Ok-ветки `start_paste_flavors`.
3. Честная деградация: `ensure_tray` failure → пользовательский notice («иконка недоступна, vbuff продолжает работать — вызов хоткеем/перезапуском, выход из меню») + `publish_tray_capability(Unavailable)` на trust-поверхность; то же для сборки без фичи tray.
4. Док-комментарий `UiAction::Hide`: «runtime всегда скрывает; выход только через Quit».

**Тесты.** Замена (не дублирование) capability-записи; чистая функция `close_disposition(quit_requested)` — политика не зависит от трея; kittest: Esc/Close/фокус-лосс → `UiAction::Hide`; «Quit vbuff» → `UiAction::Quit`. Ручной чек-лист для Linux без AppIndicator.

**Риски.** «Зомби»-восприятие (процесс живёт невидимо) — гасится notice + capability-записью. Поведенческое изменение для привыкших к «Esc = выход» — changelog. Следом рекомендуется баг корректного graceful shutdown (Quit остаётся единственным выходом, join capture-thread/flush отсутствует).

---

## БАГ-7. RGBA w*h*4 переполнение — S (<1 дня) ⚠️ переоценено

**Подтверждение с корректировкой: это НЕ крэш-баг.** Уязвимый `thumbnail.rs:56-61` — **файл-сирота, не входит в сборку** (`mod thumbnail` нигде не объявлен). Живой путь `media.rs:29` уже защищён (`checked_mul` + лимит 64 МБ + тест на `usize::MAX`). Реальные проблемы: мёртвый файл с переполнением, готовый выстрелить при переподключении; три независимые копии парсера RGBA-MIME (types, media, image_fingerprint) при том, что recommendation.md:262 ложно утверждает консолидацию.

**План.**
1. **Удалить `thumbnail.rs`** (финальный grep перед коммитом).
2. `vbuff-types/src/rgba.rs`: `rgba_required_len(w,h) -> Option<usize>` (checked, нули → None), `parse_rgba_dims_checked(mime) -> Option<(w,h,len)>`, `is_rgba_mime` (регистронезависимый); re-export.
3. Перевести `media.rs`, `image_fingerprint.rs` на общие функции (удалить приватные копии); упростить `clipboard.rs::rgba_dimensions_match` через `rgba_required_len`. Локальные лимиты (64/128 МБ) остаются — политика, не контракт.

**Тесты.** usize::MAX dims → None; нули → None; регистр MIME; существующие тесты media/clipboard — регрессионные стражи, не трогаем.

---

## БАГ-8. Членство sync-групп не аутентифицировано — M (1–2 дня)

**Подтверждение по всем 4 пунктам.** Записи без подписи, `added_by` — голая строка (атакующий с любым активным устройством подпишет запись от чужого имени); SAS 6 цифр ≈ 19,9 бит; строгая монотонность часов + отсутствие мержа = невосстановимый форк; идентификаторы без валидации, `entries` безграничен. Потребителей API вне модуля нет — крейт frozen foundation, бласт-радиус минимален.

**План (membership.rs + 1 строка видимости в device_experience.rs).**
1. Per-entry подписи: `DeviceMember.signing_key` (ed25519), `MembershipEntry.signature`; подписываемые байты = preimage хеша, домен `vbuff-membership-entry-v2`; `append`/`revoke_and_rekey` принимают `SigningKey`; `verify` верифицирует каждую запись ключом автора из replay-набора.
2. SAS: 8 байт дайджеста → 20 цифр ≈ 66,4 бит, группами по 5 (`12345-67890-...`), домен `...-sas-v2`.
3. Модель писателя: **single-writer (owner-only)** — автор записей только устройство из записи 0; задокументировать явно (конкурентное мульти-писательство — осознанно не поддерживается до CRDT-решения). Устраняет форки по построению.
4. Лимиты: `valid_identifier` (`pub(crate)` из device_experience) ≤128 ASCII для `device_id`/`added_by`/`node_id`; `MAX_MEMBERSHIP_ENTRIES = 1024` fail-closed на запись и на чтение.

**Совместимость.** Формат меняется несовместимо, но персистентных/сетевых потребителей нет; домены бампнуты → старые данные fail-closed не десериализуются.

**Тесты (8 новых + 4 обновлённых).** Подделка автора с пересчётом хеша → Err; чужой ключ → Err, лог не мутирован; отозванный ключ отвергается; non-owner не авторует; SAS ≥ 60 бит и симметричен; валидация id (Unicode/пробелы/>128); граница 1024/1025; rekey только owner-ключом.

---

## БАГ-9. Молчаливая потеря данных в plan_snippet_mirror — S (<1 дня)

**Подтверждение.** (а) snippets.rs:115-119 — победитель при разных `content_hash` выбирается по `revision`, но ревизии — независимые счётчики, не каузальность: правка target стирается молча. (б) :113 — target-only сниппет планируется на `DeleteTarget` без конфликта. (в) `last_manifest_hash` объявлен, но нигде не используется; планировщик курсор вообще не принимает.

**План (один файл + реэкспорты).**
1. `SnippetSyncManifest { entries: BTreeMap<String, SnippetSyncedState> }`, где `SnippetSyncedState::{ Present{content_hash}, Deleted }` (tombstone); `compute_hash` (blake3, домен `vbuff-snippet-manifest-v1`), `validate` (≤10 000 записей).
2. Курсор += `#[serde(default)] manifest`; `trusted_manifest()` → Some только если hash сошёлся — иначе fail-closed «нет базы».
3. `plan_snippet_mirror(source, target, base: Option<&SnippetSyncManifest>)` — новая матрица: **`DeleteTarget` и `UpsertTarget` только при доказанной неизменности цели относительно базы; при любом сомнении — `Conflict`**. Target-only ключ → Conflict (не удаление); оба изменились → Conflict (revision больше не голосует); обратную синхронизацию (`UpsertSource`) осознанно не вводим.
4. Контракт обновления манифеста после применения плана (Upsert → `Present{hash}`, Delete → `Deleted`, Conflict — без изменений).

**Тесты.** Регрессионные на оба сценария потери (bug9a: обе правки + source.revision больше → Conflict; bug9b: target-only → Conflict); delete только по tombstone; safe-upsert при неизменной цели; недоверенный манифест → всё Conflict; serde старого курсора; ре-адд после tombstone → UpsertTarget.

**Риск.** Первый запуск без базы → много Conflict; осознанный fail-closed, шум исчезает после первого цикла — задокументировать.

---

## БАГ-10. Strict-режим и doctor структурно недостижимы — S (<1 дня)

**Подтверждение.** `SecurityPosture::detect` (capabilities.rs:46-151) безусловно зашивает не-Active уровни (encryption/hardware key/memory lock — всегда Unavailable, foreground_identity — максимум Degraded…) → `is_fully_protected()` всегда false → `doctor.ok` всегда false; strict-режим = навсегда мёртвый захват (паузу нельзя снять); бейдж в GUI вечно «Needs attention», attention-список 5–9 пунктов, включая принципиально недостижимые. Существующий тест `strict_mode_fails_closed_on_missing_encryption` кодирует баг как спецификацию.

**План.**
1. `CapabilitySeverity { RequiredForCapture, #[default] Informational }` + поле в `FeatureCapability`. Required — только наблюдаемо-исправимые `core_dumps` и `ptrace`; всё остальное Informational.
2. `strict_allows_capture` — фильтр по required; `is_fully_protected()` → `required_capabilities_satisfied()` + `failing_required()` для вывода.
3. `doctor.ok = store_ok && required_capabilities_satisfied()`; human-вывод печатает, что именно падает. JSON аддитивно (+severity).
4. `summarize_security_posture`: уровень только по required (Blocked/Protected/Partial), счётчики остаются глобальными (честный список пробелов сохраняется).
5. `vbuff-types`: `CapabilityView.severity` с `#[serde(default)]`; обновить struct-литералы (src/app.rs ×2, snapshot-фикстуры).
6. GUI trust_view: attention-список разбить на «Needs attention» (required) и «Known platform limitations» (informational) — убирает индикаторную усталость.

**Риски.** Главный — ошибочная классификация = fail-open: required-множество минимально и задокументировано. Поведенческий скачок для strict-пользователей (захват оживёт) и парсеров doctor (`ok` → true) — changelog. Снапшоты GUI реблесснуть с ручным просмотром diff.

---

## Порядок исполнения

| Волна | Баги | Обоснование |
|---|---|---|
| 1 (S, независимые) | **1, 4, 6, 7, 9, 10** | Каждый <1 дня, без миграций, без взаимных зависимостей. БАГ-1 и БАГ-6 — немедленный пользовательский эффект. |
| 2 (M) | **3, 5** | Приватность хранилища и ключи обновлений — единственные места, где дефект эксплуатируем/необратим уже сегодня. |
| 3 (M, проектные) | **2, 8** | БАГ-2 — структурный пререквизит для нативных бэкендов (три-стейты); координировать с решением о дефолтном deny-list. БАГ-8 делать до любого подключения sync-транспорта; предпочтительно после DeviceIdentity (plan.md:432), чтобы не менять фикстуры дважды. |

Суммарная оценка: ~4 S-дня + ~5 M-дней ≈ **1,5–2 недели** с тестами.

Сквозные правила для всех правок: fail-closed дефолты; честные doc-comments о деградации (evidence-based); changelog-записи о поведенческих изменениях (БАГ-1, 6, 10); при каждом закрытии — вычеркнуть/обновить пункты в code-audit-top-50.md и problems-improvements-top-500.md, чтобы аудит не гнил снова.
