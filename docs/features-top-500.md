# vbuff - Top 500 Clipboard Manager Features

> A catalog of **640** concrete features distilled from a competitive analysis of macOS, Windows, Linux and cloud clipboard managers (Ditto, CopyQ, Maccy, Paste, Pastebot, ClipboardFusion, Raycast, Alfred, GPaste, Klipper, and more). Each feature carries a priority tier: `MVP` (first usable release), `v1` (competitive 1.0), `v2` (advanced), `future` (differentiator).

## Table of contents

1. [Capture & monitoring](#1-capture-monitoring) (33)
2. [History & storage](#2-history-storage) (39)
3. [Search & filtering](#3-search-filtering) (33)
4. [Paste mechanics & output](#4-paste-mechanics-output) (36)
5. [Data types & formats](#5-data-types-formats) (37)
6. [Organization (pins, tags, folders, favorites, collections)](#6-organization-pins-tags-folders-favorites-collections-) (37)
7. [Snippets, templates & text expansion](#7-snippets-templates-text-expansion) (38)
8. [Transformations & quick actions](#8-transformations-quick-actions) (40)
9. [Sync & multi-device](#9-sync-multi-device) (32)
10. [Security & privacy](#10-security-privacy) (40)
11. [UI / UX / theming / window behavior](#11-ui-ux-theming-window-behavior) (40)
12. [Hotkeys, input & navigation](#12-hotkeys-input-navigation) (33)
13. [Integrations, scripting & extensibility](#13-integrations-scripting-extensibility) (37)
14. [Settings, profiles & management](#14-settings-profiles-management) (32)
15. [Platform-specific capabilities (mac/win/linux)](#15-platform-specific-capabilities-mac-win-linux-) (48)
16. [Performance, reliability & data integrity](#16-performance-reliability-data-integrity) (30)
17. [Accessibility & internationalization](#17-accessibility-internationalization) (27)
18. [Collaboration, sharing & team](#18-collaboration-sharing-team) (28)

**Priority breakdown:** 🟢 MVP: 133 · 🔵 v1: 230 · 🟣 v2: 213 · ⚪ future: 64

---

## 1. Capture & monitoring

1. **Monitor clipboard for changes** `[MVP]` - Run a background watcher that detects every system clipboard change and captures the new content into history automatically. _(seen in: Ditto, CopyQ, clipcat, Windows 11 built-in Clipboard History (Win+V))_
2. **Capture plain text** `[MVP]` - Record copied plain-text content as a history entry with full Unicode support. _(seen in: Ditto, CopyQ, GPaste)_
3. **Capture rich text and HTML** `[MVP]` - Store the RTF and HTML representations of a copy so formatting, links, and styles are preserved for later paste. _(seen in: Ditto, ClipAngel, CopyQ, CrossPaste)_
4. **Capture images and bitmaps** `[MVP]` - Detect and store copied images or screenshots as image clips rather than dropping them. _(seen in: Ditto, CopyQ, GNOME Clipboard Indicator, Paste)_
5. **Capture copied files and folders** `[v1]` - Capture file and directory references copied in the OS file manager so the paths can be pasted back as real file objects. _(seen in: Ditto, ArsClip, Comfort Clipboard Pro, Alfred)_
6. **Capture all formats per copy simultaneously** `[v1]` - When a single copy offers several representations at once (e.g. HTML plus plain text plus image), retain every flavor under one history item instead of only the first. _(seen in: Comfort Clipboard Pro, CrossPaste)_
7. **Capture arbitrary custom MIME formats** `[v2]` - Preserve non-standard or application-specific clipboard MIME/pasteboard types byte-for-byte so specialized app payloads round-trip correctly. _(seen in: Ditto, CopyQ)_
8. **Preserve content byte-for-byte** `[v1]` - Store captured content without altering whitespace, newlines, or encoding so editor selections (Vim wordwise/linewise/block) and exact byte payloads are retained. _(seen in: cliphist)_
9. **Tag each clip with its source application** `[v1]` - Record the name (and icon) of the app that owned focus at copy time and attach it to the history entry for display and filtering. _(seen in: Greenclip)_
10. **Record clip metadata at capture** `[v1]` - Stamp every captured item with timestamp, byte size, detected content type, and source window title for sorting and later inspection.
11. **Pause and resume monitoring** `[MVP]` - Provide a one-click toggle to stop tracking the clipboard temporarily and resume capture later without quitting the app. _(seen in: GPaste, GNOME Clipboard Indicator)_
12. **Private incognito mode** `[v1]` - Enter a temporary mode where copies are not recorded at all, for handling sensitive data during a focused session. _(seen in: GNOME Clipboard Indicator, PasteBar)_
13. **Exclude specific applications from capture** `[MVP]` - Maintain an app blacklist so copies originating from chosen applications (password managers, banking apps) are never recorded. _(seen in: ClipboardFusion, CopyQ, Greenclip, Paste)_
14. **Skip password and secure input fields** `[v1]` - Detect OS concealed/secure-entry clipboard flags and password fields and automatically refuse to capture content from them. _(seen in: Paste, GPaste)_
15. **Ignore content matching patterns** `[v1]` - Let the user define text or regex rules so copies whose content matches (e.g. card numbers, secrets) are excluded from history. _(seen in: CopyQ, Clipman (xfce4-clipman-plugin))_
16. **Mark transient secrets as concealed** `[v2]` - Honor concealed-secret clipboard hints by either skipping capture or storing a masked placeholder so sensitive values are not retained in plain history. _(seen in: GPaste)_
17. **Trim and skip whitespace-only copies** `[v1]` - Optionally drop or trim entries that are empty or contain only whitespace so noise never enters history. _(seen in: Parcellite / ClipIt)_
18. **Deduplicate repeated copies** `[MVP]` - Detect when newly copied content is identical to a recent entry and move/refresh it instead of creating a duplicate. _(seen in: cliphist, CopyQ)_
19. **Merge growing selection edits** `[v2]` - Recognize when a new copy is an extension of the immediately previous one and replace the prior entry rather than adding a near-duplicate. _(seen in: GPaste)_
20. **Enforce per-item capture size limit** `[v1]` - Reject or truncate copies above a configurable byte threshold so a single huge clip cannot bloat storage or memory. _(seen in: cliphist)_
21. **Set minimum capture length** `[v2]` - Ignore copies shorter than a configurable length (e.g. single characters) to avoid filling history with trivial fragments. _(seen in: cliphist)_
22. **Manual capture-on-demand hotkey** `[v1]` - Offer a dedicated shortcut or tray action to snapshot the current clipboard into history explicitly, for use when automatic monitoring is paused. _(seen in: KDE Connect (clipboard sync))_
23. **Capture-on-select (PRIMARY) selection** `[v2]` - On platforms with a selection clipboard, optionally capture text the moment it is highlighted with the mouse, independent of an explicit copy. _(seen in: GPaste, Diodon, Parcellite / ClipIt, Greenclip)_
24. **Manage PRIMARY and CLIPBOARD separately or merged** `[v2]` - Let the user choose whether the X11/Wayland selection and the explicit clipboard are tracked as one stream or as independent sources. _(seen in: Clipman (xfce4-clipman-plugin), KDE Klipper, CopyQ)_
25. **Distinguish manual copy from auto-selection capture** `[v2]` - Tag and optionally apply different capture rules to explicit Ctrl+C copies versus mouse-selection captures. _(seen in: Clipman (xfce4-clipman-plugin))_
26. **Capture screenshots directly into history** `[v2]` - Trigger a built-in screen/region capture whose resulting image lands straight in clipboard history without a separate tool. _(seen in: Clipboard Master)_
27. **Detect and label content type on capture** `[v1]` - Classify each incoming clip (URL, email, color, code, number, file path) at capture time so it can be styled and filtered by kind. _(seen in: CrossPaste)_
28. **Capture color values as color clips** `[future]` - Recognize copied hex/rgb color strings and store them as distinct color-swatch items for design workflows. _(seen in: CrossPaste, Raycast)_
29. **Daemon captures clips after source app closes** `[v2]` - Keep a lightweight daemon holding clipboard ownership so copied content survives and is recorded even after the originating application quits. _(seen in: Parcellite / ClipIt, clipcat)_
30. **Capture-event API for scripts** `[future]` - Expose a notification/event hook that fires on each clipboard capture so external scripts can react to or monitor new clips. _(seen in: clipcat, Keyboard Maestro)_
31. **Per-app capture rules** `[v2]` - Define rules scoped to a source application that change capture behavior (e.g. always strip formatting, never store images, force plain text) when copying from that app. _(seen in: Comfort Clipboard Pro, PhraseExpress)_
32. **Autostart monitoring on login** `[MVP]` - Launch the capture daemon at system startup/login so clipboard history begins recording without manual intervention. _(seen in: Diodon, ClipIt)_
33. **Notify on capture** `[future]` - Optionally surface a subtle sound or notification when a new item is captured, with a quiet/silent default to avoid noise. _(seen in: Comfort Clipboard Pro)_

## 2. History & storage

34. **Persist history across reboots** `[MVP]` - Save the clipboard history to disk so the full list survives application restarts, OS reboots, and crashes. _(seen in: Ditto, KDE Klipper, CopyQ, Paste, PasteBar)_
35. **Embed an SQLite database engine** `[MVP]` - Store all history items and metadata in a single embedded SQLite database file for reliable, indexed, transactional local persistence. _(seen in: Ditto, cliphist)_
36. **Capped item count with FIFO eviction** `[MVP]` - Keep a configurable maximum number of items and automatically drop the oldest non-pinned entry once the cap is exceeded. _(seen in: Clipboard Master, Clibor, clipcat, cliphist, Windows 11 Clipboard History)_
37. **Configurable history size limit** `[MVP]` - Let the user set the maximum number of stored items via settings, from a small list up to many thousands. _(seen in: Diodon, Clipman, GPaste, Comfort Clipboard Pro)_
38. **Unlimited history mode** `[v1]` - Offer an opt-in mode with no item-count cap so nothing is ever evicted by quantity, bounded only by disk and size caps. _(seen in: Ditto, CopyQ, Paste, CrossPaste, PasteBar)_
39. **Most-recent-first ordering** `[MVP]` - Order the history list by recency so the newest copied item always appears at the top of the list. _(seen in: Windows 11 Clipboard History, GNOME Clipboard Indicator)_
40. **Move re-copied item to top** `[v1]` - When existing content is copied again, promote that entry to the most-recent position rather than creating a duplicate. _(seen in: Ditto, CopyQ)_
41. **Exact-content deduplication** `[MVP]` - Detect when newly copied content byte-for-byte matches an existing entry and update that entry instead of inserting a duplicate row. _(seen in: cliphist, CopyQ)_
42. **Growing-selection collapse** `[v2]` - When a new copy is a superset that extends the immediately previous entry, replace the prior entry instead of adding a separate one. _(seen in: GPaste)_
43. **Per-item size cap** `[v1]` - Reject or truncate any single clipboard item larger than a configurable byte limit to keep the store lean and responsive. _(seen in: cliphist, Clipt)_
44. **Total store size cap** `[v1]` - Enforce a configurable maximum on-disk size for the whole history, evicting oldest items once the database grows beyond it. _(seen in: cliphist, GPaste, CrossPaste)_
45. **Minimum-length capture threshold** `[v2]` - Skip storing entries shorter than a configurable minimum length to avoid clutter from trivial one- or two-character copies. _(seen in: cliphist)_
46. **Trim whitespace-only entries** `[v2]` - Optionally ignore or auto-discard entries that contain only whitespace so blank copies never pollute history. _(seen in: Parcellite / ClipIt)_
47. **Time-based retention expiry** `[v1]` - Automatically delete non-pinned items older than a configurable age (for example 7, 30, or 90 days).
48. **Scheduled auto-clear** `[v2]` - Wipe non-pinned history automatically on a recurring interval or at each boot for a clean-slate workflow. _(seen in: GNOME Clipboard Indicator)_
49. **Clear entire history** `[MVP]` - Provide a single action to wipe all non-pinned history items at once, with a confirmation prompt before destruction. _(seen in: cliphist, KDE Klipper)_
50. **Delete individual items** `[MVP]` - Let the user remove any single entry from the history list permanently. _(seen in: cliphist, GNOME Clipboard Indicator)_
51. **Clear-on-exit option** `[v2]` - Optionally purge the non-pinned history every time the application quits for a privacy-focused, ephemeral session. _(seen in: KDE Klipper)_
52. **Pinned items exempt from eviction** `[v1]` - Ensure pinned, favorited, and permanently saved items are never removed by count caps, size caps, expiry, or auto-clear. _(seen in: Windows 11 Clipboard History, Ditto, Raycast)_
53. **Per-item creation timestamp** `[MVP]` - Record and store the date and time each item was first captured as queryable metadata.
54. **Per-item last-used timestamp and paste count** `[v2]` - Track when each item was last pasted and how many times, enabling recency and frequency views. _(seen in: TextExpander, PhraseExpress)_
55. **Per-item source-application metadata** `[v1]` - Store the name (and icon) of the application that was focused when each item was copied, for later filtering and recall.
56. **Content-type and byte-size metadata** `[v1]` - Persist each item's detected content kind and stored byte size as metadata for sorting, filtering, and storage accounting. _(seen in: CrossPaste)_
57. **Content-hash column for fast dedup** `[v2]` - Store a hash of each item's content and index it so duplicate detection and lookups stay fast even with huge histories.
58. **Configurable storage location** `[v1]` - Let the user choose a custom directory or file path for the history database instead of the default app data folder. _(seen in: clipcat, cliphist, CopyQ)_
59. **Cross-platform default storage paths** `[MVP]` - Default the database to the correct per-OS app-data location (macOS Application Support, Windows AppData, Linux XDG data dir).
60. **Portable storage mode** `[v2]` - Support keeping the database alongside the executable so the whole app and its history can run from a USB drive without installation. _(seen in: Clibor, ArsClip, ClipX)_
61. **Spill large blobs to external files** `[v2]` - Store oversized images and files as separate blob files referenced by the database rather than inline rows to keep queries fast.
62. **Manual backup and restore** `[v1]` - Export the full history and settings to a single backup file and restore the application state from it on demand. _(seen in: Clibor, Comfort Clipboard Pro)_
63. **Scheduled automatic backups** `[v2]` - Periodically snapshot the database to a rotating set of timestamped backup files for disaster recovery.
64. **Crash and power-failure recovery** `[v1]` - Use write-ahead logging and atomic commits so the most recently captured items survive an unexpected crash or power loss. _(seen in: Comfort Clipboard Pro)_
65. **Export history to portable format** `[v1]` - Export selected items or the whole history to JSON or CSV for inspection, migration, or processing by other tools. _(seen in: ClipX)_
66. **Import history from file** `[v2]` - Import items from a previously exported file or from another clipboard manager's export to seed or merge into the store. _(seen in: PhraseExpress, TextExpander)_
67. **Archive instead of delete** `[future]` - Move evicted or aged-out items into a separate searchable archive store rather than discarding them outright.
68. **Database compaction and vacuum** `[v2]` - Reclaim disk space and defragment the store via a background or on-demand vacuum after large deletions.
69. **Schema migration on upgrade** `[v1]` - Automatically and safely migrate the on-disk database schema when the app updates so old histories keep working.
70. **Storage usage dashboard** `[future]` - Show how much disk the history consumes, broken down by content type and largest items, with quick prune actions.
71. **Restore session state on launch** `[MVP]` - Reload the complete ordered history and pinned items into memory at startup so the list is intact immediately on first open. _(seen in: CopyQ, Parcellite / ClipIt, Qlipper)_
72. **Pause and resume capture** `[v1]` - Provide a toggle to temporarily stop writing new items to history without losing existing stored data. _(seen in: GPaste, GNOME Clipboard Indicator)_

## 3. Search & filtering

73. **Search-as-you-type incremental filtering** `[MVP]` - Filter the history list live on every keystroke with sub-frame latency so results narrow instantly without pressing Enter. _(seen in: Ditto, CopyQ, GPaste, KDE Klipper, Parcellite / ClipIt, Windows 11 Clipboard History, Clibor)_
74. **Substring plain-text search** `[MVP]` - Match the typed query as a case-insensitive substring against each clip's text content as the baseline default search mode. _(seen in: Ditto, CopyQ, 1Clipboard, ClipAngel, Clipboard Master, ClipClip)_
75. **Fuzzy matching** `[v1]` - Tolerate typos, transpositions, and gaps so a query like 'recpt' still finds 'receipt', ranking by edit-distance closeness. _(seen in: Raycast, Alfred, Greenclip, clipcat)_
76. **Regex search** `[v1]` - Let power users enter a regular expression to match clips by pattern, with a toggle to switch between literal and regex interpretation. _(seen in: Ditto, GNOME Clipboard Indicator, Clibor)_
77. **Full-text indexed search** `[v1]` - Maintain a persistent full-text index (e.g. SQLite FTS5) so searches over tens of thousands of clips return in milliseconds regardless of history size. _(seen in: Paste, Ditto)_
78. **Live match highlighting** `[MVP]` - Visually highlight the matched substrings or fuzzy-matched characters inside each result row and preview so users see why an item matched. _(seen in: Raycast, Alfred)_
79. **No-match visual feedback** `[MVP]` - Signal an empty result set immediately by tinting the search field (e.g. red) or showing an inline 'no results' state instead of a silent blank list. _(seen in: Parcellite / ClipIt)_
80. **Filter by content type** `[MVP]` - Restrict results to a chosen clipboard type such as text, image, link, file, color, code, or rich text via one-click type chips. _(seen in: Paste, ClipAngel, Comfort Clipboard Pro, CrossPaste)_
81. **Filter by source application** `[v1]` - Scope the list to clips that were copied from a specific app (e.g. only items from a browser or terminal) using a captured source-app facet.
82. **Filter by date and time range** `[v1]` - Narrow history to clips captured within a relative window (today, last hour, this week) or an explicit from/to date range.
83. **Filter to pinned and favorites only** `[MVP]` - Toggle a view that shows only pinned, sticky, or starred clips so frequently reused items are reachable without scrolling. _(seen in: GNOME Clipboard Indicator, ClipAngel, 1Clipboard, Ditto)_
84. **Filter by tag or label** `[v1]` - Restrict results to clips carrying a chosen user tag, with multi-tag intersection and exclusion for precise narrowing. _(seen in: CopyQ, GNOME Clipboard Indicator)_
85. **Scope search to a collection or tab** `[v1]` - Limit a query to a single named tab, board, or folder so searches stay within the relevant project instead of the whole history. _(seen in: CopyQ, ClipClip, PasteBar)_
86. **Global vs scoped toggle** `[v2]` - Offer a one-keystroke switch between searching only the current scope and searching across every collection at once. _(seen in: CopyQ)_
87. **Search query prefix operators** `[v2]` - Support compact inline operators like type:image, app:Chrome, tag:work, and before:2026-01-01 typed directly into the search box to combine filters. _(seen in: Raycast, Alfred)_
88. **Boolean operators in queries** `[v2]` - Allow AND, OR, NOT and quoted exact phrases so users can express compound conditions in a single query string.
89. **Relevance ranking** `[v1]` - Order results by a composite score blending match quality, prefix/word-boundary hits, and field weighting rather than plain recency. _(seen in: Raycast, Alfred)_
90. **Frecency-weighted ranking** `[v2]` - Boost clips that are both recently and frequently pasted so a user's habitual items surface to the top of ambiguous searches. _(seen in: Raycast, Alfred)_
91. **Recency default ordering** `[MVP]` - Show the newest clips first when the query is empty, giving a predictable reverse-chronological baseline before any filtering. _(seen in: Windows 11 Clipboard History, GPaste, cliphist)_
92. **Saved searches** `[v2]` - Persist a named query plus its active filters so the user can re-run a frequent search like 'images from last week' with one click.
93. **Smart filters** `[v2]` - Provide auto-maintained virtual lists driven by saved filter rules (e.g. 'all URLs', 'all code snippets', 'large images') that update as new clips arrive.
94. **OCR text search inside images** `[future]` - Index text extracted from copied images and screenshots so queries match words that appear visually inside image clips. _(seen in: Paste, CrossPaste, ClipClip)_
95. **Semantic search** `[future]` - Match clips by meaning using local embeddings so a query like 'meeting link' finds a Zoom URL even without the literal words.
96. **Filter by detected data kind** `[v2]` - Offer smart facets that recognize structured content (URL, email, phone number, hex color, file path, code) and filter to just those. _(seen in: KDE Klipper)_
97. **Keyboard-driven result navigation** `[MVP]` - Move through filtered results with arrow keys, Vim-style j/k, and number hotkeys without leaving the search field. _(seen in: GNOME Clipboard Indicator, CopyQ, KDE Klipper, Greenclip)_
98. **External picker / finder integration** `[v2]` - Stream the searchable history to an external fuzzy finder (rofi, dmenu, fzf, fuzzel) so Linux users search via their preferred launcher. _(seen in: Greenclip, clipcat, cliphist, GPaste)_
99. **Filter by content size** `[future]` - Narrow results by byte size or character/line count so users can isolate large blobs or trim trivial one-word clips. _(seen in: cliphist)_
100. **Filter by clip status** `[v1]` - Distinguish and filter clips by lifecycle state such as used vs never-pasted, masked/secret, or pinned to refine the working set. _(seen in: ClipAngel, GPaste)_
101. **Exclude / negative filtering** `[v2]` - Let the user hide clips matching a type, app, or text pattern (e.g. exclude password-manager clips from results) as an inverse filter.
102. **Search field with active-filter chips** `[v1]` - Render currently applied filters as removable chips beside the query so the active scope is always visible and individually clearable. _(seen in: Paste)_
103. **Result count and stats** `[v2]` - Display the number of matches (and total scanned) live as the query narrows, giving immediate feedback on filter selectivity.
104. **Search history and suggestions** `[future]` - Remember recent queries and offer autocomplete suggestions for tags, apps, and operators as the user types in the search box.
105. **Diacritic and case-insensitive normalization** `[v1]` - Normalize accents, ligatures, and case during matching so 'cafe' finds 'café' and queries work across mixed-language history.

## 4. Paste mechanics & output

106. **Paste-back into previously focused app** `[MVP]` - Restore focus to the window that was active before the popup opened and inject the selected clip into it. _(seen in: Ditto, CopyQ, Paste, Windows 11 Clipboard History)_
107. **Paste on Enter key** `[MVP]` - Pressing Enter on the highlighted history item immediately pastes it into the focused app and closes the popup. _(seen in: Ditto, CopyQ)_
108. **Paste on double-click** `[MVP]` - Double-clicking an item in the history list pastes it into the previously focused window. _(seen in: Ditto, CopyQ, Windows 11 Clipboard History)_
109. **Paste by number key** `[MVP]` - Press a digit (1-9, 0) to instantly select and paste the correspondingly positioned item without arrow navigation. _(seen in: Alfred, Raycast)_
110. **Copy-to-clipboard-only mode** `[MVP]` - Select an item to place it on the system clipboard without auto-pasting, letting the user paste manually later. _(seen in: Ditto, Windows 11 Clipboard History)_
111. **Paste as plain text** `[MVP]` - Strip all fonts, colors, and rich formatting and paste only the raw Unicode text of the clip. _(seen in: Paste, Windows 11 Clipboard History, ClipboardFusion, ClipClip)_
112. **Force plain-text default with rich override** `[v1]` - A configurable global default (plain vs. rich) with a modifier key to invert the behavior on a per-paste basis.
113. **Paste keeping original formatting** `[MVP]` - Inject the clip preserving its captured rich text, HTML, or RTF representation when the target app supports it. _(seen in: Paste, Comfort Clipboard Pro)_
114. **Simulated keystroke paste fallback** `[v1]` - When Ctrl+V/Cmd+V injection fails, type the clip character-by-character as synthetic keystrokes so it lands in apps that block clipboard paste.
115. **Terminal-safe paste shortcut** `[v1]` - Detect terminal windows and send the terminal paste combo (e.g. Ctrl+Shift+V) instead of Ctrl+V to avoid breaking the shell. _(seen in: Ditto)_
116. **Paste stack collection** `[v1]` - Collect several clips into an ordered stack, then pop and paste them one at a time with repeated paste presses. _(seen in: Paste, Comfort Clipboard Pro)_
117. **FIFO sequential multi-paste** `[v1]` - Select multiple items and paste them all in copy/selection order in a single action. _(seen in: Clibor, Comfort Clipboard Pro)_
118. **LIFO reverse-order multi-paste** `[v2]` - Paste a batch of selected items in reverse order (most-recent first). _(seen in: Clibor)_
119. **Merge multiple items into one paste** `[v1]` - Concatenate several selected clips into a single block and paste the combined result at once. _(seen in: Clibor, Alfred)_
120. **Configurable merge separator** `[v2]` - Choose the joiner used when merging clips (newline, space, comma, tab, or custom delimiter).
121. **Paste-and-delete (consume) on paste** `[v1]` - Remove the item from history immediately after it is pasted so one-time secrets or queue items are consumed.
122. **Auto-advance queue paste** `[v2]` - After pasting the top queued clip, automatically promote the next one so successive paste hotkeys walk the queue. _(seen in: Keyboard Maestro)_
123. **Paste without leaving the keyboard** `[MVP]` - Full hotkey-driven flow (open, filter, navigate, paste) so a clip can be inserted without ever touching the mouse. _(seen in: Alfred, Raycast, CopyQ)_
124. **Type-to-filter then paste top match** `[v1]` - Begin typing to narrow the list and press Enter to paste the top-ranked match without arrow keys. _(seen in: Raycast, Alfred)_
125. **Drag-and-drop paste into target field** `[v1]` - Drag an item from the history list and drop it directly into any input field or document. _(seen in: Ditto, CopyQ, Comfort Clipboard Pro, CrossPaste)_
126. **Paste from tray menu** `[v1]` - Expose recent items in the system tray menu so a clip can be pasted without opening the full popup. _(seen in: CopyQ, Diodon)_
127. **Change-case transform on paste** `[v2]` - Apply UPPER, lower, Sentence case, Title Case, or CamelCase to the clip as it is pasted. _(seen in: ClipAngel, Ditto, Keyboard Maestro)_
128. **Trim and collapse whitespace on paste** `[v2]` - Strip leading/trailing whitespace and collapse internal runs of spaces or blank lines before injecting. _(seen in: Parcellite)_
129. **Find-and-replace transform before paste** `[v2]` - Run a literal or regex find/replace over the clip content and paste the transformed result. _(seen in: Clibor, ClipboardFusion, Keyboard Maestro)_
130. **Per-app paste rules** `[v2]` - Define rules that change paste behavior (plain vs rich, keystroke vs clipboard, transform) based on the destination application. _(seen in: Comfort Clipboard Pro, ClipboardFusion)_
131. **Restore clipboard after paste** `[v1]` - After auto-pasting a clip, restore the user's prior clipboard contents so the paste does not clobber what they had copied.
132. **Paste delay before injection** `[v2]` - Configurable millisecond delay between focus restore and key injection to accommodate slow or animation-heavy target apps.
133. **Paste image as file vs inline** `[v2]` - For image clips, choose whether to paste the bitmap inline or drop it as a temporary file reference into the target.
134. **Paste rich item as Markdown** `[future]` - Convert an HTML/rich clip to Markdown on the fly and paste the Markdown source. _(seen in: PasteBar)_
135. **Paste file paths as text or as file objects** `[v2]` - For copied file references, choose to paste the path strings or the actual file objects depending on the target. _(seen in: Ditto)_
136. **Append-to-existing-clip on copy** `[future]` - A hotkey that appends the next copy onto the current top clip instead of creating a new entry, building one paste from many selections. _(seen in: GPaste)_
137. **Numbered copy buffers / slots** `[v2]` - Bind several fixed slots (e.g. Cmd+1..9 to store, paste with another modifier) for instantly reusable named clipboards. _(seen in: Ditto, Keyboard Maestro)_
138. **Quick-paste palette overlay** `[future]` - A compact heads-up overlay near the caret listing top clips for one-keystroke insertion without the full window. _(seen in: PasteBar, Keyboard Maestro)_
139. **Strip tracking parameters from pasted URLs** `[future]` - Detect URL clips and optionally remove tracking query parameters (utm_, fbclid, etc.) before pasting.
140. **Paste with leading/trailing wrapper** `[future]` - Wrap the clip in configurable prefix/suffix text (quotes, code fences, bullet markers) as it is pasted.
141. **Confirm before bulk multi-paste** `[v2]` - Show a preview/confirmation of the assembled output before pasting a large merged or sequential batch.

## 5. Data types & formats

142. **Capture plain text** `[MVP]` - Capture and store plain UTF-8 text copied from any application as the baseline clip type. _(seen in: Ditto, CopyQ, Windows 11 Clipboard History, GPaste)_
143. **Capture rich text and RTF** `[MVP]` - Preserve copied rich text in RTF form so fonts, bold, italic, and styling survive a paste back into formatting-aware apps. _(seen in: Ditto, ClipAngel, ArsClip, CopyQ)_
144. **Capture HTML fragments** `[MVP]` - Store the HTML flavor of a clip (CF_HTML / text/html) so web and editor copies retain their markup structure. _(seen in: Ditto, CopyQ, Windows 11 Clipboard History, CrossPaste)_
145. **Capture images and bitmaps** `[MVP]` - Capture copied raster images (PNG, BMP, screenshots) and retain them as full-resolution image clips. _(seen in: Ditto, CopyQ, GNOME Clipboard Indicator, Paste)_
146. **Capture copied files and folder paths** `[MVP]` - Record file references (file URIs / paths) when files or folders are copied in the OS file manager so they can be pasted back as real files. _(seen in: Ditto, ArsClip, Comfort Clipboard Pro, Alfred)_
147. **Preserve all simultaneous clipboard flavors** `[MVP]` - Store every MIME flavor present on a single copy together (e.g. plain + RTF + HTML) so the richest representation is available at paste time. _(seen in: Comfort Clipboard Pro, CopyQ)_
148. **Capture arbitrary custom MIME formats** `[v1]` - Retain non-standard, application-specific clipboard MIME types byte-for-byte so specialized app data round-trips correctly. _(seen in: Ditto, CopyQ)_
149. **Byte-for-byte content preservation** `[v1]` - Store text exactly as copied, preserving whitespace, trailing newlines, and editor-specific selection modes without normalization. _(seen in: cliphist)_
150. **Full Unicode and emoji support** `[MVP]` - Correctly handle the full Unicode range including emoji, combining marks, and right-to-left scripts in storage, search, and display. _(seen in: ClipboardFusion, Clibor, ArsClip)_
151. **Auto-detect content type on copy** `[v1]` - Classify each new clip (text, URL, color, code, image, file, email, phone, number) at capture time to drive type-specific UI and actions. _(seen in: CrossPaste, Raycast, Paste)_
152. **Detect and tag URLs and links** `[v1]` - Recognize copied URLs as a distinct link type and surface the title, domain, and a clickable affordance. _(seen in: Raycast, CrossPaste, KDE Klipper)_
153. **Detect and preview color values** `[v1]` - Recognize hex, RGB, RGBA, HSL, and named colors and render a color swatch preview with the value. _(seen in: CrossPaste, Raycast)_
154. **Detect code snippets and highlight syntax** `[v1]` - Identify clips that are source code, infer the language, and render them with syntax highlighting in the preview. _(seen in: PasteBar, Raycast)_
155. **Detect Markdown and render preview** `[v2]` - Recognize Markdown text and offer a rendered preview alongside the raw source. _(seen in: PasteBar)_
156. **Image thumbnail previews in the list** `[MVP]` - Show a scaled thumbnail for image clips directly in the history list for fast visual scanning. _(seen in: Ditto, Paste, CrossPaste)_
157. **Distinct visual styling per content type** `[v1]` - Render each clip with a type-specific icon, accent color, and layout so users can scan the history by type at a glance. _(seen in: CrossPaste, Paste)_
158. **Rich per-type preview panel** `[v1]` - Provide an expanded preview tailored to the clip type: rendered HTML, full image, file icon and metadata, color swatch, or formatted code. _(seen in: Paste, ClipboardFusion, CrossPaste)_
159. **Paste as plain text (strip formatting)** `[MVP]` - Offer a one-action paste that discards RTF/HTML styling and inserts only the plain-text content. _(seen in: Windows 11 Clipboard History, Paste, ClipClip, ClipboardFusion)_
160. **Convert rich text to clean Markdown** `[v2]` - Transform a captured HTML or RTF clip into well-formed Markdown on paste for documentation and note-taking workflows.
161. **Convert Markdown to rich HTML** `[v2]` - Render a Markdown clip into styled HTML/RTF so it pastes with formatting into email and word processors.
162. **Image format conversion on paste** `[v2]` - Re-encode an image clip into a chosen format (PNG, JPEG, WebP) when pasting or exporting it.
163. **OCR text extraction from image clips** `[v2]` - Run optical character recognition on copied images and screenshots so their embedded text becomes selectable, searchable, and copyable. _(seen in: Paste, CrossPaste, ClipClip)_
164. **Search inside image text via OCR index** `[v2]` - Index OCR-extracted text so full-text search returns image clips that visually contain the query terms. _(seen in: Paste)_
165. **Copy image as Base64 or data URI** `[v2]` - Provide an action to convert an image clip into a Base64 string or data: URI for pasting into code or CSS.
166. **Decode Base64 / data URI to image** `[future]` - Detect a Base64 image or data URI in a text clip and offer to decode and preview it as an image.
167. **Extract dominant colors from images** `[future]` - Analyze an image clip and expose its palette so individual swatch colors can be copied as hex values.
168. **Generate QR code from clip content** `[v2]` - Render any text, URL, or color clip as a scannable QR code for quick transfer to a phone. _(seen in: KDE Klipper, Clipman (xfce4-clipman-plugin))_
169. **Detect emails and phone numbers** `[v2]` - Recognize email addresses and phone numbers in clips and offer type-aware actions like compose or call.
170. **Detect numeric and currency values** `[future]` - Identify numbers, currency amounts, and units in a clip to enable inline math, unit, and currency conversion actions.
171. **Preserve source format with original-format fallback** `[v1]` - Keep the original highest-fidelity flavor as the canonical store while exposing lower-fidelity variants, so paste can choose the best match for the target app. _(seen in: Comfort Clipboard Pro, CopyQ)_
172. **File metadata extraction for file clips** `[v1]` - For copied files, capture and display name, size, type, modified date, and an icon or preview without copying the full bytes. _(seen in: Alfred, Comfort Clipboard Pro)_
173. **Inline preview of file clips** `[v2]` - Render a small preview (thumbnail for images, icon for documents) for copied-file clips in the history. _(seen in: Paste)_
174. **SVG detection and dual preview** `[future]` - Recognize copied SVG markup, render it as a vector image preview, and allow pasting either the rendered image or the raw markup.
175. **Configurable per-type max stored size** `[v1]` - Let users set separate size caps for text, image, and file clips so large binary clips do not bloat the database. _(seen in: cliphist, GPaste)_
176. **Vectorize raster image clips** `[future]` - Convert a copied raster image into vector (SVG) output for design workflows directly from the clip.
177. **Rich link unfurl with preview metadata** `[future]` - Fetch a copied URL's title, favicon, and thumbnail to display a rich link card in the history.
178. **Detect file paths inside plain text** `[v2]` - Recognize absolute or relative filesystem paths embedded in text clips and offer to open, reveal, or paste them as file references.

## 6. Organization (pins, tags, folders, favorites, collections)

179. **Pin item to top** `[MVP]` - Mark any clipboard entry as pinned so it sticks to the top of the history list and survives history pruning, clears, and reboots. _(seen in: Ditto, Windows 11 Clipboard History, GNOME Clipboard Indicator, Raycast, CopyQ, Greenclip)_
180. **Star as favorite** `[MVP]` - Toggle a star/heart flag on an item to add it to a dedicated Favorites view separate from recency-based history. _(seen in: 1Clipboard, ClipAngel, Comfort Clipboard Pro)_
181. **Tag with labels** `[v1]` - Attach one or more free-text labels to an item so it can be grouped and filtered across the whole history. _(seen in: CopyQ, GNOME Clipboard Indicator)_
182. **Folders and collections** `[v1]` - Create named folders/collections and move saved clips into them for hierarchical, project-based organization. _(seen in: ClipClip, PasteBar, Paste, ClipboardFusion)_
183. **Nested subfolders** `[v2]` - Allow folders to contain other folders for a multi-level hierarchy of organized clips. _(seen in: PhraseExpress)_
184. **Named tabs** `[v1]` - Organize clips into multiple top-level named tabs that segment the history into independent lists. _(seen in: CopyQ, PasteBar)_
185. **Pinboards of quick clips** `[v1]` - Maintain named pinboards holding frequently reused clips for one-click recall, distinct from the rolling history. _(seen in: Paste, PasteBar)_
186. **Color-code items** `[v1]` - Assign a color swatch to an item or group so related clips are visually scannable at a glance in the list. _(seen in: Comfort Clipboard Pro, ArsClip)_
187. **Add note to item** `[v1]` - Attach a free-text memo to any clip to record context, purpose, or reminders that show in preview. _(seen in: CopyQ)_
188. **Custom display name** `[v1]` - Give an item a human-readable alias that replaces its raw content in the list, useful for hiding sensitive values. _(seen in: Comfort Clipboard Pro)_
189. **Manual drag reorder** `[v1]` - Drag items within a list, folder, or pinboard to set an explicit custom order independent of recency. _(seen in: CopyQ)_
190. **Persistent vs ephemeral history** `[MVP]` - Distinguish a permanent saved-clips store that never expires from a rolling ephemeral history that auto-prunes by age or count. _(seen in: Ditto, Greenclip)_
191. **Promote clip to permanent** `[MVP]` - Convert a transient history entry into a permanently saved clip in one action so it leaves the auto-pruning pool. _(seen in: ArsClip, Greenclip, Keyboard Maestro)_
192. **Smart auto-tag rules** `[v2]` - Define rules that automatically tag or file incoming clips based on content type, regex match, or source application.
193. **Saved searches as smart groups** `[v2]` - Save a search/filter query as a named smart group whose membership updates dynamically as matching clips arrive.
194. **Filter by content type** `[MVP]` - Narrow the view to a single content category such as text, image, link, file, or color for focused browsing. _(seen in: Paste, ClipAngel, CrossPaste)_
195. **Filter by status** `[v1]` - Show only items matching an organizational status such as pinned, favorited, tagged, or recently used. _(seen in: ClipAngel)_
196. **Numbered quick-slots** `[v1]` - Assign clips to numbered buffer slots (1-9) that paste directly via a slot-specific hotkey for muscle-memory recall. _(seen in: Ditto, Keyboard Maestro)_
197. **Named clipboard sets** `[v2]` - Create addressable named clipboards/clip sets beyond the system clipboard that you can fill and paste from by name. _(seen in: Keyboard Maestro, Ditto)_
198. **Paste-stack grouping** `[v1]` - Collect a chosen set of clips into an ordered stack so they can be pasted sequentially as a unit. _(seen in: Paste, Clibor)_
199. **Multi-select bulk organize** `[v1]` - Select multiple items at once to pin, tag, move to a folder, color, or delete them in a single batch action. _(seen in: Clibor, CopyQ)_
200. **Sort order controls** `[v1]` - Switch list sorting between most-recent, oldest, most-used, alphabetical, or manual custom order. _(seen in: Parcellite/ClipIt, Comfort Clipboard Pro)_
201. **Pin to ordered top region** `[v1]` - Keep pinned items in their own fixed top section that stays visually separated above the scrolling recent items. _(seen in: GNOME Clipboard Indicator, Windows 11 Clipboard History)_
202. **Per-collection passcode lock** `[v2]` - Protect a folder or collection with a PIN/passcode so its organized clips stay hidden until unlocked. _(seen in: PasteBar)_
203. **Favorites/pins sync across devices** `[v2]` - Synchronize pinned and favorited items across the user's paired devices so organization is consistent everywhere. _(seen in: ClipboardFusion, Windows 11 Clipboard History, Raycast)_
204. **Folder/collection icons** `[v2]` - Assign an emoji or icon to each folder or collection for faster visual identification in the sidebar.
205. **Pin expiry override** `[v1]` - Configure whether pinned items are exempt from the history size cap and time-based auto-clear, with per-pin override. _(seen in: Windows 11 Clipboard History)_
206. **Recently used favorites surfacing** `[v2]` - Track and surface the most frequently or recently pasted favorites at the top of the favorites view. _(seen in: PhraseExpress, TextExpander)_
207. **Move item between collections** `[v1]` - Relocate a saved clip from one folder/collection/tab to another via drag or context menu without re-copying. _(seen in: ClipClip, PasteBar)_
208. **Tag-based filter chips** `[v2]` - Display existing tags as clickable chips that toggle on to filter the list down to clips carrying those tags.
209. **Collection-scoped search** `[v1]` - Restrict text search to the currently selected folder, tab, or collection rather than the entire history. _(seen in: CopyQ, ClipClip)_
210. **Pin via drag to pinboard** `[v2]` - Drag a history item onto a pinboard or favorites area to pin it there in one gesture. _(seen in: Paste)_
211. **Auto-file by source app** `[future]` - Optionally route incoming clips into a collection automatically based on the application they were copied from.
212. **Smart duplicate-merge grouping** `[v1]` - Detect repeat copies of the same content and merge them into a single organized entry that bumps to top instead of cluttering the list. _(seen in: cliphist, GPaste)_
213. **Shareable collection export** `[future]` - Export a folder or collection of clips to a portable file others can import, for sharing reusable clip sets. _(seen in: CopyQ)_
214. **Pin-protected from clear-all** `[MVP]` - Exclude pinned and favorited items from the clear-history action so a wipe never destroys curated clips. _(seen in: Windows 11 Clipboard History, Ditto)_
215. **Favorite count badges on collections** `[v2]` - Show a live count of items contained in each folder, tab, or collection in the navigation sidebar.

## 7. Snippets, templates & text expansion

216. **Create saved snippets** `[MVP]` - Let users save reusable text fragments as named snippets stored in a local library separate from transient clipboard history. _(seen in: TextExpander, Raycast, Alfred, Espanso, aText, PhraseExpress)_
217. **Trigger expansion by abbreviation** `[MVP]` - Detect a typed abbreviation system-wide and replace it inline with the snippet body across virtually any application. _(seen in: TextExpander, Espanso, PhraseExpress, aText, Alfred)_
218. **Insert snippet via hotkey** `[MVP]` - Assign a global keyboard shortcut to a snippet so it can be inserted on demand without typing an abbreviation. _(seen in: PhraseExpress, Keyboard Maestro, Alfred)_
219. **Search snippets in popup** `[MVP]` - Open an inline search bar to find and insert a snippet by name or content without memorizing its trigger. _(seen in: TextExpander, Espanso, Raycast)_
220. **Organize snippets into folders** `[MVP]` - Group snippets into a multi-level folder hierarchy for browsing and management. _(seen in: PhraseExpress, TextExpander, ClipClip, aText)_
221. **Edit snippets in built-in editor** `[MVP]` - Create and modify snippet bodies in an integrated editor without leaving the app. _(seen in: TextExpander, Espanso, PhraseExpress)_
222. **Insert date and time placeholders** `[MVP]` - Embed dynamic date/time tokens that resolve to the current date or time at the moment of expansion. _(seen in: TextExpander, Espanso, Raycast, Alfred, aText, Clipboard Master)_
223. **Position cursor after expansion** `[v1]` - Place a caret placeholder in the snippet so the cursor lands at a defined point after the text is inserted. _(seen in: Espanso, Raycast, Alfred, TextExpander)_
224. **Insert clipboard contents into snippets** `[v1]` - Reference the current clipboard value as a token so it is injected into the expanded snippet. _(seen in: Espanso, Raycast, Alfred)_
225. **Fill-in placeholder fields** `[v1]` - Define editable placeholder fields inside a snippet that the user fills before the final text is inserted. _(seen in: TextExpander, Espanso, PhraseExpress, aText, PasteBar)_
226. **Pop-up fill-in forms** `[v1]` - Present a multi-field form dialog for complex snippets, collecting all inputs at once before expansion. _(seen in: TextExpander, Espanso, PhraseExpress)_
227. **Dropdown and optional form sections** `[v2]` - Support dropdown choice fields and toggleable optional sections within fill-in forms for branching snippet content. _(seen in: TextExpander, PhraseExpress)_
228. **Do date math in placeholders** `[v2]` - Compute relative dates (e.g. +3 days, next Monday) and format them within snippets for scheduling text. _(seen in: Espanso, TextExpander, PhraseExpress, aText)_
229. **Nest snippets within snippets** `[v2]` - Allow a snippet body to reference and inline other snippets so shared fragments stay in one place. _(seen in: TextExpander, aText)_
230. **Author rich-text snippets** `[v1]` - Store and insert formatted content with fonts, bold, color and links, not just plain text. _(seen in: TextExpander, aText, PhraseExpress)_
231. **Embed images and attachments in snippets** `[v2]` - Include images, logos, or file attachments in a snippet so they are pasted along with the text. _(seen in: TextExpander, aText, Espanso)_
232. **Insert random or choice variables** `[v2]` - Provide random, echo, and choice tokens that pick or repeat values at expansion time for varied output. _(seen in: Espanso, TextExpander)_
233. **Scope snippets per application** `[v2]` - Restrict or vary snippets and triggers based on the frontmost application so the same abbreviation behaves differently per app. _(seen in: Espanso, PhraseExpress, Keyboard Maestro)_
234. **Match triggers with regex** `[v2]` - Define abbreviations as regular expressions so a single rule expands a family of typed patterns. _(seen in: Espanso)_
235. **Control word boundaries and case propagation** `[v1]` - Configure whether a trigger fires only at word boundaries and whether the typed case (lower/Title/UPPER) propagates to the output. _(seen in: Espanso, TextExpander)_
236. **Insert emoji and special characters** `[v1]` - Provide a built-in catalog of emoji, symbols, and special characters insertable by trigger or picker. _(seen in: Espanso, Clipboard Master, aText)_
237. **Ship a built-in snippet library** `[v1]` - Include ready-made snippet packs such as greetings, salutations, HTML fragments, and common misspelling corrections. _(seen in: Clipboard Master, aText)_
238. **Auto-capitalize and auto-correct** `[v2]` - Automatically fix double-capital errors and capitalize new sentences while expanding, and apply spelling-correction snippets as you type. _(seen in: aText, PhraseExpress)_
239. **Run shell commands in snippets** `[v2]` - Execute a shell command at expansion time and inject its standard output into the snippet result. _(seen in: Espanso, PhraseExpress, Keyboard Maestro)_
240. **Run scripts for dynamic output** `[future]` - Invoke an external interpreter (Python, JavaScript, AppleScript) so snippets can compute their content programmatically. _(seen in: Espanso, TextExpander, aText, Alfred)_
241. **Suggest snippets from repeated typing** `[future]` - Detect text the user types frequently and proactively suggest turning it into a snippet. _(seen in: TextExpander)_
242. **Track snippet usage statistics** `[v2]` - Record how often each snippet is used and surface most-used and unused snippets for cleanup. _(seen in: TextExpander, PhraseExpress)_
243. **Share team snippet libraries** `[future]` - Publish snippet groups that teammates can subscribe to, with role-based permissions for editing. _(seen in: TextExpander, PhraseExpress, aText)_
244. **Subscribe to public snippet packs** `[future]` - Browse and install community-published snippet packages from a shared hub by URL or catalog. _(seen in: TextExpander, Espanso)_
245. **Import from other expanders** `[v2]` - Migrate existing snippets by importing from TextExpander, aText, PhraseExpress, Espanso YAML, and similar formats. _(seen in: PhraseExpress, aText)_
246. **Sync snippets via file or cloud folder** `[v1]` - Keep the snippet library in a git directory, network folder, or third-party cloud drive so it stays consistent across machines and platforms. _(seen in: aText, Espanso, PhraseExpress, Alfred, Raycast)_
247. **Promote a clip to a snippet** `[MVP]` - Convert any clipboard-history item directly into a saved, named, reusable snippet in one action. _(seen in: ClipClip, PasteBar)_
248. **Tag and color-code snippets** `[v2]` - Apply tags and colors to snippets so they can be filtered and visually scanned alongside folders. _(seen in: ClipClip, GNOME Clipboard Indicator)_
249. **Show conflict palette for ambiguous triggers** `[v2]` - When a typed string matches several snippets, present a quick chooser palette so the user picks the intended one. _(seen in: Keyboard Maestro)_
250. **Syntax-highlight code snippets** `[v2]` - Detect the language of code snippets and display them with syntax highlighting in the editor and previews. _(seen in: PasteBar)_
251. **AI-assisted snippet authoring** `[future]` - Use AI to draft, rewrite, shorten, expand, or translate snippet content from a prompt. _(seen in: TextExpander, PhraseExpress)_
252. **Toggle expansion per language or layout** `[future]` - Maintain parallel-language snippet sets and switch the active set while preserving the folder hierarchy. _(seen in: PhraseExpress)_
253. **Pause expansion globally** `[v1]` - Provide a quick toggle to temporarily disable all abbreviation expansion when typing literal text or sharing a screen. _(seen in: Espanso)_

## 8. Transformations & quick actions

254. **Change case (UPPER/lower/Title/Sentence)** `[MVP]` - Convert the clip's text to uppercase, lowercase, Title Case, or Sentence case with one action before pasting. _(seen in: ClipAngel, Ditto, Keyboard Maestro, Alfred)_
255. **camelCase / snake_case / kebab-case convert** `[v1]` - Reformat identifiers between programmer casing styles (camelCase, PascalCase, snake_case, kebab-case, CONSTANT_CASE) for code workflows. _(seen in: ClipAngel, Ditto)_
256. **Trim whitespace** `[MVP]` - Strip leading and trailing whitespace and optionally collapse internal runs of spaces and blank lines. _(seen in: Parcellite / ClipIt)_
257. **Strip / remove formatting** `[MVP]` - Drop RTF/HTML styling and keep only plain text so the clip pastes clean into any target. _(seen in: ClipboardFusion, Windows 11 Clipboard History, Paste, ClipClip)_
258. **Paste as plain text (one-shot)** `[MVP]` - Override the next paste to insert the unformatted text without permanently altering the stored clip. _(seen in: Windows 11 Clipboard History, Paste, ClipClip)_
259. **Find & replace (literal)** `[MVP]` - Perform a literal search-and-replace across the clip's text before pasting, with optional case sensitivity. _(seen in: Clibor, Keyboard Maestro)_
260. **Regex find & replace** `[v1]` - Apply a regular-expression substitution with capture-group references to transform the clip's content. _(seen in: ClipboardFusion, Clibor, Keyboard Maestro)_
261. **Base64 encode / decode** `[v1]` - Encode the clip to Base64 or decode a Base64 string back to its original text in one quick action. _(seen in: Keyboard Maestro)_
262. **URL encode / decode** `[v1]` - Percent-encode or decode text for safe use in URLs and query strings. _(seen in: Keyboard Maestro)_
263. **HTML entity encode / decode** `[v2]` - Escape or unescape HTML entities (&, <, ', etc.) for web-content handling.
264. **Hash the clip (MD5/SHA-1/SHA-256)** `[v2]` - Compute a cryptographic hash of the clip's text and replace or copy the resulting digest.
265. **Format / pretty-print JSON** `[v1]` - Validate and pretty-print JSON with consistent indentation, reporting parse errors inline. _(seen in: PasteBar)_
266. **Minify JSON** `[v2]` - Collapse JSON to a single compact line by stripping insignificant whitespace.
267. **Format XML / HTML** `[v2]` - Reindent and pretty-print XML or HTML markup for readability.
268. **Translate clip text** `[v2]` - Translate the clip into a chosen target language via an online translation service and paste the result. _(seen in: ClipClip, PhraseExpress, TextExpander)_
269. **Evaluate inline math expression** `[v2]` - Detect and compute an arithmetic expression in the clip and paste its numeric result. _(seen in: Alfred, Raycast, Keyboard Maestro)_
270. **Run shell command on clip** `[v1]` - Pipe the clip through a user shell command and capture its stdout as the transformed content (using a %s / stdin placeholder). _(seen in: Parcellite / ClipIt, Clipman (xfce4-clipman-plugin), KDE Klipper, CopyQ)_
271. **Run script on clip (Python/JS/etc.)** `[v2]` - Execute a user script in a chosen interpreter with the clip passed as input and use its output as the new clip. _(seen in: ClipboardFusion, CopyQ, Keyboard Maestro, Espanso)_
272. **Custom command library** `[v1]` - Let users define, name, and save reusable transformation commands that appear in the quick-action menu. _(seen in: CopyQ, Keyboard Maestro, KDE Klipper)_
273. **Auto-run actions on copy (rules)** `[v2]` - Trigger transformations automatically when newly copied content matches a regex or condition (e.g. scrub tracking params). _(seen in: ClipboardFusion, KDE Klipper, Clipman (xfce4-clipman-plugin), CopyQ)_
274. **Append / combine selected clips** `[v1]` - Concatenate two or more selected clips into one, with a configurable separator (newline, space, comma). _(seen in: Clibor, Paste)_
275. **Sort lines** `[v1]` - Sort the clip's lines alphabetically or numerically, ascending or descending, with optional case-insensitive ordering.
276. **Deduplicate lines** `[v1]` - Remove duplicate lines from the clip while preserving original order or alongside sorting.
277. **Reverse lines / text** `[v2]` - Reverse the order of lines, or reverse a single string of characters, in one action.
278. **Join / split lines** `[v2]` - Collapse multiple lines into one with a chosen joiner, or split a delimited string into separate lines.
279. **Remove blank / empty lines** `[v2]` - Delete empty and whitespace-only lines from the clip to compact list-style content.
280. **Count lines / words / characters** `[v2]` - Report and optionally copy line, word, and character counts for the clip's text.
281. **Number lines** `[future]` - Prefix each line with a sequential number using a configurable start value and separator.
282. **Wrap / prefix / suffix lines** `[v2]` - Add a chosen prefix, suffix, or surrounding wrapper to every line (e.g. quote, bullet, comment marker).
283. **Convert delimiters (CSV/TSV/etc.)** `[future]` - Swap field delimiters in tabular text, such as comma-to-tab or tab-to-comma, for spreadsheet interchange.
284. **Smart-quote / dash normalization** `[v2]` - Convert curly quotes, em/en dashes, and other typographic characters to plain ASCII equivalents (or back). _(seen in: ClipboardFusion)_
285. **Normalize / strip diacritics** `[future]` - Apply Unicode normalization and optionally remove accents to produce ASCII-friendly text.
286. **Extract matches (emails/URLs/numbers)** `[v2]` - Pull all occurrences of a pattern (emails, URLs, phone numbers, or a custom regex) out of the clip into a list.
287. **Generate QR code from clip** `[v2]` - Encode the clip's text or URL as a QR code image and copy or display it. _(seen in: KDE Klipper, Clipman (xfce4-clipman-plugin))_
288. **Quick-action menu / palette** `[MVP]` - Open a searchable popup of all applicable transformations for the selected clip, navigable by keyboard. _(seen in: Raycast, Alfred, CopyQ)_
289. **Apply transform chain (pipeline)** `[future]` - Compose multiple transformations into a saved ordered pipeline that runs as a single quick action. _(seen in: Keyboard Maestro, Alfred)_
290. **Bind hotkeys to transformations** `[v1]` - Assign global or in-popup keyboard shortcuts to individual transformations and saved commands for instant invocation. _(seen in: CopyQ, Keyboard Maestro, Raycast)_
291. **Preview transform result before paste** `[v2]` - Show a live before/after preview of a transformation so the user can confirm or cancel before it is applied or pasted.
292. **Open URL clip in browser** `[v2]` - Detect a URL in the clip and open it directly in the default or chosen browser as a quick action. _(seen in: KDE Klipper)_
293. **Timestamp / date math conversion** `[future]` - Convert between Unix epoch timestamps and human-readable dates, or insert relative dates, from a clip value. _(seen in: Espanso, PhraseExpress)_

## 9. Sync & multi-device

294. **Pair devices via discovery** `[MVP]` - Automatically discover other vbuff instances on the same LAN (via mDNS/Bonjour-style broadcast) and let the user pair them with a one-time confirmation, so devices find each other without manual IP entry. _(seen in: CrossPaste, KDE Connect, Ditto)_
295. **Sync over LAN peer-to-peer** `[MVP]` - Replicate clipboard items directly between paired devices over the local network with no third-party server or cloud relay involved. _(seen in: Ditto, CrossPaste, KDE Connect, Qlipper, ArsClip)_
296. **Encrypt sync traffic in transit** `[MVP]` - Protect every byte exchanged between paired devices with authenticated transport encryption (e.g. TLS/Noise) so clipboard contents cannot be intercepted on the wire. _(seen in: Ditto, KDE Connect, Apple Universal Clipboard, CrossPaste)_
297. **Manage device list** `[MVP]` - Show all paired devices with name, platform, online/offline status, and last-seen time, and let the user rename or remove any device from the trust set. _(seen in: ClipboardFusion, Clipt, KDE Connect)_
298. **Confirm pairing with code** `[MVP]` - Display a short numeric or QR verification code on both devices during pairing that the user must match, preventing a malicious device from silently joining the sync group. _(seen in: KDE Connect, CrossPaste)_
299. **Sync automatically on copy** `[MVP]` - Push each newly copied item to all paired/connected devices in the background the instant it is captured, with no manual action required. _(seen in: Microsoft Cloud Clipboard (Win+V), Apple Universal Clipboard, KDE Connect, ClipboardFusion)_
300. **Toggle automatic vs manual sync** `[v1]` - Offer a global setting to either sync every copy automatically or hold items locally until the user explicitly chooses to send them, mirroring Windows' auto/manual modes. _(seen in: Microsoft Cloud Clipboard (Win+V), KDE Connect)_
301. **Push a single item manually** `[v1]` - Provide a 'Send clipboard now' action (button and hotkey) that transmits only the current clipboard or a selected history item on demand. _(seen in: KDE Connect, Microsoft Cloud Clipboard (Win+V))_
302. **Send to a specific device** `[v1]` - Let the user right-click any history item and choose a single target device to push it to, instead of broadcasting to the whole group. _(seen in: KDE Connect, Clipt)_
303. **End-to-end encrypt synced items** `[v1]` - Encrypt clipboard payloads with keys held only on the user's paired devices so that even a relay or cloud backend stores nothing readable, with an optional opt-in toggle. _(seen in: CrossPaste, Apple Universal Clipboard)_
304. **Sync via optional cloud relay** `[v1]` - Offer an opt-in encrypted cloud relay so devices that are not on the same network (different sites, mobile data) can still exchange clips when LAN sync is impossible. _(seen in: ClipboardFusion, Microsoft Cloud Clipboard (Win+V), TextExpander, Raycast)_
305. **Sync through user's own cloud drive** `[v1]` - Let privacy-conscious users route sync through their personal Google Drive, iCloud Drive, Dropbox, or OneDrive folder as the transport backend, so no vendor server ever holds the data. _(seen in: 1Clipboard, Clipt, PhraseExpress, aText, Alfred)_
306. **Resolve sync conflicts** `[v1]` - Detect when the same item was edited or two clips arrive concurrently and merge or surface a chooser, with a deterministic last-writer-wins fallback to keep histories consistent.
307. **Select what content syncs** `[v1]` - Give per-content-type controls (text, images, files, rich text, colors) so the user can, for example, sync text everywhere but keep large images local. _(seen in: CrossPaste, Apple Universal Clipboard)_
308. **Exclude items from sync** `[v1]` - Let the user mark individual clips, or sources/apps, as local-only so sensitive content (passwords, secrets) never leaves the originating device even when sync is on. _(seen in: GPaste, Paste, PasteBar)_
309. **Sync pinned and favorite clips** `[v1]` - Keep pinned/favorited items mirrored across all devices so a clip pinned on the desktop is immediately available and pinned on the laptop and phone. _(seen in: Microsoft Cloud Clipboard (Win+V), ClipboardFusion, Paste)_
310. **Transfer whole files between devices** `[v2]` - Support copying a file on one machine and pasting the actual file (not just a path) on another, streaming the bytes across the link for moderately large files. _(seen in: Apple Universal Clipboard, KDE Connect, Clipt)_
311. **Cap synced item size** `[v2]` - Enforce a configurable maximum payload size for synced clips so huge images or files do not saturate the network, with the option to fall back to a transfer-on-demand link. _(seen in: Apple Universal Clipboard, Clipt)_
312. **Sync selectively by group or tab** `[v2]` - Let the user designate specific collections, tabs, or boards as 'synced' while keeping others purely local, giving fine-grained control beyond all-or-nothing. _(seen in: CopyQ, PasteBar, CrossPaste)_
313. **Pull device clipboard on demand** `[v2]` - Let one device request and fetch the live clipboard of another paired device in real time (a 'grab from that machine' action) rather than waiting for a push. _(seen in: KDE Connect)_
314. **Run a self-hosted sync server** `[v2]` - Ship an optional lightweight self-hostable relay/sync server so teams or power users can run their own always-on rendezvous point across networks without trusting any vendor cloud. _(seen in: CrossPaste, PhraseExpress)_
315. **Browse history from any device** `[v2]` - Make the full synced history searchable and pasteable from every paired device, so an item copied on the phone yesterday is findable on the desktop today. _(seen in: Paste, ClipboardFusion, 1Clipboard)_
316. **Access clipboard via web app** `[v2]` - Provide a signed-in web interface to view, search, copy, and push synced clips from any browser when the native app is not installed. _(seen in: ClipboardFusion)_
317. **Paste synced clips via browser extension** `[v2]` - Offer a companion browser extension that exposes the synced history for quick paste into web pages and can capture copies from the browser into the shared history. _(seen in: ClipboardFusion, Clipt)_
318. **Provide a mobile companion app** `[v2]` - Ship Android/iOS companion apps that participate in the same sync group, letting users copy on mobile and paste on desktop and vice versa. _(seen in: ClipboardFusion, CrossPaste, Paste, ArsClip)_
319. **Sync offline and reconcile later** `[v2]` - Queue clips created while a device is offline and replay them to peers once connectivity returns, keeping the history eventually consistent without data loss. _(seen in: 1Clipboard)_
320. **Sync mobile keyboard clips** `[v2]` - Integrate with an iOS/Android keyboard extension so clips synced to the phone can be pasted into any mobile app directly from the keyboard. _(seen in: Paste, ClipboardFusion)_
321. **Transfer by proximity** `[future]` - Use Bluetooth presence plus Wi-Fi to enable zero-config copy-here-paste-there only between nearby trusted devices, mirroring Apple's proximity handoff model. _(seen in: Apple Universal Clipboard)_
322. **Show per-item sync status** `[v2]` - Annotate each history entry with an indicator showing whether it is local-only, syncing, synced, or failed, plus which devices have received it.
323. **Audit and revoke device access** `[future]` - Maintain a sync activity log (what was sent where and when) and allow instantly revoking a lost or stolen device's keys to cut it out of the sync group.
324. **Cap mobile sync to recent items** `[future]` - Optionally limit what propagates to constrained mobile devices to the last N items or a storage budget, auto-expiring older synced clips to save phone storage. _(seen in: Clipt, Microsoft Cloud Clipboard (Win+V))_
325. **Share a sync group with a team** `[future]` - Let multiple users join a shared, permissioned sync group for collaboratively shared clips (e.g. a team snippet pool), distinct from a single user's personal device mesh. _(seen in: TextExpander, PhraseExpress)_

## 10. Security & privacy

326. **Encrypt history database at rest** `[MVP]` - Store the clipboard history database encrypted on disk so a stolen drive or stray file copy cannot reveal past clips. _(seen in: Ditto, Comfort Clipboard Pro, Clibor, CopyQ)_
327. **Detect and skip password fields** `[MVP]` - Recognize when a copy originates from a secure/password input and refuse to record it in history. _(seen in: Paste)_
328. **Honor concealed-clipboard markers** `[MVP]` - Respect OS sensitivity hints (macOS org.nspasteboard.ConcealedType, Windows ExcludeClipboardContentFromMonitorProcessing, Wayland sensitive flags) so password managers' clips are never stored.
329. **Maintain per-app exclusion list** `[MVP]` - Let users blacklist specific applications by name or path so anything copied from them is ignored entirely. _(seen in: ClipboardFusion, Comfort Clipboard Pro, CopyQ, Greenclip, GNOME Clipboard Indicator, Paste)_
330. **Toggle incognito/private capture mode** `[MVP]` - Provide a one-click pause that stops all clipboard recording until re-enabled, for handling sensitive material on demand. _(seen in: GPaste, GNOME Clipboard Indicator, 1Clipboard)_
331. **Auto-clear history on a timer** `[MVP]` - Automatically purge clips older than a user-set age (e.g. 1 hour, 24 hours) so sensitive data does not linger indefinitely. _(seen in: GNOME Clipboard Indicator)_
332. **Wipe entire history on demand** `[MVP]` - Offer a single command to delete all stored clips immediately, with a confirmation prompt to prevent accidents. _(seen in: cliphist, KDE Klipper, CopyQ)_
333. **Keep data fully local by default** `[MVP]` - Ship with no cloud account, no remote server, and no network calls so the user's clipboard never leaves the machine unless they opt in. _(seen in: Ditto, CrossPaste, PasteBar, Espanso)_
334. **Send zero telemetry** `[MVP]` - Collect no usage analytics, crash pings, or identifiers, and document this no-telemetry stance plainly. _(seen in: Ditto, Espanso)_
335. **Exclude content by regex/keyword rule** `[v1]` - Drop any clip whose text matches user-defined patterns (e.g. credit-card or API-key shapes) before it is ever stored. _(seen in: CopyQ, Greenclip)_
336. **Auto-detect and tag secrets** `[v1]` - Run built-in detectors for common secret formats (credit cards, SSNs, private keys, JWTs, OTP codes) and flag or auto-exclude matching clips.
337. **Set master password to unlock app** `[v1]` - Require a master password at launch to decrypt history and open the popup, deriving the encryption key from it. _(seen in: Comfort Clipboard Pro, Clibor, ClipboardFusion)_
338. **Lock with quick PIN** `[v1]` - Offer a short numeric PIN as a fast secondary unlock for the search popup without re-entering the full master password. _(seen in: PasteBar)_
339. **Auto-lock after idle timeout** `[v1]` - Re-lock the app and clear in-memory plaintext after a configurable period of inactivity or on screen lock.
340. **Exclude OTP and one-time codes** `[v1]` - Detect short numeric verification codes from messages/authenticators and skip storing them since they are single-use secrets.
341. **Cap retention of sensitive types** `[v1]` - Apply a shorter, separate auto-expiry to clips flagged sensitive than to ordinary clips, so secrets evaporate faster.
342. **Lock individual collections with passcode** `[v1]` - Let users mark certain folders/boards of saved clips as PIN-locked, requiring authentication to view or paste from them. _(seen in: PasteBar)_
343. **Confirm before clearing history** `[v1]` - Always show a confirmation dialog before any bulk delete or wipe so history is not lost by a misclick. _(seen in: CopyQ, Parcellite / ClipIt)_
344. **Clear clipboard contents on exit** `[v1]` - Optionally blank the live system clipboard and/or drop history when the app quits or the OS shuts down. _(seen in: KDE Klipper)_
345. **Store encryption key in OS keychain** `[v1]` - Persist the database key in the platform secret store (macOS Keychain, Windows DPAPI/Credential Manager, Linux Secret Service) instead of a plaintext file.
346. **Mask sensitive clips in the list** `[v1]` - Render flagged or password-typed clips as dots/asterisks in the popup, revealing the real text only on explicit action. _(seen in: GPaste)_
347. **Exclude private browsing windows** `[v1]` - Automatically suspend capture when copies come from a browser's incognito/private window where supported.
348. **Encrypt LAN sync end-to-end** `[v1]` - When peer-to-peer sync is enabled, encrypt clips end-to-end between paired devices so nothing is readable in transit. _(seen in: Ditto, CrossPaste, Apple Universal Clipboard, KDE Connect)_
349. **Strip metadata on capture** `[v2]` - Remove source-app identifiers, EXIF/location data from copied images, and other incidental metadata before storing a clip.
350. **Securely erase deleted clips** `[v2]` - Overwrite or VACUUM freed database pages so deleted clips cannot be recovered by forensic tools.
351. **Keep an access audit log** `[v2]` - Record a tamper-evident log of unlocks, exports, wipes, and failed authentication attempts for security review.
352. **Provide a panic-wipe hotkey** `[v2]` - Bind a global emergency shortcut that instantly clears history, blanks the clipboard, and locks the app in one keystroke.
353. **Unlock with platform biometrics** `[v2]` - Allow Touch ID, Windows Hello, or fingerprint unlock as an alternative to typing the master password or PIN.
354. **Export a GDPR data package** `[v2]` - Let the user export everything stored about them in a portable, human-readable format to satisfy data-access and portability rights.
355. **Run a privacy dashboard** `[v2]` - Surface a single panel showing what is being captured, current exclusions, retention settings, and storage size for transparency.
356. **Limit clipboard read to focused app** `[v2]` - On platforms that expose it, restrict background apps from silently reading the clipboard and notify the user when an app pastes/reads.
357. **Encrypt and password-protect backups** `[v2]` - When exporting or backing up history, produce an encrypted archive gated by a passphrase rather than plaintext. _(seen in: Clibor)_
358. **Honor transient (history-only-off) clips** `[v2]` - Support copying an item into the live clipboard while explicitly marking it never to be persisted to history (transient paste).
359. **Schedule recurring data retention purge** `[v2]` - Run a configurable scheduled job (e.g. nightly or on boot) that enforces retention limits and removes expired clips automatically. _(seen in: GNOME Clipboard Indicator, CrossPaste)_
360. **Suspend capture on screen share** `[future]` - Detect active screen-sharing or recording sessions and automatically pause the popup and capture to avoid leaking history on camera.
361. **Enforce hardware-backed key storage** `[future]` - Bind the encryption key to a TPM, Secure Enclave, or hardware security key so the database cannot be decrypted off the original machine.
362. **Apply per-clip self-destruct timers** `[future]` - Let users set an individual expiry on a specific clip so it auto-deletes after a chosen interval regardless of global retention.
363. **Detect clipboard hijacking** `[future]` - Watch for suspicious silent rewrites of the clipboard (e.g. crypto-address swapping malware) and alert the user when a clip is altered between copy and paste.
364. **Offer plausible-deniability hidden vault** `[future]` - Provide a decoy password that opens an innocuous history while a separate secret password unlocks the real protected vault.
365. **Enable enterprise security policy lockdown** `[future]` - Allow admins to enforce policies (mandatory encryption, forced exclusions, disabled sync/export, minimum retention) via a managed config that users cannot override.

## 11. UI / UX / theming / window behavior

366. **Show history popup near cursor** `[MVP]` - Spawn the searchable history window at the current mouse/caret location so the user's eyes and pointer stay in place when the hotkey fires. _(seen in: Ditto, ArsClip)_
367. **Provide system tray / menu-bar icon** `[MVP]` - Place a persistent tray icon (Windows/Linux) and menu-bar item (macOS) that opens history, exposes settings, and shows the app is running. _(seen in: Ditto, CopyQ, Diodon, Klipper)_
368. **Render scrollable vertical list view** `[MVP]` - Present clipboard entries as a single-column, recency-ordered scrollable list as the default browsing layout. _(seen in: Ditto, Windows 11 Clipboard History, CopyQ)_
369. **Support dark and light themes** `[MVP]` - Ship built-in dark and light color schemes the user can pick explicitly in settings. _(seen in: 1Clipboard, CopyQ, Raycast)_
370. **Follow OS theme automatically** `[v1]` - Default to an 'auto' mode that tracks the operating system's light/dark appearance and switches live when the system toggles. _(seen in: Paste)_
371. **Show inline content-type icons** `[MVP]` - Prefix each row with a small glyph indicating whether the clip is text, image, file, link, color, or rich text for fast visual scanning. _(seen in: CrossPaste, Paste)_
372. **Style rows distinctly per content type** `[v1]` - Apply distinct background tint or accent per content type (text, image, link, color, code) so types are distinguishable at a glance. _(seen in: CrossPaste, Paste)_
373. **Render image thumbnail previews** `[MVP]` - Display a scaled thumbnail for image clips directly inside the list row instead of a generic placeholder. _(seen in: Ditto, Paste)_
374. **Show full-content preview pane** `[v1]` - Reserve a side or bottom panel that renders the full, scrollable content of the currently highlighted clip. _(seen in: ClipboardFusion, ClipAngel)_
375. **Offer hover quick-look popout** `[v2]` - On hover or a quick-look key, pop a larger floating preview of the item without committing to selection, similar to macOS Quick Look. _(seen in: Paste, ClipboardFusion)_
376. **Provide horizontal carousel / grid layout** `[v2]` - Offer an alternate horizontal carousel or grid view of visual previews, well suited to image- and file-heavy histories. _(seen in: Paste)_
377. **Toggle compact vs comfortable density** `[v1]` - Let the user switch row density between a tight compact mode (more rows visible) and a comfortable mode with larger previews. _(seen in: Paste)_
378. **Add minimal compact bar mode** `[v2]` - Offer a slim always-summonable horizontal bar that floats over the active app for one-handed quick paste, distinct from the full window. _(seen in: Paste)_
379. **Choose UI font and size** `[v1]` - Let users select the list font family and adjust text size for readability and personal preference. _(seen in: ArsClip, Comfort Clipboard Pro)_
380. **Apply monospace font to code clips** `[v2]` - Render clips detected as code in a monospace font with proper whitespace so structure stays legible in previews. _(seen in: PasteBar)_
381. **Syntax-highlight code and Markdown previews** `[v2]` - Detect language and apply syntax highlighting to code snippets and Markdown in the preview pane. _(seen in: PasteBar)_
382. **Make the popup window resizable** `[v1]` - Allow the user to drag-resize the history window and remember the chosen dimensions across sessions. _(seen in: CopyQ, Comfort Clipboard Pro)_
383. **Remember window position and size** `[v1]` - Persist the popup's last position, size, and view mode so it reopens exactly as the user left it. _(seen in: CopyQ)_
384. **Support multi-monitor cursor placement** `[v1]` - Open the popup on the monitor containing the cursor and keep it fully on-screen across mixed DPI and resolutions.
385. **Clamp popup within screen work area** `[MVP]` - Reposition the window so it never opens under the taskbar/dock or off-screen near display edges.
386. **Scale crisply on HiDPI/Retina** `[v1]` - Render icons, thumbnails, and text sharply at fractional and high DPI scaling on all three platforms.
387. **Dismiss popup on focus loss / Escape** `[MVP]` - Auto-close the history window when it loses focus or the user presses Escape, behaving like a transient picker. _(seen in: Windows 11 Clipboard History, Raycast)_
388. **Pin popup open (non-transient mode)** `[v2]` - Provide a toggle to keep the window open and on top while the user works, instead of dismissing on focus loss. _(seen in: CopyQ)_
389. **Animate open/close with reduced-motion respect** `[v2]` - Use a subtle fade/slide animation when summoning and dismissing the popup, automatically disabled when the OS requests reduced motion. _(seen in: Paste, Raycast)_
390. **Show empty / no-results state** `[MVP]` - Render a friendly placeholder when history is empty or a search yields nothing, with a hint on how to populate or refine.
391. **Indicate live search match feedback** `[MVP]` - Visually signal match state as the user types (highlight matched substrings; flag no-match clearly). _(seen in: Parcellite, Clibor)_
392. **Visually mark pinned and favorite clips** `[MVP]` - Render pinned/favorite items with a distinct badge and group or float them at the top of the list. _(seen in: Ditto, Windows 11 Clipboard History, GNOME Clipboard Indicator)_
393. **Color-code and label clips in the UI** `[v2]` - Let users assign a color tag and custom display name to a clip, shown in the row to organize and to mask sensitive content visually. _(seen in: Comfort Clipboard Pro, Clibor)_
394. **Provide first-run onboarding tour** `[v1]` - Show a short guided walkthrough on first launch covering the global hotkey, paste-back, search, and pinning.
395. **Display hotkey hints / cheat-sheet overlay** `[v1]` - Surface contextual keyboard-shortcut hints in the footer and a toggleable full shortcut overlay inside the popup. _(seen in: Raycast)_
396. **Expose accent color / theme customization** `[v2]` - Let users pick an accent color and customize core UI colors beyond the bundled themes. _(seen in: ArsClip, Raycast, Alfred)_
397. **Support custom / importable themes** `[future]` - Allow loading and sharing of full custom theme files so the community can distribute color schemes. _(seen in: Alfred, Raycast, CopyQ)_
398. **Provide full keyboard navigation of the list** `[MVP]` - Make every UI action (move, select, paste, pin, delete, preview) reachable via the keyboard with no mouse required. _(seen in: GNOME Clipboard Indicator, Klipper, CopyQ)_
399. **Support screen readers and accessible roles** `[v1]` - Expose proper accessibility roles, labels, and focus order so screen readers (VoiceOver/Narrator/Orca) can announce list items and actions.
400. **Offer high-contrast and large-text modes** `[v2]` - Provide a high-contrast theme and a large-text option for low-vision users, honoring OS accessibility settings.
401. **Show numbered quick-pick shortcuts** `[v1]` - Label the first several visible rows with index numbers so the user can jump-paste an item by pressing its number. _(seen in: Clibor, ArsClip)_
402. **Display per-item metadata footer** `[v2]` - Show contextual metadata for the highlighted clip (source app, timestamp, size, character/word count) in a status footer. _(seen in: CrossPaste)_
403. **Provide a translucent / blurred background** `[future]` - Offer an optional translucent, OS-blurred window background (vibrancy/acrylic/Mica) for a native, modern look. _(seen in: Raycast, Paste)_
404. **Localize and mirror UI for RTL languages** `[v2]` - Translate the interface into multiple languages and correctly mirror layout for right-to-left locales. _(seen in: Clibor, Comfort Clipboard Pro)_
405. **Adapt tray icon to OS appearance** `[v1]` - Render a monochrome template tray/menu-bar icon that adapts to light/dark menu bars and shows a subtle state for paused/private mode. _(seen in: GPaste, Diodon)_

## 12. Hotkeys, input & navigation

406. **Global summon hotkey** `[MVP]` - A single system-wide hotkey (e.g. Ctrl+` / Cmd+Shift+V) that opens the searchable history popup from any application without focusing the app first. _(seen in: Ditto, Windows 11 Clipboard History, Paste, Raycast, CopyQ)_
407. **Rebind summon hotkey** `[MVP]` - Let the user record and change the global summon shortcut to any key combination through a preferences capture field. _(seen in: Ditto, ClipboardFusion, ArsClip, GPaste, CopyQ)_
408. **Cross-platform modifier normalization** `[v1]` - Present and store hotkeys using each OS's native modifier conventions (Cmd/Option on macOS, Ctrl/Win/Alt on Windows, Super/Ctrl on Linux) so one config reads correctly everywhere.
409. **Arrow-key list navigation** `[MVP]` - Move the selection up and down the history list with the arrow keys, with Home/End jumping to the first and last items. _(seen in: Windows 11 Clipboard History, KDE Klipper, GNOME Clipboard Indicator)_
410. **Enter to paste selected** `[MVP]` - Press Enter to paste the highlighted item into the previously focused window and dismiss the popup. _(seen in: Ditto, CopyQ, Paste)_
411. **Type-to-filter search** `[MVP]` - Start typing the moment the popup opens to incrementally filter the history list as-you-type, with no need to click into a search box. _(seen in: Ditto, Windows 11 Clipboard History, Parcellite, GPaste, Clibor)_
412. **Number quick-select** `[v1]` - Press a digit key (1-9, then 0) to instantly select and paste the corresponding item from the top of the list. _(seen in: Clibor, Keyboard Maestro)_
413. **Letter quick-select labels** `[v2]` - Assign a-z access keys shown next to items so a single keystroke jumps to and pastes a clip even beyond the first ten positions. _(seen in: Alfred)_
414. **Vim-style navigation keys** `[v2]` - Support j/k for down/up, g/G for top/bottom, and / to focus search for users who prefer modal, home-row navigation. _(seen in: CopyQ, cliphist, Greenclip)_
415. **Hotkey conflict detection** `[v1]` - Warn at bind time when a chosen shortcut is already registered by the OS or another running app, and block or flag the collision.
416. **Chord / two-stroke hotkeys** `[v2]` - Allow multi-step key sequences (a leader key followed by a second key, e.g. Ctrl+K then P) to expand the available shortcut space. _(seen in: Keyboard Maestro)_
417. **Per-action hotkeys** `[v1]` - Let users bind independent global shortcuts to discrete actions such as paste-as-plain-text, paste-last, open-pinned, or toggle-private-mode. _(seen in: ClipboardFusion, Keyboard Maestro, Raycast)_
418. **Paste-plain-text modifier** `[v1]` - Hold a modifier (e.g. Shift) while pressing Enter to paste the selected clip stripped of formatting in one motion. _(seen in: Windows 11 Clipboard History, Paste, Ditto)_
419. **Escape to dismiss** `[MVP]` - Press Esc to close the popup without pasting and restore focus to the originating window. _(seen in: Windows 11 Clipboard History, CopyQ)_
420. **Toggle-on-same-hotkey** `[v1]` - Pressing the summon hotkey again while the popup is open closes it, so one key both shows and hides the window. _(seen in: Windows 11 Clipboard History, Raycast)_
421. **Tab to cycle panes** `[v1]` - Use Tab/Shift+Tab to move keyboard focus between the search field, history list, pinned section, and preview pane.
422. **Keyboard multi-select range** `[v2]` - Extend the selection with Shift+arrows and toggle individual items with Ctrl/Cmd+Space to mark several clips for batch paste, all from the keyboard. _(seen in: Clibor, CopyQ, Paste)_
423. **Keyboard pin toggle** `[v1]` - Pin or unpin the highlighted item with a dedicated key (e.g. P or Ctrl+P) without leaving keyboard navigation. _(seen in: GNOME Clipboard Indicator, Raycast)_
424. **Keyboard delete item** `[v1]` - Remove the selected clip from history with Delete/Backspace, with selection advancing to the next item automatically. _(seen in: GNOME Clipboard Indicator, cliphist, CopyQ)_
425. **Search-input idle navigation** `[v1]` - While typing in the filter, let arrow keys still move list selection so users can refine and navigate without releasing the keyboard or moving to the list. _(seen in: Ditto, GPaste)_
426. **No-match input feedback** `[v2]` - Visibly signal a failed filter (e.g. tint the search field) when the typed query matches no items so the user knows immediately. _(seen in: Parcellite, ClipIt)_
427. **Type-ahead jump in list** `[v2]` - When focus is on the list rather than search, typing a letter jumps to the next item beginning with that character (incremental find).
428. **Sequential paste-stack hotkey** `[v2]` - A hotkey that pastes the next clip from a queued stack on each press, advancing through collected items in order (FIFO). _(seen in: Clibor, Paste, Keyboard Maestro)_
429. **Numbered buffer hotkeys** `[v2]` - Bind direct shortcuts to a set of fixed numbered clipboard slots (e.g. Ctrl+Alt+1..9) to copy into and paste from them without opening the list. _(seen in: Ditto, Keyboard Maestro)_
430. **Mouse-free preview toggle** `[v1]` - Open and close the full preview of the selected item with a key (e.g. Space) so long clips can be inspected without the mouse. _(seen in: GNOME Clipboard Indicator, ClipboardFusion)_
431. **Editable shortcut profiles** `[future]` - Offer preset keymap profiles (Default, Vim, Emacs-like) that remap navigation keys in bulk, plus a fully custom profile.
432. **In-app shortcut cheat-sheet** `[v2]` - A keyboard-summoned overlay (e.g. ?) listing all active shortcuts in the current context for discoverability. _(seen in: Raycast)_
433. **Focus-on-open behavior setting** `[v2]` - Let users choose where keyboard focus and the initial selection land when the popup opens (search field vs first list item, top vs most-recent).
434. **Tray menu keyboard access** `[v1]` - Expose the tray/menu-bar icon menu with full keyboard operability (open, navigate, activate entries) for users who reach it without the global hotkey. _(seen in: KDE Klipper, Diodon, Qlipper)_
435. **Hold-to-peek summon mode** `[future]` - An optional mode where holding the hotkey shows the list and releasing it pastes the highlighted item, enabling fast press-navigate-release pasting.
436. **Wraparound list scrolling** `[v2]` - Optionally wrap selection from the bottom of the list back to the top (and vice versa) when navigating past the ends with arrow or vim keys.
437. **Page-jump navigation** `[v1]` - Use PageUp/PageDown to move the selection by a full visible page through long history lists.
438. **Reserved-hotkey safe defaults** `[future]` - Ship default shortcuts chosen to avoid clashing with common OS and desktop-environment bindings, with a guided picker steering users away from reserved combos on rebind.

## 13. Integrations, scripting & extensibility

439. **Ship a first-class CLI** `[MVP]` - Provide a `vbuff` command-line binary to list, get, copy, add, search, and delete clips so the history can be driven from any terminal or script. _(seen in: CopyQ, GPaste, clipcat, cliphist)_
440. **Pipe stdin into history** `[MVP]` - Let `vbuff copy` read stdin (e.g. `cat file | vbuff copy`) and `vbuff paste`/`get` write a selected clip to stdout for shell pipelines. _(seen in: cliphist, GPaste, clipcat)_
441. **Run a daemon with a local control socket** `[MVP]` - Expose the background daemon over a Unix domain socket / named pipe so the CLI and external tools issue commands to the running instance with low latency. _(seen in: CopyQ, clipcat, GPaste)_
442. **Emit structured machine-readable output** `[v1]` - Add `--json` (and NUL-delimited) output modes to every CLI command so scripts can parse results without fragile text scraping. _(seen in: cliphist)_
443. **Integrate with external pickers** `[v1]` - Offer a pipe-friendly menu mode that feeds history into rofi, dmenu, wofi, fuzzel, fzf, and skim and pastes the chosen entry back. _(seen in: cliphist, clipcat, Greenclip, GPaste)_
444. **Register a custom URL scheme** `[v1]` - Handle a `vbuff://` URL scheme (search, paste-by-id, add-clip, open-settings) so links from notes, launchers, and browsers can drive the app.
445. **Run commands on matching clips** `[v1]` - Let users define rules that auto-run a shell command when a copied clip matches a regex or content type, passing the clip on stdin / as an argument. _(seen in: CopyQ, KDE Klipper, Clipman, Parcellite / ClipIt)_
446. **Build a custom-command library** `[v1]` - Provide a command editor where each entry binds a trigger (menu, hotkey, or clipboard-change) to an action, mirroring CopyQ's command dialog. _(seen in: CopyQ)_
447. **Embed a scripting engine** `[v2]` - Ship an embedded scripting runtime (e.g. Lua/Rhai/JS) with a documented API to transform, filter, route, and act on clips programmatically. _(seen in: CopyQ, ClipboardFusion, Keyboard Maestro)_
448. **Run user transform scripts on copy** `[v2]` - Allow per-clip transformation scripts (strip HTML, regex replace, normalize) that execute automatically the moment content is copied. _(seen in: ClipboardFusion, CopyQ)_
449. **Expose a plugin architecture** `[v2]` - Define a stable plugin interface (dynamic libraries or sidecar processes) so third parties can add content types, actions, and sources without forking. _(seen in: CopyQ, ClipX, Diodon, GNOME Clipboard Indicator)_
450. **Host a shareable extension store** `[future]` - Run a community hub where users browse, install, update, and publish plugins and command presets directly from the app. _(seen in: Raycast, Espanso, Alfred)_
451. **Provide a developer SDK** `[future]` - Publish a typed SDK plus scaffolding tooling so developers can build and test extensions against vbuff's API with minimal boilerplate. _(seen in: Raycast)_
452. **Fire outbound webhooks on events** `[v2]` - POST a JSON payload to user-configured URLs when a clip is copied, pinned, or matched, enabling automation in n8n, Zapier, and home servers.
453. **Serve an inbound local HTTP API** `[v2]` - Expose an authenticated localhost REST/RPC endpoint so local apps and scripts can query and push clips over HTTP. _(seen in: clipcat)_
454. **Offer a gRPC remote-control API** `[future]` - Provide a gRPC service for high-throughput programmatic access and remote control of the running daemon from native clients. _(seen in: clipcat)_
455. **Emit an event stream for scripts** `[v1]` - Provide a `vbuff watch` / notify command that streams clipboard-change events so external scripts can react in real time. _(seen in: clipcat, GPaste)_
456. **Support AppleScript on macOS** `[v1]` - Ship a scripting dictionary so AppleScript and JXA can read, add, and paste clips, integrating with macOS automation workflows. _(seen in: TextExpander, Keyboard Maestro, Alfred)_
457. **Support PowerShell on Windows** `[v1]` - Provide a PowerShell module with cmdlets (Get-VbuffClip, Add-VbuffClip, Invoke-VbuffPaste) for Windows scripting and automation. _(seen in: ClipboardFusion)_
458. **Expose a D-Bus interface on Linux** `[v1]` - Publish a D-Bus service so desktop environments, GNOME/KDE applets, and scripts can control vbuff using standard Linux IPC. _(seen in: GPaste, KDE Klipper)_
459. **Build a Raycast extension** `[v2]` - Publish an official Raycast extension to search, paste, and pin vbuff clips from the Raycast launcher. _(seen in: Raycast)_
460. **Build an Alfred workflow** `[v2]` - Ship a downloadable .alfredworkflow that surfaces vbuff history and actions inside Alfred on macOS. _(seen in: Alfred)_
461. **Integrate with Keyboard Maestro and macro tools** `[v2]` - Document and expose tokens/actions so Keyboard Maestro, Espanso, and similar macro engines can read vbuff clips and trigger its actions. _(seen in: Keyboard Maestro, Espanso, PhraseExpress)_
462. **Import from other clipboard managers** `[v1]` - One-click importers that read history and pinned items from Ditto, CopyQ, Paste, Maccy, ClipboardFusion, and Win+V into vbuff. _(seen in: PhraseExpress, ClipboardFusion)_
463. **Import snippets from text expanders** `[v2]` - Import snippet/phrase libraries from TextExpander, Espanso, aText, and PhraseExpress so existing snippets become vbuff entries. _(seen in: PhraseExpress, aText)_
464. **Export and import history as portable files** `[v1]` - Export the full history or selected collections to JSON/CSV and re-import them, enabling backups, migrations, and scripted manipulation. _(seen in: ClipX, Clibor)_
465. **Sync a collection with a watched directory** `[v2]` - Mirror a tab/collection to files in a folder so clips can be edited, version-controlled with git, or generated by external tools on disk. _(seen in: CopyQ, Alfred, aText)_
466. **Provide a native share/service extension** `[v2]` - Register a system share-sheet item (macOS Share Extension, Windows Share target, Linux portal) so any app can send content into vbuff.
467. **Register OS context-menu / Quick Actions** `[v2]` - Add right-click 'Send to vbuff' and macOS Services / Windows shell verbs so selected text and files flow into history from the desktop. _(seen in: Alfred)_
468. **Build a browser extension bridge** `[v2]` - Ship a browser extension that captures and pushes selections, links, and images to the local vbuff daemon and lets the page request clips. _(seen in: ClipboardFusion, Clipt (OnePlus / OneLab))_
469. **Ship an MCP server for AI agents** `[future]` - Expose clipboard history through a Model Context Protocol server so AI assistants and agents can search and reuse the user's clips.
470. **Generate shell completions and man pages** `[v1]` - Provide completion scripts for bash/zsh/fish/PowerShell and a generated man page so the CLI is discoverable and ergonomic.
471. **Manage config as a versionable file** `[v1]` - Store all settings, rules, and commands in a human-editable TOML/YAML file with hot-reload and `vbuff config` commands for headless setup. _(seen in: Greenclip, Espanso)_
472. **Run startup and shutdown hooks** `[v2]` - Let users register scripts that run on daemon start, stop, and history-clear for custom provisioning, logging, or cleanup. _(seen in: ClipboardFusion)_
473. **Provide event-based automation triggers** `[future]` - Trigger actions on non-clipboard events (timer, app focus change, screen lock/unlock, network change) to run scripts or toggle vbuff behavior. _(seen in: ClipboardFusion, Keyboard Maestro)_
474. **Expose named/addressable clipboard slots over the API** `[v2]` - Let scripts read and write persistent named buffers beyond the system clipboard via the CLI and API for staged automation workflows. _(seen in: Keyboard Maestro, Ditto)_
475. **Offer a portable headless/server mode** `[v2]` - Run vbuff as a no-GUI daemon configurable entirely via CLI, files, and API for remote, SSH, and CI environments. _(seen in: clipcat, GPaste, cliphist)_

## 14. Settings, profiles & management

476. **Build preferences window** `[MVP]` - Provide a dedicated, native multi-pane settings window (General, Appearance, Hotkeys, Privacy, Sync, Advanced) where all configuration is discoverable and editable. _(seen in: Ditto, CopyQ, Paste, Raycast, Alfred)_
477. **Launch at login** `[MVP]` - Offer a toggle to auto-start vbuff at OS login using each platform's native mechanism (LaunchAgent on macOS, Run registry/Startup on Windows, XDG autostart on Linux). _(seen in: Diodon, Comfort Clipboard Pro, ClipboardFusion)_
478. **Start minimized to tray** `[MVP]` - Let the app start hidden in the system tray/menu bar with no foreground window so it stays out of the way after login. _(seen in: Ditto, Diodon, Qlipper)_
479. **Set history retention limit** `[MVP]` - Expose user-configurable caps for maximum item count, maximum per-item size, and total database size from the preferences UI. _(seen in: Clibor, cliphist, GPaste, CrossPaste, clipcat)_
480. **Manage per-app exclusion list** `[v1]` - Give a settings panel to add applications whose clipboard activity vbuff should never capture, with a picker that lists installed/running apps. _(seen in: ClipboardFusion, Comfort Clipboard Pro, Greenclip, GNOME Clipboard Indicator, CopyQ)_
481. **Define per-app paste behavior rules** `[v2]` - Let users configure default paste mode per target application (e.g. always plain-text in terminals, keep formatting in editors) via a rules table in settings. _(seen in: Ditto, Comfort Clipboard Pro, Keyboard Maestro)_
482. **Customize global and in-app hotkeys** `[MVP]` - Provide a keyboard-shortcut editor where every action (open popup, paste-back, plain-text paste, clear, pause) can be rebound with live conflict detection. _(seen in: CopyQ, ArsClip, Diodon, Raycast, Keyboard Maestro)_
483. **Create switchable profiles** `[v2]` - Support multiple named configuration profiles (e.g. Work, Personal) bundling their own settings, exclusions, and hotkeys, switchable from the tray menu.
484. **Auto-switch profile by context** `[future]` - Activate a profile automatically based on context signals such as active app, connected network/SSID, or time of day defined in rules. _(seen in: Keyboard Maestro)_
485. **Export and import configuration** `[v1]` - Allow exporting all settings (and optionally history) to a portable file and importing it on another machine to replicate setup. _(seen in: Alfred, Clibor, Espanso)_
486. **Reset settings to defaults** `[v1]` - Provide a clearly-labeled action to restore all preferences to factory defaults without necessarily wiping history, with a confirmation prompt.
487. **Check for and apply updates** `[v1]` - Build an in-app updater that checks for new releases, shows release notes, and downloads/installs them with user consent. _(seen in: ClipboardFusion, TextExpander)_
488. **Choose update channel** `[v2]` - Let users opt into stable vs beta release channels and toggle automatic vs manual update installation. _(seen in: Raycast)_
489. **Opt in to anonymous telemetry** `[v1]` - Default analytics to OFF and offer an explicit opt-in toggle for anonymous, aggregated usage and crash data, with a plain-language explanation of what is collected. _(seen in: Espanso, Ditto)_
490. **Submit opt-in crash reports** `[v2]` - Capture crash dumps locally and offer to send them to the developer only after explicit per-incident or pre-authorized consent.
491. **Switch theme and appearance** `[v1]` - Offer light/dark/system theme selection plus accent color, font, and density controls for the popup and settings UI. _(seen in: ArsClip, CopyQ, Raycast, Alfred, Comfort Clipboard Pro)_
492. **Configure popup layout and size** `[v1]` - Expose settings for popup position (cursor, fixed, last-used), dimensions, preview width, and number of visible rows. _(seen in: cliphist, Parcellite / ClipIt, Klipper)_
493. **Run multiple popup windows** `[v2]` - Allow more than one independent history window/instance open simultaneously, each potentially scoped to a different tab or profile, for side-by-side workflows.
494. **Select interface language** `[v1]` - Provide a localization setting to switch the UI language, with bundled translations and locale-aware date/number formatting. _(seen in: Clibor, Comfort Clipboard Pro)_
495. **View usage statistics dashboard** `[v2]` - Show a statistics panel with metrics like total captures, captures per day, most-used clips, content-type breakdown, and time saved by paste-back. _(seen in: TextExpander, PhraseExpress)_
496. **Surface storage usage breakdown** `[v2]` - Display how much disk the history database occupies, broken down by content type, with one-click compaction/vacuum.
497. **Toggle sound and desktop notifications** `[v1]` - Provide settings to enable/disable copy sounds, paste confirmations, and OS notifications, with per-event granularity. _(seen in: Comfort Clipboard Pro, Diodon, Parcellite / ClipIt)_
498. **Schedule automatic history clearing** `[v1]` - Let users configure auto-clear of history on a schedule, at app exit, or at next boot from the privacy settings pane. _(seen in: GNOME Clipboard Indicator, Klipper, Clipman (xfce4-clipman-plugin))_
499. **Run in portable mode** `[v2]` - Support a no-install portable build that stores all config and history beside the executable instead of OS config directories. _(seen in: Ditto, ArsClip, ClipX, Clibor)_
500. **Customize tray icon and menu** `[v2]` - Let users pick the tray/menu-bar icon style, choose monochrome vs color, and configure which quick actions appear in the tray context menu. _(seen in: ArsClip)_
501. **Relocate config and database paths** `[v2]` - Allow advanced users to set custom file paths for the config file and clipboard database, enabling storage on encrypted volumes or synced folders. _(seen in: cliphist, clipcat, Greenclip)_
502. **Provide multi-platform installers** `[v1]` - Ship via each platform's standard channels (macOS .dmg/Homebrew, Windows installer/Winget/Chocolatey/Store, Linux .deb/.rpm/AUR/Flatpak) with consistent settings migration. _(seen in: Ditto, CopyQ)_
503. **Edit raw config file** `[v2]` - Offer an 'open config in editor' action and a human-readable text/TOML config so power users can edit settings outside the GUI, with live reload on change. _(seen in: Greenclip, Espanso, cliphist)_
504. **Manage license and account** `[v2]` - Provide a settings section to view edition/tier, enter or deactivate a license key, and (if applicable) manage the linked account, kept separate from feature toggles. _(seen in: ClipboardFusion, Alfred)_
505. **Show onboarding and permissions setup** `[MVP]` - Present a first-run onboarding flow that requests required OS permissions (accessibility/clipboard access), sets the global hotkey, and explains privacy defaults. _(seen in: Paste, Raycast)_
506. **Search and reset individual settings** `[v2]` - Add a search box within the preferences window to jump to any setting by name, plus per-setting revert-to-default affordances. _(seen in: Raycast)_
507. **Back up and restore from settings** `[v2]` - Offer scheduled and on-demand backups of config plus history to a chosen folder, with a restore picker that previews backup contents before applying. _(seen in: Clibor)_

## 15. Platform-specific capabilities (mac/win/linux)

508. **Paste-back into previously focused app (macOS)** `[MVP]` - Synthesize a Cmd+V keystroke via the macOS CGEvent API into the app that had focus before the popup opened, restoring focus first. _(seen in: Paste, Alfred, Raycast)_
509. **Paste-back into previously focused app (Windows)** `[MVP]` - Send a Ctrl+V via the Windows SendInput API to the window that held foreground focus before vbuff's popup appeared. _(seen in: Ditto, Windows 11 Clipboard History)_
510. **Paste-back into previously focused app (Linux X11)** `[MVP]` - Inject a Ctrl+V key event through XTest (XTEST extension) into the previously focused X11 window after restoring its focus. _(seen in: CopyQ, Parcellite)_
511. **Paste-back into previously focused app (Linux Wayland)** `[v1]` - Paste back under Wayland where global key injection is restricted, using the wlr virtual-keyboard or input-method protocols (or ydotool/wtype) with graceful fallback to set-and-let-user-paste. _(seen in: CopyQ, cliphist)_
512. **Request macOS Accessibility permission** `[MVP]` - Detect missing AXIsProcessTrusted authorization and guide the user through granting Accessibility access needed for keystroke synthesis and focus control. _(seen in: Paste, Raycast, Alfred)_
513. **Global hotkey registration (macOS)** `[MVP]` - Register a system-wide hotkey to summon the popup using the Carbon RegisterEventHotKey API or a CGEventTap, surviving Secure Input mode where possible. _(seen in: Paste, Raycast, Alfred)_
514. **Global hotkey registration (Windows)** `[MVP]` - Register the summon hotkey via the Win32 RegisterHotKey API with conflict detection against system and other-app bindings. _(seen in: Ditto, ArsClip)_
515. **Global hotkey registration (Linux X11/Wayland)** `[MVP]` - Grab a global hotkey via XGrabKey on X11, and on Wayland register through the GNOME/KDE GlobalShortcuts portal (xdg-desktop-portal) since compositors block raw grabs. _(seen in: CopyQ, GPaste, Klipper)_
516. **macOS clipboard change monitoring via NSPasteboard polling** `[MVP]` - Detect new copies by polling NSPasteboard's changeCount on a timer, since macOS provides no clipboard-change callback. _(seen in: Paste, Maccy)_
517. **Windows clipboard listener via AddClipboardFormatListener** `[MVP]` - Receive WM_CLIPBOARDUPDATE messages through the Win32 clipboard-format-listener API for event-driven capture without polling. _(seen in: Ditto, Comfort Clipboard Pro)_
518. **X11 clipboard capture via CLIPBOARD selection owner** `[MVP]` - Capture copies on X11 by taking/observing CLIPBOARD selection ownership and requesting TARGETS, including a clipboard-manager handoff so content survives the source app closing. _(seen in: CopyQ, Parcellite, Klipper)_
519. **Wayland clipboard capture via wlr-data-control** `[MVP]` - Capture clipboard changes on Wayland using the wlr-data-control protocol (wl-clipboard) where supported, with a documented fallback for compositors lacking it. _(seen in: cliphist, CopyQ, wl-clipboard)_
520. **PRIMARY selection (middle-click) capture on Linux** `[v1]` - Optionally track the X11/Wayland PRIMARY selection produced by text highlighting, storing it alongside or separately from CLIPBOARD. _(seen in: GPaste, Klipper, Diodon, Parcellite, Clipman)_
521. **PRIMARY/CLIPBOARD sync and separation toggle** `[v1]` - Let users mirror PRIMARY into CLIPBOARD (and vice versa) or keep them strictly independent, per the X11/Wayland selection model. _(seen in: Klipper, Clipman, Greenclip, GPaste)_
522. **Middle-click paste of selected history item** `[v2]` - Set the PRIMARY selection from a chosen history entry so the user can paste it anywhere with a middle-click, matching X11/Wayland muscle memory. _(seen in: Klipper, Parcellite)_
523. **macOS menu bar (status item) UI** `[MVP]` - Provide an NSStatusItem menu-bar icon with a recent-items dropdown and keyboard navigation as the native macOS access point. _(seen in: Paste, Maccy, Alfred)_
524. **Windows system tray (notification area) UI** `[MVP]` - Provide a Shell_NotifyIcon tray icon with a context menu and recent-items flyout as the native Windows access point. _(seen in: Ditto, ArsClip, Clipboard Master)_
525. **Linux tray / app-indicator (StatusNotifierItem)** `[MVP]` - Expose a tray icon via the StatusNotifierItem/AppIndicator (KStatusNotifierItem) spec with XEmbed fallback for legacy panels. _(seen in: Diodon, CopyQ, Qlipper)_
526. **Windows Win+V panel integration** `[v2]` - Optionally inject vbuff entries into, or coexist cleanly with, the built-in Win+V Cloud Clipboard panel via the Windows clipboard history APIs. _(seen in: Windows 11 Clipboard History, Microsoft Cloud Clipboard)_
527. **Windows Jump List recent clips** `[v2]` - Populate the taskbar/Start Jump List with recent or pinned clips as tasks using the Win32 ICustomDestinationList API.
528. **macOS Dock menu recent clips** `[v2]` - Show recent and pinned clips in the right-click Dock menu via applicationDockMenu for one-handed access without the popup.
529. **macOS Services menu integration** `[future]` - Register a macOS Services provider so users can send the current selection to vbuff (or paste a clip) from any app's Services menu.
530. **macOS Spotlight / Shortcuts app actions** `[future]` - Expose vbuff actions (paste last clip, open history) as App Intents usable from the Shortcuts app and Spotlight. _(seen in: Raycast, Alfred)_
531. **Native notifications (macOS UserNotifications)** `[v1]` - Surface copy/sync/error notices through the macOS UserNotifications framework respecting the system Focus and notification settings. _(seen in: CrossPaste)_
532. **Native notifications (Windows toast)** `[v1]` - Show Windows toast notifications via the WinRT ToastNotification API, with actionable buttons where useful. _(seen in: CrossPaste)_
533. **Native notifications (Linux libnotify/D-Bus)** `[v1]` - Emit desktop notifications through the org.freedesktop.Notifications D-Bus interface (libnotify) for copy and error events. _(seen in: Parcellite, CopyQ)_
534. **OS keychain integration (macOS Keychain)** `[v1]` - Store vbuff secrets such as the history-DB encryption key or sync credentials in the macOS Keychain via the Security framework rather than on disk.
535. **OS credential store (Windows Credential Manager / DPAPI)** `[v1]` - Persist sensitive keys using the Windows Credential Manager and protect data with DPAPI bound to the user account.
536. **OS secret store (Linux Secret Service / kwallet)** `[v1]` - Store secrets via the freedesktop Secret Service D-Bus API (GNOME Keyring / KWallet) with an encrypted-file fallback on headless systems. _(seen in: CopyQ)_
537. **Concealed/sensitive clipboard flag handling** `[v1]` - Honor OS hints that mark a copy as transient or sensitive (Windows ExcludeClipboardContentFromMonitorProcessing / CanIncludeInClipboardHistory, macOS org.nspasteboard.ConcealedType, X11 password-manager hints) by skipping or auto-expiring such entries. _(seen in: GPaste, Windows 11 Clipboard History)_
538. **macOS transient/automatic pasteboard markers** `[v2]` - Set org.nspasteboard.TransientType / AutoGeneratedType when re-copying so other well-behaved macOS clipboard tools skip vbuff-originated writes (avoiding capture loops).
539. **Per-OS clipboard format/UTI mapping** `[v1]` - Translate between platform clipboard format identifiers (macOS UTIs, Windows CF_* / registered formats, X11/Wayland MIME targets) so rich content round-trips across the same OS faithfully. _(seen in: CopyQ, Ditto)_
540. **File reference paste with native semantics** `[v1]` - Capture and paste copied files using each OS's native file format (macOS NSFilenamesPboardType/file URLs, Windows CF_HDROP, X11/Wayland text/uri-list) so paste into Finder/Explorer/file managers works. _(seen in: Ditto, ArsClip, CopyQ)_
541. **Apple Universal Clipboard coexistence** `[v2]` - Detect and avoid double-capturing items arriving via Apple Continuity/Universal Clipboard, deduplicating against the local copy that lands on NSPasteboard. _(seen in: Apple Universal Clipboard)_
542. **macOS Secure Input detection** `[v1]` - Detect when an app has enabled Secure Event Input (e.g. password fields) and pause capture/keystroke synthesis, informing the user why paste-back is temporarily unavailable.
543. **Per-OS autostart / launch-at-login** `[v1]` - Register vbuff to start with the session natively per platform: macOS SMAppService/LaunchAgent, Windows Run-key or Startup, Linux XDG autostart desktop entry. _(seen in: Diodon, Comfort Clipboard Pro, CopyQ)_
544. **Wayland compositor capability detection** `[v1]` - Probe the running Wayland compositor for wlr-data-control, virtual-keyboard, and global-shortcut portal support and adapt features/messaging accordingly (GNOME vs KDE vs wlroots). _(seen in: cliphist, CopyQ)_
545. **X11-vs-Wayland session auto-detection** `[MVP]` - Detect the active Linux display server at startup (XDG_SESSION_TYPE / available sockets) and load the correct capture, paste, and hotkey backend automatically. _(seen in: CopyQ)_
546. **DPI / per-monitor scaling awareness** `[v1]` - Render the popup crisply across mixed-DPI setups using per-monitor DPI awareness on Windows, NSScreen backing-scale on macOS, and fractional-scale handling on Wayland.
547. **Popup placement at native cursor/caret** `[v2]` - Position the history popup at the text caret or mouse cursor using per-OS APIs (macOS accessibility AXTextField bounds, Windows GetCaretPos/GUITHREADINFO, X11 pointer query) for in-place pasting. _(seen in: Espanso)_
548. **Windows terminal paste compatibility (Ctrl+Shift+V)** `[v2]` - Send Ctrl+Shift+V instead of Ctrl+V when the target foreground window is a console/terminal class, matching Windows terminal paste conventions. _(seen in: Ditto)_
549. **macOS App Sandbox / hardened-runtime packaging** `[v1]` - Ship a notarized, hardened-runtime macOS build with the entitlements needed for global hotkeys and Accessibility, signed for Gatekeeper.
550. **Per-OS native packaging and installers** `[v1]` - Provide platform-native distribution: macOS signed .dmg/.pkg, Windows MSI/MSIX plus Winget and Microsoft Store, and Linux .deb/.rpm/Flatpak/AppImage. _(seen in: Ditto)_
551. **Drag-and-drop paste with native DnD** `[v2]` - Start a native drag of a history item (NSDraggingSession on macOS, OLE IDataObject on Windows, XDND on X11) so users can drop clips into any drop target. _(seen in: Ditto, CopyQ, Comfort Clipboard Pro)_
552. **GTK/Qt desktop theme conformance on Linux** `[v2]` - Follow the active desktop theme, dark/light preference, and accent color via the appearance portal/Qt platform theme so vbuff feels native on GNOME and KDE.
553. **macOS clipboard owner/source-app attribution** `[v2]` - Record the frontmost application that produced each copy via NSWorkspace so entries can show and be filtered by their originating macOS app.
554. **Window-class based ignore rules (per platform)** `[v1]` - Skip capturing from specified source windows using native identifiers: macOS bundle IDs, Windows window class/process, X11 WM_CLASS, Wayland app-id. _(seen in: CopyQ, Greenclip, GNOME Clipboard Indicator, ClipboardFusion)_
555. **Restore last clipboard on login (clipboard persistence daemon)** `[v1]` - On Linux, act as a clipboard manager that re-serves the last CLIPBOARD/PRIMARY contents after the source app exits or across logout, since X11/Wayland selections are owner-held and otherwise lost. _(seen in: CopyQ, Parcellite, Klipper)_

## 16. Performance, reliability & data integrity

556. **Embed bounded local SQLite store** `[MVP]` - Persist all clipboard history in a single embedded SQLite database file as the system of record, avoiding a separate server process or fragile flat files. _(seen in: Ditto, CopyQ)_
557. **Run capture in async daemon thread** `[MVP]` - Watch the OS clipboard on a dedicated background thread/event loop so the UI never blocks and idle CPU usage stays near zero. _(seen in: clipcat, GPaste, cliphist)_
558. **Cap history by item count** `[MVP]` - Enforce a configurable maximum number of stored items, evicting the oldest non-pinned entries first to bound database growth. _(seen in: Clipboard Master, ClipX, Clibor, cliphist)_
559. **Cap total store size on disk** `[v1]` - Enforce a configurable maximum total bytes for the history store, pruning oldest items once the budget is exceeded. _(seen in: cliphist, GPaste, CrossPaste)_
560. **Reject oversized clips with threshold** `[v1]` - Refuse to persist individual clips above a configurable byte limit (e.g. huge images or files) to protect memory and DB performance. _(seen in: cliphist, CrossPaste)_
561. **Index full-text search** `[v1]` - Maintain a full-text search index (e.g. SQLite FTS5) so as-you-type queries over tens of thousands of items return in milliseconds. _(seen in: Paste, Ditto)_
562. **Deduplicate by content hash** `[MVP]` - Detect re-copied identical content via a content hash and bump the existing row's timestamp instead of inserting a duplicate, keeping the DB lean. _(seen in: cliphist, Greenclip)_
563. **Coalesce growing selections** `[v2]` - Recognize when a newly copied value is an extension of the previous one and replace the prior entry rather than storing both. _(seen in: GPaste)_
564. **Store large blobs out-of-row** `[v1]` - Keep big images and file payloads as external blob files referenced by the DB, so row scans and search stay fast and the main DB stays small.
565. **Generate downscaled thumbnails** `[v1]` - Compute and cache small thumbnail bitmaps for image clips so list rendering never decodes full-resolution originals. _(seen in: Ditto, Paste)_
566. **Lazy-load and virtualize list** `[v1]` - Page query results and render only visible rows with windowed/virtualized scrolling so the popup opens instantly regardless of history size. _(seen in: cliphist)_
567. **Recover crashed clipboard contents** `[v1]` - Persist each capture transactionally the moment it arrives so an app crash, power loss, or kill never loses the most recent clips. _(seen in: Comfort Clipboard Pro, CopyQ)_
568. **Enable WAL crash-safe journaling** `[v1]` - Run SQLite in write-ahead-logging mode with synchronous commits so a power failure mid-write cannot corrupt the database.
569. **Self-heal corrupted database** `[v2]` - Run an integrity check on startup and, on detected corruption, automatically rebuild from a backup or quarantine the bad file and start fresh without crashing.
570. **Version and migrate schema** `[v1]` - Stamp the DB with a schema version and apply forward migrations on upgrade so user history survives across app versions.
571. **Snapshot periodic backups** `[v2]` - Take consistent on-disk backup snapshots of the history database on a schedule and on upgrade, with restore from a chosen snapshot. _(seen in: Clibor, Comfort Clipboard Pro)_
572. **Compact and vacuum on idle** `[v2]` - Reclaim space from deleted/pruned rows and external blobs by vacuuming and garbage-collecting orphaned blobs during idle periods.
573. **Optimize cold-start time** `[v1]` - Defer non-essential work, memory-map the DB, and lazily build the UI so the tray icon and global hotkey become responsive within a few hundred milliseconds of launch. _(seen in: ClipX, Qlipper)_
574. **Throttle background poll on battery** `[v2]` - Reduce clipboard-poll frequency and pause maintenance tasks when on battery or under power-saver to minimize wakeups and energy use.
575. **Cap resident memory budget** `[v2]` - Bound in-process RAM by evicting decoded images and search caches under an LRU memory ceiling, keeping idle footprint small. _(seen in: GPaste, ClipX, Diodon)_
576. **Defer reading delayed-render clipboard** `[v2]` - Render large or expensive formats lazily and on demand rather than eagerly materializing every offered format on each copy, avoiding stalls when apps publish delayed-render data.
577. **Handle clipboard-owner contention** `[v1]` - Open the clipboard with bounded retries/backoff and timeouts so a misbehaving owning app cannot hang capture or deadlock the daemon.
578. **Debounce rapid copy bursts** `[v1]` - Collapse multiple change notifications fired for a single copy (common with multi-format writers) into one capture to avoid duplicate work and rows.
579. **Single-instance guard with handoff** `[v1]` - Detect an already-running instance via a lock and forward the hotkey/show request to it instead of launching a second conflicting daemon.
580. **Auto-restart watcher on failure** `[v2]` - Supervise the capture loop and platform clipboard listener, restarting it with backoff if the OS drops the listener or a platform API errors.
581. **Compress stored text and blobs** `[v2]` - Transparently compress large text and blob payloads at rest to shrink disk usage with negligible decode cost on paste.
582. **Enforce write-path backpressure** `[v2]` - Batch and queue captures through a bounded async writer so a flood of rapid copies cannot exhaust memory or stall the watcher thread.
583. **Expire items by age policy** `[v2]` - Optionally auto-delete non-pinned items older than a configurable retention window during background maintenance to keep history bounded over time. _(seen in: GNOME Clipboard Indicator)_
584. **Verify integrity with health diagnostics** `[future]` - Expose a built-in diagnostics command reporting DB size, item counts, index health, and orphaned blobs, with a one-click repair/reindex action.
585. **Stream very large clips without OOM** `[future]` - Read, hash, and write multi-hundred-megabyte file or image clips via streaming chunks rather than loading them entirely into RAM.

## 17. Accessibility & internationalization

586. **Expose screen-reader accessibility tree** `[MVP]` - Render the entire UI (history list, search box, item previews, tray menu) through native platform accessibility APIs (UIA on Windows, NSAccessibility/AX on macOS, AT-SPI on Linux) so screen readers can perceive and navigate every control.
587. **Announce clip metadata in list rows** `[v1]` - Give each history row a descriptive accessible name and role conveying content type, source app, timestamp, and pinned status so a screen-reader user hears "image, 2 KB, from Safari, pinned" instead of raw bytes.
588. **Voice live-announce copy and paste events** `[v1]` - Fire ARIA-style live-region announcements through the platform speech API when a clip is captured, pasted, pinned, or deleted so blind users get confirmation of every action without watching the screen.
589. **Provide full keyboard-only navigation** `[MVP]` - Make every feature - opening the popup, searching, moving selection, pasting, pinning, deleting, opening settings - reachable and operable from the keyboard alone with a logical, predictable focus order. _(seen in: KDE Klipper, GNOME Clipboard Indicator, CopyQ)_
590. **Manage and rebind keyboard focus traps** `[v1]` - Trap focus inside the popup while it is open, restore focus to the previously focused application on close, and never strand keyboard focus on a hidden or disabled control.
591. **Honor OS high-contrast themes** `[v1]` - Detect and follow the operating system high-contrast / forced-colors mode (Windows High Contrast, macOS Increase Contrast, Linux high-contrast themes) so all text and UI chrome remain legible.
592. **Scale UI with OS display and font settings** `[MVP]` - Respect per-monitor DPI scaling and the system font-size / text-zoom preference so the popup, list, and previews grow proportionally without clipping or overlap.
593. **Offer in-app text size control** `[v2]` - Provide an independent font-size / zoom slider for the history list and previews so low-vision users can enlarge content beyond the OS default without affecting other apps.
594. **Honor reduced-motion preference** `[v1]` - Detect the OS reduce-motion setting and disable or shorten popup slide/fade animations, scrolling effects, and transitions for users sensitive to motion.
595. **Support right-to-left layout mirroring** `[v1]` - Mirror the entire interface layout (alignment, icon placement, search field, scrollbars) for RTL locales such as Arabic, Hebrew, Farsi, and Urdu.
596. **Render bidirectional clip text correctly** `[v1]` - Apply the Unicode bidirectional algorithm when displaying and previewing clip contents so mixed LTR/RTL text (e.g. Arabic with embedded English/numbers) shows in correct visual order.
597. **Localize the application interface** `[v1]` - Ship a translatable UI string catalog (e.g. Fluent/gettext) and bundle multiple languages so menus, settings, and dialogs appear in the user's chosen language. _(seen in: Clibor)_
598. **Auto-detect UI language from OS locale** `[v1]` - Pick the interface language automatically from the operating system locale on first run, with a manual override in settings.
599. **Format dates, times, and numbers per locale** `[v1]` - Render clip timestamps, sizes, and counts using the active locale's conventions for date order, 12/24-hour clock, decimal separators, and digit grouping.
600. **Render complex scripts with full font shaping** `[v1]` - Use a complex-text-layout shaping engine and font fallback so CJK, Indic, Thai, Arabic, and emoji clips display with correct glyphs, ligatures, and combining marks rather than tofu boxes.
601. **Respect OS color-accent and dark-mode settings** `[v1]` - Follow the system light/dark appearance and accent color so the manager visually integrates and remains comfortable for light-sensitive users. _(seen in: 1Clipboard)_
602. **Provide colorblind-safe content-type indicators** `[v2]` - Pair every color-coded distinction (content type, pinned, favorite) with an icon, label, or shape so colorblind users can tell items apart without relying on hue alone. _(seen in: Comfort Clipboard Pro, CrossPaste)_
603. **Meet WCAG contrast ratios in all themes** `[v2]` - Guarantee that bundled and generated themes hit WCAG AA contrast ratios for text and essential UI, validated at build time so no shipped theme is unreadable.
604. **Add screen-reader OCR of image clips** `[future]` - Run on-device OCR over copied images/screenshots so screen-reader users can hear the text contained in an otherwise opaque image clip. _(seen in: Paste, CrossPaste, ClipClip)_
605. **Caption and describe non-text clips** `[v2]` - Generate concise accessible descriptions for image, color, and file clips ("red 320x240 PNG screenshot", "folder of 3 PDFs") so non-text items are meaningful to assistive tech.
606. **Make global hotkey fully remappable for assistive needs** `[v1]` - Allow the summon hotkey and all in-popup keys to be remapped, including single-key and modifier-light bindings, to accommodate switch access, one-handed use, and conflicting AT shortcuts. _(seen in: ArsClip, Diodon, CopyQ)_
607. **Provide spoken/audio cues for key actions** `[v2]` - Offer optional distinct sound cues for copy, paste, and errors so users who cannot watch the screen get non-visual feedback independent of a full screen reader. _(seen in: Comfort Clipboard Pro)_
608. **Honor extended interaction and dismissal timing** `[v2]` - Expose configurable popup auto-dismiss timeouts (or no timeout) and avoid time-limited interactions so users who navigate slowly with assistive tech are not rushed.
609. **Support community translation contributions** `[v2]` - Use a standard externalized localization format and a contribution workflow (e.g. Weblate/Crowdin or PO files) so the community can add and maintain languages. _(seen in: Clibor)_
610. **Add per-language input and search normalization** `[v2]` - Apply locale-aware Unicode normalization, case-folding, and accent-insensitive matching in history search so queries like "resume" find "resume" across scripts and diacritics.
611. **Verify accessibility automatically in CI** `[future]` - Run automated accessibility audits (accessible-name presence, focus order, contrast, hit-target size) against the GUI in continuous integration to prevent regressions across releases.
612. **Provide a large-target high-readability mode** `[future]` - Offer an accessibility layout preset that enlarges hit targets, increases row spacing, and uses a high-readability typeface for users with motor or vision impairments.

## 18. Collaboration, sharing & team

613. **Generate shareable clip link** `[MVP]` - Create a one-click web URL that exposes a single clip's contents to anyone who opens it, with copy-to-clipboard of the link itself. _(seen in: ClipboardFusion)_
614. **Show clip as QR code** `[MVP]` - Render any clip (URL, text, Wi-Fi credentials) as a scannable QR/barcode in a popup so a phone or nearby device can pick it up instantly. _(seen in: KDE Klipper, Clipman (xfce4-clipman-plugin))_
615. **Send clip to a paired device** `[MVP]` - Push a selected clip directly to another of your own machines or phone over LAN with a manual 'Send to device' action. _(seen in: KDE Connect (clipboard sync), CrossPaste, Clipt (OnePlus / OneLab))_
616. **Export selected clips to file** `[MVP]` - Select one or many history items and export them as a single .txt, .csv, .json, or .md file for handing off or archiving. _(seen in: ClipX)_
617. **Paste clip into focused chat app** `[MVP]` - Drop a chosen clip straight into the currently focused messaging or chat window (Slack, Teams, Discord) via paste-back, optionally as plain text. _(seen in: Paste (Paste app / FiplabIO))_
618. **Send clip to a contact** `[v1]` - Pick a saved contact (email/handle) and dispatch the clip to them through a configured channel without leaving the popup.
619. **Shared team snippet library** `[v1]` - A central library of clips/snippets that an invited team can all browse, search, and paste from, kept in sync across members. _(seen in: TextExpander, aText, PhraseExpress)_
620. **Role-based permissions for shared sets** `[v1]` - Assign owner / editor / viewer roles per shared library so only authorized members can add, edit, or delete shared clips. _(seen in: TextExpander)_
621. **Subscribe to a shared set by URL** `[v1]` - Join a public or invite-only shared snippet group by pasting its share URL, pulling all its clips into your local list read-only. _(seen in: TextExpander)_
622. **Invite members to a shared library** `[v1]` - Send invitations by email or invite link to add collaborators to a shared clip set, with pending/accepted status tracking. _(seen in: TextExpander)_
623. **History of shared items** `[v1]` - A dedicated log showing every clip you have shared or sent, including recipient, channel, and timestamp, for audit and re-send.
624. **Link expiry and access controls** `[v1]` - Set a share link to auto-expire after a time window or view count, and optionally require a password to open it.
625. **Revoke a shared link** `[v1]` - Instantly disable any previously generated share link from the share history so its contents can no longer be opened.
626. **Drop clip into folder sync target** `[v1]` - Export selected clips into a watched Dropbox/OneDrive/Google Drive folder or network share so teammates pick them up via their own cloud. _(seen in: PhraseExpress, aText, Alfred)_
627. **Email selected clips** `[v1]` - Compose an email with one or more selected clips as the body or attachments straight from the history popup.
628. **Import a shared clip pack** `[v1]` - Open a shared .vbuff/.json export bundle to merge another person's exported clip collection into your own library. _(seen in: TextExpander, Alfred)_
629. **Comment on shared clips** `[v2]` - Leave threaded notes on items in a shared library so teammates can discuss why a snippet exists or how to use it.
630. **Pending-approval shared edits** `[v2]` - Require that viewer-or-contributor changes to a shared set be approved by an owner before they go live for everyone.
631. **Per-clip share audit trail** `[v2]` - Track who opened, copied, or pasted each shared clip and when, exposing a per-item access log for sensitive content.
632. **Team activity feed** `[v2]` - A live feed showing recent additions, edits, and removals across all shared libraries the team belongs to.
633. **Send-to-app integrations** `[v2]` - One-click routing of a clip into third-party tools (Slack, Notion, Jira, Trello) through configured integration targets.
634. **SSO and team admin console** `[v2]` - Centralized admin panel with SAML/SSO sign-on, consolidated billing, and member provisioning for organization-wide deployments. _(seen in: TextExpander)_
635. **Shared set version history and rollback** `[v2]` - Keep a revision timeline for each shared library so an owner can review past states and roll back accidental or bad edits.
636. **Usage analytics for shared snippets** `[v2]` - Per-snippet and per-member statistics showing which shared clips are pasted most, surfacing dead weight and team favorites. _(seen in: TextExpander, PhraseExpress)_
637. **End-to-end encrypted sharing** `[v2]` - Encrypt shared clips and links so only intended recipients with the key can decrypt the contents, even via relay. _(seen in: CrossPaste, Apple Universal Clipboard (Continuity))_
638. **Live collaborative clipboard session** `[future]` - Start a temporary shared session where multiple participants' copies stream into one ephemeral pool everyone can paste from in real time.
639. **Nearby-device handoff via proximity** `[future]` - Detect a trusted teammate's nearby device over Bluetooth/Wi-Fi and beam a clip to it with proximity-based discovery, no link needed. _(seen in: Apple Universal Clipboard (Continuity))_
640. **AI-suggested recipients and channels** `[future]` - Analyze a clip's content and the user's history to suggest the most likely contact, app, or shared set to send it to.
