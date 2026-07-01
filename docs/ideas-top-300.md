# vbuff - Ideas and Improvements Backlog, Items 198-300

This file covers ideas 198-300 of the 400-idea backlog. Companion lists: engineering ideas (1-113) live in [../architecture.md](../architecture.md), product/strategy ideas (114-197) live in [../recommendation.md](../recommendation.md), and extended ideas (301-400) live in [ideas-301-400.md](ideas-301-400.md). Effort tags are `S`/`M`/`L`.

The backlog is source material, not an automatic commitment. [../plan.md](../plan.md) remains the execution order; an idea only becomes planned work when it is pulled into a milestone.

---

## Power-User Workflows

198. **Live paste-stack scratchpad window** `[M]` - Add a small dockable panel showing the current paste queue as editable rows that can be reordered, deleted, duplicated, or pasted one by one. _Value: Makes batch paste work visible before the user commits, avoiding blind stack-pop mistakes._
199. **Form-fill capture mode** `[L]` - Record a sequence of copied field values as named slots, then replay them into a target form with Tab/paste steps. _Value: Gives non-programmers a deterministic form-filling workflow without a macro DSL._
200. **Structured merge templates** `[M]` - Let selected clips merge through templates such as bullet lists, numbered citations, CSV rows, or Markdown tables. _Value: Turns multi-clip composition into one action instead of manual separator cleanup._
201. **Two-clip diff and compare** `[M]` - Multi-select two text clips and open an inline line/word diff with copy-left, copy-right, and copy-merged actions. _Value: Developers and writers can compare copied variants without leaving the clipboard manager._
202. **Non-destructive transform overlay** `[M]` - Preview a transform result directly on the selected row while keeping the stored canonical bytes unchanged. _Value: Users can audition case, trim, JSON, or regex edits before paste without a modal._
203. **Conditional snippet fields** `[M]` - Add GUI-authored visibility rules so a snippet field can reveal or hide other fields based on a checkbox or dropdown. _Value: Covers real support and sales templates without introducing scripting._
204. **Computed snippet fields** `[M]` - Let one field derive from another through built-in transforms such as slugify, uppercase, date math, or character count. _Value: Removes repeated typing inside structured snippets._
205. **Visual quick-action pipeline builder** `[L]` - Compose quick actions as connected nodes rather than a serialized script list. _Value: Gives power users reusable pipelines while preserving the no-DSL positioning._
206. **Per-action sandbox permissions** `[L]` - Any shell or external action declares filesystem, network, timeout, and environment access, shown as a permission badge before use. _Value: Makes shareable actions safe enough for a privacy-sensitive tool._
207. **Dry-run and explain for quick actions** `[S]` - Holding a modifier opens a plain-language summary and diff of what the action will do, without writing or pasting. _Value: Builds trust before destructive or surprising transforms._
208. **Range selection with aggregate preview** `[S]` - Shift-select or drag over rows and show live aggregate size, count, and merged preview. _Value: Makes bulk copy/export/paste decisions inspectable._
209. **Spatial pin board grid** `[M]` - Arrange pins in a fixed grid mapped to number-row hotkeys. _Value: Gives frequent snippets muscle-memory recall similar to a small command pad._
210. **Context-aware boards** `[M]` - Auto-activate a pin board based on the focused app or window class. _Value: Surfaces SQL, shell, mail, or support snippets only where they are useful._
211. **Filter by example** `[S]` - Right-click a clip and create a filter matching its app, kind, source host, or tag set. _Value: Lets casual users build power filters without learning query syntax._
212. **Self-consuming queue board** `[M]` - A queue mode where each paste removes the item and advances a visible progress counter, with undo. _Value: Fits license-key, ID, and form-entry workflows better than static history._
213. **Capture-to-collector hotkey** `[S]` - Toggle a mode that appends the next N copies into one growing clip using a chosen joiner. _Value: Supports research and quote gathering without repeated merge cleanup._
214. **Destination-ranked action chooser** `[M]` - Rank quick actions based on the current target app and clip kind. _Value: Makes the likely operation one keystroke instead of an alphabetical hunt._
215. **Reversible transform history** `[L]` - Record paste-time transform chains against a clip so the user can revisit, tweak, and re-paste a prior transformed result. _Value: Keeps canonical bytes immutable while making transformations iterative._
216. **Session basket** `[M]` - Let users drag clips into a temporary basket that persists until cleared or promoted to a collection. _Value: Supports short-lived project work without permanent organization overhead._
217. **Clip checklist mode** `[S]` - Mark clips as todo/done inside a working set, with paste optionally checking the item off. _Value: Helps users process copied tasks, IDs, or migration steps without another app._
218. **One-shot named slots** `[M]` - Bind temporary slots such as `A`, `B`, and `C` to clips and paste them by slot key. _Value: Gives predictable recall for short workflows where recency order keeps shifting._
219. **Clipboard timeline scrubber** `[M]` - Add a time-axis scrubber that jumps by hour/day/session instead of only search and scroll. _Value: Makes "what did I copy yesterday afternoon" fast without exact keywords._
220. **Work-session grouping** `[L]` - Group clips by active app, time window, and project metadata into collapsible sessions. _Value: Reduces firehose fatigue while preserving the raw timeline._

## UI, UX, and Accessibility

221. **Velocity-aware row virtualization** `[M]` - During fast scrolling, draw cheap text rows first and defer thumbnails or syntax highlighting until velocity drops. _Value: Keeps the popup responsive on huge image/code histories._
222. **Type-to-paste digit overlay** `[S]` - Show transient 1-9 and optional home-row labels on visible results, relabeling as the filter changes. _Value: Makes quick-pick discoverable and keeps recall on the keyboard._
223. **Inline near-duplicate delta** `[M]` - Collapse near-duplicates and show the latest row with a subtle inline diff of what changed. _Value: Declutters edit/re-copy workflows without losing variants._
224. **Confidence-shaded match highlights** `[S]` - Vary highlight strength by fuzzy-match confidence rather than binary bolding. _Value: Helps users understand why a result ranked first._
225. **Screen-reader paste confirmation** `[S]` - Announce a successful paste through an accessibility live region when a screen reader is active. _Value: Makes paste-back observable after the popup closes._
226. **Caret-anchored popup** `[M]` - Position the popup near the text caret when the OS exposes caret bounds, falling back to cursor placement. _Value: Makes the UI feel attached to the insertion point._
227. **Reduced-motion crossfade** `[S]` - Replace slide/scale animation with a short opacity transition when reduced motion is active. _Value: Preserves orientation without vestibular-triggering movement._
228. **Recency tinting** `[S]` - Subtly tint rows by freshness while preserving contrast. _Value: Lets users sense age without a dense timestamp column._
229. **Sensitive-row peek mode** `[M]` - Blur or mask sensitive clips until hover, focus, or a deliberate peek key. _Value: Protects against shoulder-surfing while keeping useful temporary secrets available._
230. **Live transform preview rail** `[L]` - Show a small strip of context actions under the selected row with before/after previews. _Value: Makes transformations discoverable at the point of paste._
231. **First-run hotkey coachmark** `[S]` - Show a tray-anchored hotkey hint until the user successfully opens the popup once. _Value: Solves the common "installed but never learned the summon key" problem._
232. **Keyboard action flyout** `[M]` - A row-level flyout with mnemonic letters for pin, delete, transform, tag, and preview. _Value: Teaches shortcuts while keeping the flow keyboard-first._
233. **Adaptive density mode** `[M]` - Auto-select compact or comfortable row density based on monitor size, DPI, and visible work area. _Value: Makes the same app feel right on laptop and large monitor setups._
234. **Color swatch format ring** `[M]` - Render color clips as swatches with selectable hex/rgb/hsl output and a contrast hint. _Value: Turns passive color detection into a useful designer workflow._
235. **Focus-loss grace period** `[S]` - Briefly dim and countdown before dismissing the popup on focus loss, allowing immediate re-focus. _Value: Prevents accidental loss while switching context._
236. **Contextual empty-search hints** `[S]` - Rotate restrained hints in the empty search field based on unused capabilities. _Value: Teaches power features without a tour._
237. **High-contrast self-audit badge** `[M]` - Validate the resolved theme/accent colors against contrast rules at runtime when forced-colors mode is active. _Value: Prevents user customizations from silently breaking readability._
238. **RTL/CJK sample gallery** `[S]` - Include a built-in preview page with multilingual sample clips and rendering diagnostics. _Value: Makes international text support testable by users and maintainers._
239. **Pointer-free settings navigation** `[S]` - Ensure every settings control has a predictable tab order, mnemonic, and screen-reader label. _Value: Avoids a polished popup with an inaccessible settings surface._
240. **Motion budget inspector** `[S]` - Expose an internal overlay showing animation durations, repaint causes, and dropped frames. _Value: Makes UI polish measurable during development._
241. **One-handed mode** `[S]` - Optional keybindings biased toward one side of the keyboard for users who keep one hand on the mouse or trackpad. _Value: Improves ergonomics without changing default shortcuts._
242. **Large-preview side pane** `[M]` - Toggle a side preview for images, long text, HTML, and diff views without leaving the popup. _Value: Lets users inspect ambiguous clips before pasting._
243. **Clip confidence badges** `[M]` - Show badges such as lossless, partial, sensitive, local-only, synced, or degraded directly on rows. _Value: Makes invisible state visible where decisions happen._
244. **Per-row undo affordance** `[S]` - After pin/delete/tag/paste-stack actions, show an inline undo slot for a short window. _Value: Makes rapid keyboard operations less scary._
245. **Command palette for vbuff itself** `[M]` - Add an internal command palette for settings, diagnostics, transforms, and view switches. _Value: Keeps deep functionality discoverable without cluttering the popup._

## Everyday Quality-of-Life

246. **Near-duplicate text dedup** `[M]` - Add a normalized fingerprint for whitespace, wrapping, and punctuation variants while keeping exact bytes. _Value: Reduces clutter from cosmetic re-copies._
247. **Dedup merge ledger** `[S]` - Track how often and when a deduped clip was re-copied. _Value: Turns repeated copies into a frecency signal._
248. **Suggested pins** `[M]` - Propose pinning clips that cross a reuse threshold. _Value: Helps users curate without manual housekeeping._
249. **Encrypted grace-bin** `[M]` - Keep recently deleted or evicted clips recoverable for a short encrypted window. _Value: Makes accidental deletion reversible without abandoning secure-delete._
250. **Retention by content kind** `[M]` - Let images, code, links, secrets, and snippets have different default retention policies. _Value: Keeps valuable text longer while limiting bloat and secret exposure._
251. **Idle and lock auto-pause** `[M]` - Pause capture after user idle, screen lock, or remote-control detection. _Value: Reduces noise and accidental capture when the owner is absent._
252. **Rule suggestions from behavior** `[M]` - Suggest per-app capture/paste rules based on repeated manual actions. _Value: Turns advanced policy into guided setup._
253. **Actionable capture-health notifications** `[S]` - Notify once when capture stalls, permissions disappear, or compositor support degrades, with a direct fix link. _Value: Converts silent loss into a fixable event._
254. **Kind jump keys** `[S]` - With an empty query, single keys jump to URLs, images, code, files, colors, or snippets. _Value: Faster than typed prefix filters for common slices._
255. **Sticky filter scope with clear escape** `[S]` - Reopen in the last-used scope while showing a prominent reset action. _Value: Saves repeated filtering without creating confusing empty views._
256. **First-run default profiles** `[S]` - Offer Casual, Developer, and Privacy-max presets for retention and exclusions. _Value: Gives better defaults without forcing a settings tour._
257. **Weekly clipboard health digest** `[M]` - Show an optional in-app digest of DB size, largest clips, expiring clips, suggested pins, and skipped secrets. _Value: Makes maintenance a tiny habit rather than a hidden chore._
258. **Copy-burst grouping** `[L]` - Group several rapid distinct clips from the same source into an expandable burst. _Value: Keeps list order useful during spreadsheet and research workflows._
259. **Temporary protect lock** `[S]` - Mark a clip as protected from expiry, eviction, near-dup merge, and clear-on-lock for the current session. _Value: Lets users trust aggressive privacy defaults without losing one important value._
260. **Calendar-aware expiry labels** `[S]` - Show "expires tonight", "kept 7d", or "permanent" labels on rows when retention applies. _Value: Makes retention predictable instead of magical._
261. **One-click plain-text clone** `[S]` - Create a plain-text-only clone from a rich clip while preserving the original. _Value: Helps users prepare safe snippets for terminals and config files._
262. **Domain clean-link memory** `[M]` - Remember when a user repeatedly strips tracking params for a domain and offer to make it automatic. _Value: Converts repeated cleanup into a rule._
263. **Clipboard size budget alerts** `[S]` - Warn before storing or syncing a very large payload with an option to keep preview-only. _Value: Prevents accidental screenshot/video bloat._
264. **Recent-app filter chips** `[S]` - Show chips for apps that produced recent clips. _Value: App-based filtering becomes one tap/keystroke instead of query syntax._
265. **Pinned item review mode** `[S]` - Periodically show stale pins and ask whether to keep, archive, or delete. _Value: Keeps the permanent bank clean over months._

## Sync and Device Experience

266. **Device trust levels** `[M]` - Mark paired devices as full-trust, receive-only, send-only, or untrusted until verified again. _Value: Supports phones, work machines, and shared devices without one all-or-nothing sync graph._
267. **Local-only default for sensitive clips** `[M]` - Treat secret-detected clips as local-only unless explicitly overridden. _Value: Prevents sync from widening the blast radius of a capture mistake._
268. **Pairing rehearsal mode** `[S]` - Let users simulate device pairing and see what would sync without transferring real clips. _Value: Makes the sync model understandable before trust is granted._
269. **Selective device replay** `[M]` - Replay only selected clips or collections to a newly paired device. _Value: Avoids dumping years of history onto a temporary machine._
270. **Device revoke with tombstone propagation** `[M]` - Revoking a device emits encrypted tombstones for clips that should be wiped from it. _Value: Gives revocation practical teeth rather than only blocking future sync._
271. **Conflict timeline view** `[M]` - Show how a clip changed across devices and why one version won. _Value: Makes sync conflict resolution explainable._
272. **Offline sync outbox** `[S]` - Show clips waiting to sync, target devices, retry state, and last error. _Value: Turns "why is it not on my laptop" into inspectable state._
273. **Bandwidth saver mode** `[S]` - Sync metadata and thumbnails first, fetching large payloads on demand. _Value: Makes mobile/hotspot sync usable._
274. **Device-specific retention overrides** `[M]` - Keep long history on desktop but short, masked history on laptops or phones. _Value: Matches different risk profiles per device._
275. **Travel mode** `[M]` - Temporarily disables sync, hides sensitive collections, and shortens retention until turned off. _Value: Gives users a one-switch privacy posture for borders, conferences, or shared workspaces._
276. **QR handoff for one clip** `[S]` - Display a locally generated QR payload or pairing token for a selected clip. _Value: Provides a network-free escape hatch for small transfers._
277. **Nearby device send sheet** `[M]` - Show verified nearby devices as direct targets for one clip. _Value: Makes vbuff feel like cross-platform AirDrop for text and small payloads._
278. **Sync dry-run estimator** `[S]` - Before enabling a collection, estimate item count, bytes, sensitive count, and devices affected. _Value: Prevents surprising first syncs._
279. **Per-device clipboard write policy** `[M]` - Allow a paired device to receive history but not write directly into the live clipboard. _Value: Reduces surprise and abuse on secondary machines._
280. **Shared snippet library approvals** `[L]` - Require review before a team snippet update reaches everyone. _Value: Prevents accidental or malicious shared-text changes._

## Integrations and Extensibility

281. **Browser source-origin bridge** `[M]` - Browser extensions report origin, private-tab state, and selected link metadata to the local daemon. _Value: Fills the source-attribution gap that OS clipboards cannot expose._
282. **Clean-link context menu** `[S]` - Browser extension adds "Copy clean link to vbuff" using the URL-strip transform at copy time. _Value: Keeps noisy URLs out of history from the start._
283. **Editor language-aware paste** `[M]` - VS Code/JetBrains plugins re-indent code clips and manage Markdown fences based on destination context. _Value: Solves a daily developer paste annoyance._
284. **Editor capture metadata** `[M]` - Editor plugins tag clips with language, file path, repo, and branch. _Value: Makes developer history searchable by real project context._
285. **Vim/Neovim register bridge** `[M]` - Mirror vbuff history into a virtual register namespace and fuzzy picker. _Value: Makes ephemeral editor yanks durable and searchable._
286. **tmux and SSH local-paste bridge** `[L]` - Pull local vbuff clips into remote shells over an existing SSH channel. _Value: Avoids brittle OSC52 forwarding and remote clipboard hacks._
287. **Shortcuts and Tasker actions** `[M]` - Provide native actions for Get Clip, Add Clip, Paste Clip, and Send to Device. _Value: Makes vbuff usable in mainstream automation without scripting._
288. **Scoped webhook and local API tokens** `[M]` - Issue per-integration tokens restricted by tag, content kind, and secret policy. _Value: Keeps automation from becoming full-history exfiltration._
289. **Share-sheet tag prompt** `[S]` - Ingest from OS share sheets through a quick destination/tag dialog. _Value: Keeps deliberate saved items organized instead of dumping them into recency._
290. **Text-expander bridge** `[M]` - Keep a vbuff collection mirrored to Espanso or TextExpander format. _Value: Lets users migrate gradually instead of replacing everything at once._
291. **Read-only MCP server scoped by tag** `[M]` - Expose only clips tagged as AI-shareable through a read-only MCP server. _Value: Gives agents useful memory without opening the whole clipboard._
292. **Import adapter SDK** `[M]` - Let community plugins implement import/export adapters for other clipboard managers and snippet formats. _Value: Expands migration coverage without bloating core._
293. **Launcher extension ranking API** `[S]` - Expose a ranking endpoint optimized for Raycast, Alfred, rofi, dmenu, and fzf. _Value: Makes external pickers feel native instead of flat._
294. **Terminal OSC52 policy bridge** `[M]` - Detect OSC52 clipboard writes from terminals and attach remote-host/session metadata. _Value: Makes remote-origin clips auditable and filterable._
295. **Webhook replay protection** `[S]` - Sign outbound webhook events with monotonic ids and replay windows. _Value: Makes automation reliable without leaking clipboard content._

## Operations, Governance, and Release Quality

296. **Public limitation ledger** `[S]` - Maintain a versioned page listing every known platform limitation and its current workaround. _Value: Turns sharp edges into honest expectations._
297. **Release evidence bundle** `[M]` - Attach test matrix results, canary-grep output, dependency audit, SBOM, and benchmark summaries to every release. _Value: Makes trust claims inspectable by users and security teams._
298. **Config migration preview** `[S]` - Before changing config schema, show what will be rewritten and keep a rollback copy. _Value: Prevents settings upgrades from feeling like data loss._
299. **Maintainer handoff playbook** `[S]` - Document release keys, emergency patch flow, dependency update cadence, and sunset policy. _Value: Reduces abandonment risk for a sensitive daily-driver tool._
300. **Quarterly scope pruning review** `[S]` - Re-rank the full backlog every quarter and explicitly cut or defer anything that threatens the current milestone. _Value: Keeps the catalog from quietly eating the product plan while preserving the imagination that made it useful._
