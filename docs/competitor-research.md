# vbuff - Competitor Research Notes

### Windows clipboard managers

A mature, crowded utility category. Free open-source tools (Ditto, CopyQ) anchor the low end on privacy and power; commercial tools (ClipboardFusion, Comfort Clipboard Pro) compete on sync, macros, formatting, and polish; and Windows 11's built-in Win+V covers the casual majority for free. Several once-popular entries (ClipX, 1Clipboard) are now discontinued.

- **Ditto** - Free, open-source (GPL), Windows-only. The default recommendation for free clipboard history. Persistent unlimited history of text/images/HTML/files/custom formats, image thumbnails, regex search, global hotkey + tray, sticky clips, multiple copy buffers, and peer-to-peer LAN sync with AES encryption (no cloud, no login, no telemetry). Stores a local SQLite DB; many install paths (installer, ZIP, Chocolatey, Winget, Store). Weaknesses: dated UI, fiddly network-sync setup, Windows-only, no off-LAN cloud sync.

- **ClipboardFusion** - Freemium (Pro $19-$899, lifetime), from Binary Fortress; Windows/macOS/iOS/Android + browser. Headline feature is programmable macros in C#/VB.NET that transform content on copy, plus a rich trigger system (clipboard change, lock/unlock, timers, window focus). Cloud sync, online pinned items, and cross-device/browser access are Pro-only. Weaknesses: best features paywalled, macros need .NET skills, basic free tier.

- **ArsClip** - Free, Windows (+ Android via optional Global Clipboard). Multiple-item storage (text/images/HTML/RTF/Unicode/files), Permanent Clips, paste macros, optional cross-PC/Android Global Clipboard, portable, customizable colors/fonts/hotkeys. Weaknesses: dated UI, smaller community, Android only via add-on.

- **Clipboard Master** - Free (Jumping Bytes), Windows. Multi-clipboard up to 10,000 entries with a strong templates/phrases system (letter/email boilerplate, salutations, special characters, HTML, smart date/name fields), screenshot-to-clipboard, and an integrated password safe. Weaknesses: Windows-only, heavier feature set, occasional rough localization.

- **1Clipboard** - Free but discontinued (no releases since ~2016), Windows/macOS. Was a universal clipboard syncing history through the user's Google Drive, with favorites, search, and an Offline Mode. Weaknesses: unmaintained, often unavailable, no security updates.

- **Windows 11 built-in Clipboard History (Win+V)** - Free, built into Windows 10/11. Up to 25 items (text/images/HTML), pinning, search, "paste as text", optional Microsoft-account cloud sync (text only), and integrated emoji/GIF search. Weaknesses: 25-item cap, text-only sync that's off by default, no grouping/macros/templates/regex.

- **ClipAngel** - Free, open-source, Windows. Records text/RTF/HTML/images/files; favorites, type/status filters, real-time search, rich-text view toggle, in-app text editing, and case transforms (UPPER/lower/Sentence/CamelCase). Portable; needs .NET 4.52+. Weaknesses: dated UI, small user base, no cloud sync.

- **ClipX** - Free but discontinued (last update 2008), Windows. Tiny, low-resource history tool holding ~1024 items with hotkey recall and a plugin system (categories, persistent clipboards, export). Weaknesses: abandoned, no updates/security fixes, modern-Windows compatibility issues; CopyQ is the usual replacement.

- **Clibor** - Free (Japanese dev Chigusa), Windows. Portable ~800KB tool storing up to 10,000 entries, specializing in FIFO/LIFO multi-paste, "set phrases" with groups/memos, regex find-and-replace, incremental search, multi-select, startup-password encryption, and backup/restore. Multilingual (EN/JA). Weaknesses: Windows-only, dense UI, no cloud sync.

- **Comfort Clipboard Pro** - Paid $19.95 one-time (30-day full trial), Windows (XP-11, Server 2008-2025). Polished commercial manager: configurable/unlimited history across all formats (keeps multiple formats per item), crash/power-failure recovery, color coding, custom display names, 448-bit Blowfish encryption + password protection, drag-and-drop, multi-fragment paste, templates, per-app paste rules, exception list, hotkeys, and sound alerts. Weaknesses: paid after trial, Windows-only, no cloud sync.

- **CopyQ** *(notable addition)* - Free, open-source (GPLv3), Windows/macOS/Linux. The most powerful free option: clipboard stored in customizable tabs with tags and notes, full scripting via a command line and Python/Bash/PowerShell, custom MIME handling, plugins, and deep search/edit. Weaknesses: steep learning curve, plain default UI, no built-in cloud sync.

- **ClipClip** *(notable addition)* - Free, Windows. User-friendly history with folder organization, search, plain-text paste, clip editing, hotkey popup, plus convenience extras like translation/OCR. Weaknesses: Windows-only, closed-source, some features depend on online services. (Specifics from general listings, not deeply verified in this pass.)

### Linux clipboard managers

All apps in this cluster are free and open-source; there is no paid tier among them. The main axes of differentiation are: desktop-environment integration (GNOME vs KDE vs Xfce vs WM), display server support (X11 vs Wayland), and depth of features (minimal history-only vs scriptable power tools).

- **CopyQ** - Platforms: Linux (X11/Wayland), Windows, macOS 13+. Pricing: free, GPL-3.0. The most powerful and scriptable cross-platform manager. Stores text/HTML/images/custom formats across multiple tabs, with a JavaScript-like scripting engine, advanced CLI, automatic commands on matching content, notes, tags, search, optional GnuPG encryption, and window/text ignore rules. Weakness: complex and overwhelming for casual users; dated default UI; heavier footprint.

- **GPaste** - Platforms: Linux (GNOME/GTK). Pricing: free, GPL/BSD. GNOME-native library + daemon + CLI + Shell extension. Searchable panel history of text, file paths, and images; detects growing lines and replaces extended entries; masks password items; configurable limits. Weakness: effectively GNOME-only; image support is opt-in; breaks on some GNOME Shell upgrades.

- **KDE Klipper** - Platforms: Linux (KDE Plasma, X11/Wayland). Pricing: free, GPL, bundled with Plasma. Default Plasma manager with persistent history, regex-matched clipboard actions (auto-open URLs etc.), QR/barcode generation, type-to-search, and inline editing. Weakness: Plasma-only; no tabs/scripting; regex action setup is unintuitive.

- **Diodon** - Platforms: Linux (GTK, GNOME/Unity). Pricing: free, GPL-3.0. Lightweight GTK manager with Ubuntu indicator integration, primary/clipboard sync, image support, a plugin system, and Zeitgeist integration for an effectively infinite history. Weakness: tied to the Unity/GNOME era; slowing development; Zeitgeist dependency.

- **Parcellite / ClipIt** - Platforms: Linux (GTK, X11). Pricing: free, GPL. Classic minimalist managers (ClipIt is a fork of Parcellite). Persistent history, primary/clipboard sync, hotkeys, %s-based clipboard actions, daemon mode, search-as-you-type, whitespace handling. Weakness: both largely abandoned, X11-only, buggy, dated GTK2 UI.

- **Clipman (xfce4-clipman-plugin)** - Platforms: Linux (Xfce, X11). Pricing: free, GPL. Standard Xfce panel plugin handling text and images, with regex-matched actions, separate or synced PRIMARY/CLIPBOARD handling, and optional QR-code display via qrencode. Weakness: Xfce/X11-only; minimal organization; regex actions require expertise.

- **GNOME Clipboard Indicator** - Platforms: Linux (GNOME Shell, X11/Wayland). Pricing: free, MIT. The most popular GNOME extension (2M+ downloads). Text + image history, pin items, regex search, edit entries, tag entries, private mode, per-app exclusion, scheduled auto-clear, rich keyboard shortcuts. Weakness: GNOME Shell only; short freeze on large images; menu paste fails in some apps.

- **Greenclip** - Platforms: Linux (X11). Pricing: free, BSD-3-Clause. Minimal Haskell daemon built for rofi (also dmenu/fzf), with a static binary, pinned permanent entries, PRIMARY+CLIPBOARD merge, app blacklist, and small-image support. Weakness: X11-only; no GUI of its own; niche tiling-WM audience.

- **clipcat** - Platforms: Linux (X11; experimental Wayland). Pricing: free, MIT/GPL. Rust client-server manager: a daemon (clipcatd) with a gRPC API, CLI client (clipcatctl), finder menu (clipcat-menu), and event-notify tool (clipcat-notify). Text + image support, configurable history. Weakness: experimental Wayland; multi-binary setup; no built-in GUI; smaller community.

- **cliphist** - Platforms: Linux (Wayland only). Pricing: free, GPL-3.0. Wayland-native, pipe-based history built on wl-clipboard. Byte-for-byte content preservation (whitespace, Vim selections), text + image support, deduplication, delete/wipe, configurable max-items/size and preview width. Weakness: Wayland-only; no GUI (needs an external picker); intentionally minimal.

- **Qlipper** - Platforms: Linux, BSD, Windows, macOS. Pricing: free, GPL-2.0. Tiny cross-platform Qt systray applet often used with LXQt. Saves/restores history, global shortcuts, network clipboard sharing, few dependencies. Weakness: very basic (history only); no images/tags/actions; sparse maintenance.

Notable cross-cluster patterns: almost every manager offers persistent history, search, configurable size limits, and PRIMARY/CLIPBOARD sync. Differentiators that only a few provide: scripting (CopyQ), encryption (CopyQ), QR generation (Klipper, Clipman), regex-driven actions (CopyQ, Klipper, Clipman, Parcellite/ClipIt), tags/notes (CopyQ, Clipboard Indicator), network sharing (Qlipper), and true byte-for-byte Wayland preservation (cliphist).

### Cross-platform & cloud-sync clipboard managers

This cluster spans three architectural approaches: (1) commercial managers with vendor-cloud or own-cloud sync (ClipboardFusion, Paste, 1Clipboard), (2) OS-native sync built into the platform (Microsoft Win+V Cloud Clipboard, Apple Universal Clipboard), and (3) open-source LAN/peer sync (CrossPaste, KDE Connect). PasteBar is included as a notable open-source manager that is deliberately local-only (a useful contrast), and Clipt is included as a cautionary discontinued example. All facts on pricing and headline features were verified against vendor sites in June 2026 where possible.

- **ClipboardFusion (Binary Fortress)** - Windows/macOS/iOS/Android/Chrome/web. Freemium; Pro is a one-time lifetime license ($19 Standard, $29 Personal, $199 Site, $899 Enterprise). Macro-driven Windows manager whose standout is a C#-like scripting engine for transforming/scrubbing clips; cross-device cloud sync, online pinned items, and event triggers are Pro-only. Weak points: sync/automation paywalled, dated UI, clips route through vendor servers.

- **Paste (pasteapp.io)** - macOS/iOS/iPadOS only. Subscription $3.99/mo or $29.99/yr (also in Setapp). The polished Apple-ecosystem pick: private iCloud sync, unlimited history, pinboards, rich previews, paste stack, and (since late 2025) OCR/Apple-Intelligence search inside image clips. Weak points: Apple-only, subscription-only (drew backlash dropping the perpetual license), iCloud-dependent.

- **CrossPaste** - Windows/macOS/Linux/Android/iOS. Free, open source (AGPL v3). The broadest true cross-platform syncer: instant LAN sync with optional end-to-end encryption, no cloud servers, rich content types (text/HTML/images/files/colors), unlimited history, optional OCR, and auto space management. Weak points: LAN-only by default, younger project, encryption/OCR off by default.

- **1Clipboard** - Windows/macOS. Free (needs a Google account). Minimal manager that syncs Mac↔Windows via the user's own Google Drive; searchable history and starred clips, works offline as a history browser. Weak points: stale/infrequent updates, no mobile or Linux, thin feature set.

- **PasteBar** - macOS/Windows. Free, open source (CC BY-NC). Local-only power tool: unlimited history, collections/tabs/boards, 30+ copy/paste operations, templated clips with form fields, code syntax highlighting, PIN-locked collections, web/API data extraction. Weak point for this cluster: no cloud or cross-device sync at all - strictly local.

- **Microsoft Cloud Clipboard (Win+V)** - Windows 10/11/Server, plus Android via SwiftKey/Phone Link. Free, built in (Microsoft account for sync). OS-native history panel with optional cloud sync across PCs, auto or manual per-item sync, pinning, and an integrated emoji/GIF picker. Weak points: ~25-item cap, 4 MB/item limit, history cleared on restart (except pins), no macOS/Linux/iOS, no boards/search.

- **Apple Universal Clipboard (Continuity)** - macOS/iOS/iPadOS. Free, built in (same Apple ID, Wi-Fi, Bluetooth, Handoff). Zero-config copy-here-paste-there across nearby Apple devices, end-to-end encrypted, supports text/images/video and Mac-to-Mac files. Weak points: no history (latest item only), item expires in ~2 minutes, proximity-bound, Apple-only, occasionally flaky.

- **KDE Connect (clipboard sync)** - Linux/Windows/macOS/Android. Free, open source. Bidirectional clipboard sync over LAN as part of a broad device-bridge suite (notifications, file transfer, remote input, battery). End-to-end encrypted pairing, no cloud/account. Weak points: automatic clipboard read on modern Android needs an ADB workaround, LAN-only, no history UI (it's a transport not a manager), limited iOS.

- **Clipt (OnePlus/OneLab) - DISCONTINUED** - Android + Chrome extension. Was free; backend shut down Sept 30, 2023 and is no longer functional. Used the user's own Google Drive to move clips and files between Android and Chrome (last ~10 items). Included as a cautionary example of a cloud-sync clipboard tool whose dependence on a vendor backend killed it.

Honorable mentions outside the structured set (popular managers that are largely local-only and so sit adjacent to this cluster): Ditto, CopyQ, ClipClip and Maccy on desktop, plus newer entrants like CrossClip and ClipSync targeting cross-device sync. CrossClip in particular markets real-time iPhone/Android/Mac/Windows sync but its feature set was not fully verifiable at research time.

### Power-user features & adjacent tools (scripting, snippets, expanders)

This cluster covers the apps that turn clipboard/snippet management into full automation: open-source scripting tools, dedicated text expanders, and launcher/macro platforms where clipboard handling is one capability among many. Pricing below is verified current as of mid-2026.

- **CopyQ** - Free/open-source (GPLv3), Windows/macOS/Linux. The power-user's clipboard manager: persistent history of text, HTML, images, and custom formats across multiple named tabs, plus a full JavaScript scripting engine, a command dialog (F6) for custom actions triggered on clipboard change, and a powerful CLI. Adds notes/tags, ignore rules, tab encryption, Vim-like editor, and directory sync. Weaknesses: dated Qt UI, steep scripting-based learning curve, weaker macOS build, no official cloud sync.

- **Espanso** - Free/open-source (GPLv3), Windows/macOS/Linux. Privacy-first text expander in Rust that runs 100% locally. YAML config, regex triggers, date math, shell/script execution with output injection, multi-field forms, variables, an ALT+Space search bar, and the Espanso Hub package ecosystem. Weaknesses: no native GUI (hand-edit YAML), limited Wayland support, no built-in sync, not beginner-friendly.

- **TextExpander** - Subscription only (Life Hacker ~$3.33/user/mo annual; Team ~$8.33; Growth ~$10.83; Enterprise custom), macOS/Windows/iOS/iPadOS/Chrome. Polished, team-oriented snippet manager: cloud sync everywhere, shared libraries with permissions, fill-in forms, date/JS/AppleScript snippets, suggested snippets, nested snippets, usage analytics, and AI snippet tools. Weaknesses: subscription-only (no perpetual), priciest per-user for teams, mandatory cloud, lighter true scripting.

- **PhraseExpress** - Perpetual one-time license from ~€100 with 1 year of updates (paid maintenance after; separate Windows/macOS licenses; free for private use), Windows/macOS/iOS. Feature-dense, Windows-first autotext/phrase manager: macro engine (keystrokes, mouse, branching), fill-in forms, built-in clipboard cache, SQL client-server sharing, multi-cloud sync, multi-language phrases, and AI rewrite/translate. Weaknesses: macOS lags Windows badly, confusing/aggressive licensing and nag prompts, cluttered UI, steep macro learning curve.

- **aText** - Cheap: ~$4.99/yr or ~$29.99 lifetime (business/floating tiers), macOS/Windows. Lightweight, inexpensive TextExpander alternative: abbreviation expansion, rich text/images/attachments, date/clipboard/fill-in variables, keystroke/mouse/shell scripting, auto-capitalization, built-in snippet packs, and Windows<->macOS sync via cloud-drive folders. Weaknesses: basic/dated UI, fewer team features, third-party-folder sync conflicts, limited platform reach, smaller community.

- **Raycast** - Free core (clipboard history, snippets, quicklinks); Pro $8/user/mo annual or $10/mo (unlimited clipboard history, AI, Cloud Sync, themes); Teams $12; Advanced AI +$8. macOS-first, Windows 2.0 in beta, iOS companion. Extensible launcher whose 2,000+ extension store doubles as a clipboard/snippet/automation hub, with a React/TypeScript SDK, script commands, snippets with dynamic placeholders, quicklinks, window management, and Raycast AI. Weaknesses: macOS-first (Windows lagging, no Linux), best features paywalled, variable extension quality, account/cloud dependency.

- **Alfred** - Free core; Powerpack one-time ~£34 (~$46), Mega Supporter adds lifetime upgrades. macOS only. Mature launcher whose Powerpack adds clipboard history (text/images/file paths), auto-expanding snippets, and a visual no-code Workflow builder that connects trigger/input/action/output objects and runs scripts in many languages. Also system commands, contacts, music control, multi-Mac preference sync. Weaknesses: macOS only, key features behind Powerpack, pressured by Raycast's free store, scripting still needed for advanced workflows, no first-party sync.

- **Keyboard Maestro** - Perpetual one-time US$36 (up to 5 Macs; ~$25 upgrade), macOS only. Deep automation/macro engine where clipboard is one capability among hundreds: clipboard history switcher, persistent Named Clipboards, clipboard transform actions, typed-string expansion, and a vast trigger set (hotkey, app event, schedule, Wi-Fi, USB, folder/clipboard change). Adds flow control, variables, app/window switchers, image-recognition/OCR, and AppleScript/shell/JS actions. Weaknesses: macOS only, very steep curve, dense UI, not a dedicated clipboard app, no team/cloud library.

**Cluster pattern:** Two distinct camps converge here. Pure expanders (Espanso, TextExpander, PhraseExpress, aText) compete on expansion depth, sync, and team libraries; automation platforms (CopyQ, Raycast, Alfred, Keyboard Maestro) treat clipboard/snippets as one module inside a scripting or launcher engine. Open-source options (CopyQ, Espanso) win on privacy and price; commercial Mac tools split between perpetual licenses (Alfred, Keyboard Maestro, PhraseExpress, aText) and subscriptions (TextExpander, Raycast Pro). The richest differentiation for a clipboard manager moving into this cluster is scripting/CLI access, forms/fill-ins, dynamic variables, app-scoped behavior, and an extension ecosystem.

I have verified data for all nine. Here is the section.

### macOS clipboard managers

macOS ships no built-in clipboard history, so a long tail of third-party managers fills the gap, ranging from free open-source menu-bar utilities to paid power tools and clipboard features bundled inside launcher apps. The nine below cover the most commonly referenced options. Pricing and feature details were verified via vendor sites, the Mac App Store, and developer GitHub repos in mid-2026 where possible; items I could not independently confirm are flagged as uncertain.

#### Maccy
- **Platforms:** macOS only (requires macOS Sonoma 14 or later as of the current release; older builds supported earlier versions).
- **Pricing:** Free and open source (MIT license). Source and notarized builds are free from GitHub; the Mac App Store listing is a paid convenience version (price fluctuates, typically a few dollars) that funds development. Also installable via Homebrew.
- **Positioning:** Minimalist, keyboard-first, open-source clipboard history that "just works" and stays out of the way.
- **Standout features:**
  1. Sub-second open and fuzzy search across the entire history (type to filter, Enter to paste).
  2. Fully keyboard-driven workflow; navigable without touching the mouse.
  3. Preserves rich text and images so formatting/quality is not degraded on paste.
  4. Pinning of frequently used entries so they persist and are not evicted from history.
  5. Configurable history size, hotkey, and popup position/appearance.
  6. Local-only storage (nothing leaves the machine); option to ignore specific apps/transient data.
  7. Lightweight, low memory footprint, idle in the menu bar.
  8. Native Swift app, actively maintained with a healthy GitHub following.
- **Weaknesses:** Single-device only (no cross-device or iOS sync); deliberately spartan, so it lacks paste filters/transformations, snippets, and multi-pasteboard organization that paid tools offer; newer macOS minimum cuts off older hardware.

#### Paste (pasteapp.io)
*(Note: the task referenced "Literally Utilities" as the publisher; I could not verify that name. The widely known Paste clipboard app is published under pasteapp.io. Treat the publisher attribution as uncertain.)*
- **Platforms:** macOS, iOS, and iPadOS (one subscription covers all).
- **Pricing:** Subscription, approximately $3.99/month or $29.99/year (verified mid-2026; subject to change). Includes the iOS/iPadOS apps.
- **Positioning:** Polished, design-forward "limitless clipboard" with seamless cross-Apple-device sync; the mainstream premium pick.
- **Standout features:**
  1. Unlimited history of text, links, images, and files, searchable far back in time.
  2. iCloud sync across Mac, iPhone, and iPad (copy on one, paste on another).
  3. Visual pinboards/boards to organize clips into named collections.
  4. Reusable snippets/templates for frequently pasted text.
  5. Per-app rules and exclusion of sensitive apps from capture.
  6. Quick-paste keyboard navigation plus an attractive full-width clipboard bar UI.
  7. Apple Intelligence integration with OCR search over image clips (late-2025 addition; search screenshot text such as receipt amounts) - verify exact behavior, marked as recently added.
  8. Long track record (10+ years) and frequent updates.
- **Weaknesses:** Subscription-only with no perpetual-license option, which some users dislike; locked to the Apple ecosystem (no Windows/Android/Linux); heavier and more "app-like" than minimalist menu-bar tools; OCR/AI features are recent and may vary by region/OS version.

#### Pastebot (Tapbots)
- **Platforms:** macOS only.
- **Pricing:** One-time purchase on the Mac App Store, around $12.99-$19.99 (Tapbots launched at $19.99 and has discounted it; verify current price). No subscription.
- **Positioning:** A clipboard "power tool" for people who work primarily on a single Mac, centered on transforming text as you paste.
- **Standout features:**
  1. **Filters** - reusable paste-time transformations (strip formatting, find-and-replace, change case, custom chains) applied on paste.
  2. Custom pasteboards to group clips into named buckets.
  3. Sequential paste (paste a series of clips in order).
  4. Stores up to ~500 recent items: text, URLs, images, files.
  5. Search by content, source app, date, or type.
  6. iCloud sync of main clipboard, custom pasteboards, and filters across your Macs.
  7. Extensive keyboard shortcuts for power users.
  8. App blacklist so chosen apps (e.g., password managers) are never captured.
  9. Tapbots' signature high-polish native UI.
- **Weaknesses:** Mac-only with no iOS companion (sync is Mac-to-Mac, not phone); 500-item cap is small versus "unlimited" rivals; update cadence has historically been slow between major versions; filters have a learning curve.

#### Flycut
- **Platforms:** macOS only.
- **Pricing:** Free and open source (MIT license); on GitHub and the Mac App Store at no cost. Built on the older Jumpcut codebase.
- **Positioning:** Clean, simple, developer-oriented clipboard buffer for plain-text snippets.
- **Standout features:**
  1. Configurable global hotkey (default Shift-Cmd-V) to cycle and paste recent clippings.
  2. Adjustable history size and capture behavior.
  3. Plain-text focus, ideal for code snippets and shell commands.
  4. Search box in the menu to filter clippings.
  5. Favorites mode for frequently reused items.
  6. Mouse support (scroll wheel to select, double-click to paste).
  7. Option to move a pasted item back to the top of the stack.
  8. Tiny, low-overhead menu-bar app.
- **Weaknesses:** Plain-text oriented with weak/limited rich-text and image handling; development has stalled (last meaningful updates around 2022), raising newer-macOS compatibility concerns; dated UI; no sync, snippets, or paste transformations.

#### Clipy
- **Platforms:** macOS only.
- **Pricing:** Free and open source (MIT license), on GitHub.
- **Positioning:** Lightweight open-source clipboard extension, a successor to the discontinued ClipMenu.
- **Standout features:**
  1. Menu-based clipboard history accessible via hotkey.
  2. Snippets for storing and quickly inserting frequently used text.
  3. Image previews of copied content.
  4. Customizable hotkeys and menu behavior.
  5. Adjustable number of stored history items.
  6. Organize snippets into folders/groups.
  7. Low resource footprint, menu-bar resident.
- **Weaknesses:** Maintenance has been intermittent and some users report reliability problems on recent macOS versions; no cross-device sync; no paste-time transformations; UI is functional but basic; inherits a ClipMenu-era design rather than a modern search-first one.

#### ClipMenu
- **Platforms:** macOS only.
- **Pricing:** Free (closed-source historically; a Swift rewrite has been referenced as "under development," status uncertain).
- **Positioning:** The classic, long-running free macOS clipboard manager that defined the menu-based pattern many others copied.
- **Standout features:**
  1. Menu-driven clipboard history triggered by hotkey.
  2. Snippets for canned text insertion.
  3. Multiple data types (plain text, rich text, images).
  4. Customizable shortcuts and number of remembered items.
  5. Optional "actions" to process clipboard contents.
  6. Very small footprint.
- **Weaknesses:** Effectively discontinued; the original is unmaintained with no fixes for bugs, security, or new-macOS compatibility, and may not run reliably on current macOS. Largely of historical interest now; Clipy is the de facto continuation. A rebuilt Swift version has been mentioned but is not confirmed shipping (uncertain).

#### CopyClip (FIPLAB)
- **Platforms:** macOS only.
- **Pricing:** CopyClip 1 is free on the Mac App Store. CopyClip 2 is a one-time paid upgrade, around $7.99. (Publisher FIPLAB; verified mid-2026.)
- **Positioning:** Simple, affordable menu-bar clipboard history; the free CopyClip 1 is a common "good enough free" recommendation.
- **Standout features (CopyClip 2):**
  1. Large buffer, up to ~9,999 text clippings.
  2. Paste with original formatting or as plain text.
  3. Instant search across the full history.
  4. Quick-paste keyboard shortcuts (Cmd+1-9, Cmd+0).
  5. Pin important clippings.
  6. Automatically skips concealed/sensitive fields (e.g., passwords) so they never enter history.
  7. Multiple themes.
  8. Silent menu-bar operation with low memory and near-zero idle CPU.
- **Weaknesses:** Text-focused (limited image/file handling versus Paste/Pastebot); no cross-device or iOS sync; CopyClip 1 is quite barebones (small history, no search), effectively pushing serious users to pay for v2; no paste transformations or snippet expansion.

#### Raycast Clipboard History
- **Platforms:** macOS (Raycast launcher); a Windows version of Raycast exists but verify clipboard parity separately - marked uncertain for Windows.
- **Pricing:** Free tier includes Clipboard History but caps retention at three months. Raycast Pro (about $8/month billed annually, or $10/month monthly) unlocks unlimited, locally stored clipboard history. (Verified mid-2026.)
- **Positioning:** Clipboard history bundled into a broader Spotlight-replacement launcher, so it lives where you already run commands.
- **Standout features:**
  1. Searchable history of text, images, colors, and links.
  2. Pin frequently used entries.
  3. Local, encrypted storage; passwords and transient data ignored by default.
  4. Invoked from the same command bar as everything else in Raycast (no separate app/hotkey to learn).
  5. Quick paste and "paste as plain text."
  6. Pro tier removes the 3-month retention limit (unlimited, local).
  7. Extensible via the Raycast ecosystem (community extensions can augment workflows).
- **Weaknesses:** Free tier deliberately limits history to 3 months, nudging toward a subscription; clipboard history is local-only and (per Raycast) intentionally excluded from cloud sync, so no cross-device clipboard; requires buying into the whole Raycast launcher rather than a standalone tool; depth of clipboard organization (boards, paste filters) is lighter than dedicated apps like Pastebot/Paste.

#### Alfred Clipboard History
- **Platforms:** macOS only.
- **Pricing:** Alfred itself is free, but Clipboard History requires the paid Powerpack: roughly £34 single-version license (~$40-45 USD) or ~£59 (~$70-75 USD) for the "Mega Supporter" lifetime-upgrades license. (Verified mid-2026; USD figures approximate.)
- **Positioning:** Clipboard history as one feature of the venerable Alfred launcher/automation suite, for users already invested in Alfred.
- **Standout features:**
  1. Searchable history of text, images, and file links.
  2. Configurable retention windows (24 hours, 7 days, 1 month, or 3 months).
  3. Snippet expansion: type a short abbreviation to expand into full text.
  4. Save a clip directly as a snippet from the history viewer (Cmd+S).
  5. Merge/append multiple clips before pasting.
  6. Clear-on-quit and app-exclusion options for privacy/sensitive data.
  7. Deep integration with Alfred Workflows for automation around clipboard content.
  8. Fast, keyboard-driven access from the Alfred bar.
- **Weaknesses:** Locked behind the Powerpack purchase and tied to the Alfred ecosystem; max retention is 3 months (no truly unlimited archive); macOS-only with no cross-device/iOS clipboard sync; overkill if you only want clipboard history and nothing else Alfred offers.

#### Cross-cutting notes for the competitive analysis
- **Free/open-source tier:** Maccy (actively maintained, search-first), Flycut and Clipy (lighter, slower-moving), ClipMenu (legacy/discontinued), CopyClip 1 (free but minimal). Maccy is the strongest modern free standalone.
- **Paid standalone power tools:** Pastebot (one-time, paste filters, Mac-only) and Paste (subscription, best cross-Apple-device sync). CopyClip 2 is the cheap paid middle ground.
- **Bundled-into-launcher:** Raycast and Alfred deliver clipboard history as part of a larger productivity app; attractive to existing users, but both gate it (Raycast behind Pro for unlimited; Alfred behind the Powerpack) and neither does cross-device clipboard sync.
- **Common gaps a competitor could exploit:** cross-device sync is rare outside Paste (and Pastebot Mac-to-Mac); most are Apple-only; paste-time transformations are largely a Pastebot differentiator; several open-source options have lagging maintenance and newer-macOS compatibility risk.

Pricing and version specifics above were checked against vendor and store listings in mid-2026 and can drift; the Paste publisher name, CopyClip exact current price, Pastebot exact current price, the ClipMenu Swift-rewrite status, and Raycast's Windows clipboard parity are the items I would re-verify before publishing.

Self-rating: 8/10 - Pricing and headline features for all nine are verified against vendor/store sources; the main residual uncertainties (Paste publisher attribution, exact current Pastebot/CopyClip prices, ClipMenu rewrite status, Raycast Windows parity) are explicitly flagged rather than fabricated. Not a 9-10 because I relied on search-result summaries rather than fetching every vendor page individually, so a few price points could be slightly stale.
