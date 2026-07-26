# SOLID/DRY-ревизия vbuff — сводный отчёт

Дата: 2026-07-26. База: HEAD `eaa4d26` (после волн top-10). Метод: 8 зональных explore-агентов + сквозная DRY-ревизия; мёртвый код и устаревшие аудиты как находки не засчитывались, кроме случаев, где он создаёт двойной источник правды.

## Сквозные темы (дедуплицировано между зонами)

### T1. Три грамматики поиска, два движка — блокер «единого поиска»
- `core/recall/query.rs:148-180` (фасеты `app|kind|tag|device|before|after`, кавычки, относительные даты) vs `store/search.rs:38-50` (`host|color|lang|iso_date`, без кавычек) — словари не пересекаются, каждый парсер молча пожирает чужие фасеты.
- Исполнителей три: `core/filter.rs:25` (мёртв, doc-comment врёт), живой in-memory `gui/projection.rs:35`, спящий SQL `Store::search` (lib.rs:1521; потребитель — только `src/ask.rs`).
- Expiry-фильтр — 3 копии (`filter.rs:33-39`, `projection.rs:87-91`, `store/lib.rs:1551`), одна с carve-out `session_protected`; `workflow/everyday.rs:195-218` — мёртвый in-memory двойник той же концепции.
- Добавление фасета = правка ~8 точек в 3 файлах (query.rs:26-33/60-63/164-178/284-311, recall/search.rs:146-164/276-321, store/search.rs:44). Реальный баг уже есть: `app:` с `to_ascii_lowercase` (query.rs:190) не матчит не-ASCII `source_app`, нормализованный юникодно (recall/search.rs:121-126).
- **Решение:** единый query AST + табличный реестр `FacetSpec` в vbuff-core; store маппит AST→SQL; `store/search.rs::parse_query`, `SearchSession`, `filter::search`, `everyday::SessionProtection` удалить; `has_payment_number` — в грамматику или перестать писать. **L** (реестр+AST — M).

### T2. Валидация идентификаторов/меток: ~20 копий в 6 крейтах
Один предикат (`non-empty && len<=max && [A-Za-z0-9._-]`): core/trust ×4, ipc ×5+ (включая дубль в одном модульном дереве automation.rs:121/129 и access.rs:277, инвертированную полярность terminal.rs:152), sync, store/data_lifecycle.rs:1163, plugin ×6 (включая verbatim `valid_version` дважды в одном крейте), update — два `validate_key_id` в одном крейте. Парная `valid_label` — 4 копии. Блокер sync-транспорта: расхождение charset = reject-штормы между устройствами.
- **Решение:** `vbuff-types::validation::{is_valid_identifier, is_valid_label, valid_key_id, valid_version, all_zero}` (pure, serde-only). **M, механический.**

### T3. Кодек `ContentKind ↔ int`: 3-4 копии с разной семантикой ошибок
`store/lib.rs:3093-3116` (wildcard → `Other`, молчаливая деградация) vs `store/lifecycle.rs:342-369` (`Err(Corrupt)`); + `cas.rs:357-380` (`kind_slug`/`ALL_KINDS` — двойная правда в одном файле); + recall `kind_code`/`natural_kind`; + SQL-литералы `kind = 7` (lib.rs:652,668,786,799,1894,1909). Новый вариант компилятор в lib.rs не поймает (wildcard).
- **Решение:** `ContentKind::{to,from}_stored_discriminant` (fail-closed) в vbuff-types + `FromStr`; SQL параметризовать. **S.**

### T4. Enum-mirrors на границах крейтов (types уже в зависимостях)
`CapabilityLevel/Severity/FeatureCapability` ≡ `CapabilityView*` (ручные маппинги main.rs:220-241, app.rs:202-206; дрейф уже есть — serde default только у View); `PrivacyDecisionKind` ≡ `PrivacyDecisionLevel` (diagnostics.rs:176-178); `SelectionSource` ≡ `ClipboardSelection` (capture.rs:416-419); security-posture словарь разъехался между GUI-шапкой и Trust-поверхностью (app.rs:3079-3109 vs trust_view.rs:29-33).
- **Решение:** types владеет словарём, platform/core публикуют его напрямую или `impl From`; маппинги в бинарнике исчезают. **S–M.**

### T5. Крипто-примитивы скопированы вместо выделения общего слоя
- 6 HMAC-механизмов: построение MAC написано дважды (sign/verify) в каждом — webhook.rs:111-130, remote.rs:144-177, access.rs:234-264, callback.rs:229-268, api_token.rs:127-173, sync/capability.rs:31-86; фрейминг разъехался (`v1.` префикс не везде).
- 3 подписанных hash-chain (membership/provenance/ledger) с разошедшейся защитой: bound и валидация signer есть только у membership; подписи `[u8;64]` vs `Vec<u8>`.
- Вывод подписывающего ключа membership продублирован в append и verify (membership.rs:109-127 vs 187-205) — дрейф = обход подписи.
- 3 копии signing-preimage в update (manifest.rs:387-397, attestation.rs:79-89, manifest.rs:80-93) + плавающая конвенция NUL-терминатора доменов по workspace (9 билдеров).
- **Решение:** `hmac_proof(domain,key,parts)`; generic `SignedChain<E>` с хуком валидации и bound для всех; `expected_signing_key` — одна функция; `signing_preimage` в update + письменная доменная конвенция. **2×M + S.**
- Отдельно: callback replay-окно (`callback.rs:154`) не чистится никогда — единственное из четырёх; утечка в долгоживущем процессе. **S, фикс.**

### T6. Приватный домен: определение секрета и sensitive-инвариант размазаны
- Две OTP-эвристики (policy.rs:417-431 vs secret.rs:31-55) и три порога секретности: `secret_threshold` (policy), литерал `0.9` (store clawback lib.rs:2008-2010), без порога (plugin migration.rs:80).
- Sensitive-кламп (sync_eligible/ai_allowed/TTL) — 3 копии (lib.rs:1003-1015, data_lifecycle.rs:1226-1236, lib.rs:2014-2018); scrub-SQL — 2 verbatim-копии списка колонок (lib.rs:754-770, 2021-2030); `SecretHandling.{mask,sync_allowed}` никто не читает — ложная ручка.
- `CapturedClipboard::default()` — fail-open булевы `coherent_generation/primary_intended` (traits.rs:80-94): БАГ-2 закрыл concealment/provenance, эти два остались. Плюс два канала evidence (статический `evidence()` vs per-read поля) — нативный бэкенд легко рассинхронизирует.
- **Решение:** секретный домен целиком в secret.rs (OTP как SecretKind, общий `min_actionable_confidence`); `tighten_sensitive()` + `SENSITIVE_SCRUB_SET` в store; три-стейты для generation/intent; один канал evidence. **M + S + S + M.**

### T7. Мёртвые файлы-двойники с ложной правдой (удалить первым проходом)
- store: `row.rs`, `paths.rs`, `schema.rs` (не подключены; SCHEMA_VERSION=1 при живой v7; row.rs без metadata_json).
- gui: `render.rs`, `theme.rs`, `color.rs`, `input.rs` (~290 строк; color.rs хранит исправленный баг parse_hex_color; render.rs не компилируется — `UiAction::ClearAll`).
- src: `gui.rs`, `actions.rs`, `constants.rs` (actions.rs — paste-путь в обход ledger/sensitive/guard!).
- core: `trust/rules.rs` (коллизия имён `CaptureRule`/`PasteGuardDecision` с живыми), `security_audit.rs` (HMAC-цепочка без эмиттеров), `eviction.rs` (doc-comment enforce_cap врёт о parity-тесте; политики разошлись), `filter.rs`, trust/access.rs, хвосты posture/consent, `boards::ActionCandidate` (коллизия с живым в plugin).
- platform: `keymap.rs` (рассинхронизированная «каноническая» спецификация), `tripwire.rs` (несбыточный контракт), мёртвый алиас AutoPauseReason; коллизия `WrappedKey` (security.rs:33 vs sync/crypto.rs:14).
- **S суммарно, доходность на единицу риска — максимальная.**

### T8. Store: граница чтение/запись и проекции
- `count()` делает `purge_expired` DELETE (lib.rs:2598-2604) → цепочки из insert/enforce_cap/search_page; вооружённая ловушка для read-only открытия (doctor). **S.**
- 10 копий 11-колоночной проекции клипа + ручная гидрация (collect_clips внедрён лишь в 2 местах). **M.**
- 5 дивергентных путей удаления с разной семантикой legal-hold на краевых случаях (delete/apply_batch/delete_with_grace/audit_content_hashes без hold-проверки/массовые с 24 копиями предикатов). **M.**
- `Store` 74 публичных метода — НЕ дробить на трейты (школьничество; History-фасад уже сужает). Достаточно секций impl.

### T9. src/: двойные правды вокруг команд и конфига
- `AppCommand` (commands.rs) — дубль `UiAction` включая redacted Debug ×2: забудешь второй — утечка контента в логи. **M.**
- Config↔ShareableConfig: 6-7 точек маппинга на ключ; строково-типизированные enum-ключи ×3 (нет serde-derive на DensityMode/HandedMode/UnknownEvidencePolicy). **M + M.**
- Бэкенд захардкожен: `ArboardClipboard::new()` в capture.rs:194 и paste.rs:38 — прямой блокер нативных бэкендов. **M (factory).**
- Два определения security-вердикта (main.rs:243-272 vs doctor.rs:29-31); три детекта сессии с тремя источниками (lifecycle/capabilities/gui) — doctor и GUI уже могут разойтись. **M + S–M.**
- `paste_modifier` валидируется и персистит, но ни на что не влияет — лживая ручка (удалить из CONFIG_KEYS до оживления injection). **S.**

### T10. GUI: покадровые пересчёты и монолит
- Нет мемоизации: `filtered()`+токенизация near-dup, `clip_by_id`, `complete_query` ×2, `recent_source_apps`, `contextual_search_hint` — каждый кадр, принудительный repaint 1/с. `AppState::revision` уже есть, но не используется. → `ProjectionCache`. **M, до утяжеления поиска.**
- `PopupApp` 37 полей, `ui()` ~666 строк, 24-элементный tuple-снапшот, `render_row` 276 строк/13 аргументов. Пошагово: snapshot-struct → handle_history_keys → RowContext → target: Option<ClipId> в палитре (сейчас хак с подменой self.selected) → вынос поверхностей по образцу trust_view.rs. **Серия S/M.**
- Design-система игнорируется: 4 баннера копипастой, ad-hoc отступы/ширины/цвета мимо токенов, 6 формулировок одного sensitive-отказа, третий оттенок красного. **Серия S.**

### T11. Update-канал
`VerifierState` без `schema`-версии и integrity (state.rs:19-23) — добавить поле сейчас, пока формат молод; связка verify+store, чтобы долговечность не была opt-in. **S/M.**

## План волн

**Волна A — «Снять мины» (S, ~2-3 дня, поведение не меняется):**
1. Удалить все файлы-сироты (T7) — одним коммитом на крейт.
2. `vbuff-types::validation` + замена ~20 копий (T2); заодно `all_zero` ×15 инлайн.
3. Единый kind-кодек в types + параметризация SQL-литералов (T3).
4. `tighten_sensitive` + `SENSITIVE_SCRUB_SET` (T6-store); `count()` без DELETE, purge в idle-тик (T8).
5. callback replay-window retain (T5); `VerifierState.schema` (T11); signing-preimage в update + доменная конвенция (T5).
6. Удалить `paste_modifier` из CONFIG_KEYS; `SecurityPosture::level()` общий для main/doctor; enum-mirrors capability → types (T4, T9).

**Волна B — «Единые примитивы» (M, ~1-1.5 недели):**
1. Query AST + реестр FacetSpec + `normalize_lookup` (T1) — с goldens на обе стороны.
2. `SignedChain<E>` + `hmac_proof` + `ReplayGuard`, миграция 6 механизмов (T5).
3. Секретный домен в secret.rs, общие пороги (T6).
4. Единый `SessionContext::detect()` один раз в main (T9).
5. Config: serde-flatten ShareableConfig + serde-енумы (T9).
6. AppCommand → UiAction (T9); ClipboardBackend::write → WriteOptions (до первого нативного бэкенда).
7. Три-стейты для generation/intent в CapturedClipboard (T6).

**Волна C — «Структура» (L, по готовности):**
1. Решение по поисковым стекам (RFC: GUI на search_page или store-поиск внутренним) + миграция (T1).
2. GUI: ProjectionCache → расчленение PopupApp по шагам (T10).
3. Backend factory + WorkerState/poll_once — шаг (b) только при появлении второго бэкенда (T9).
4. Единый путь удаления + единая проекция клипа в store (T8) — перед sync-репликацией.

## Процессные правила против рецидива
- Новый enum на границе крейтов — сразу в vbuff-types.
- Новый идентификатор — через `types::validation`.
- Новый serde-контракт — `deny_unknown_fields` + осознанный `#[serde(default)]`.
- Чек-лист каждой волны: grep на mod-объявления новых файлов (сироты); sign/verify используют один MAC-билдер; redacted Debug — в одном месте на тип.
