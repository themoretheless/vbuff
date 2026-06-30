# vbuff - Competitor Pain-Point Research Notes

### macOS app complaints

Concrete, evidence-backed problems users report with macOS clipboard managers (Paste, Pastebot, Maccy, CopyClip, Flycut, Clipy, ClipMenu, Raycast/Alfred). Sourced from GitHub issues, App Store / Product Hunt reviews, MacRumors / Apple Community / Keyboard Maestro forums, and security write-ups.

- **The core paste action silently fails.** Selecting a history item inserts nothing, plays an error sound, or (worst) pops a notification containing the text instead of pasting it. Maccy issues #1280, #1230, #754. Apps: Maccy, Paste, Pastebot, ClipMenu. (critical)
- **Wrong formatting on paste.** Rich text comes through when plain text was wanted; the "paste without formatting" path strips/mangles content (e.g. file URLs removed). Maccy #109/#693, Alfred forum ("the main reason I don't use Alfred as a clipboard manager"). Apps: Maccy, Paste, Alfred, Pastebot. (high)
- **Shortcut collisions with macOS.** Paste-as-plain-text uses Cmd+Shift+number, which fires the OS screenshot tool instead. Maccy #1232. App: Maccy. (medium)
- **Popup steals focus.** The history window grabs focus from the active app, breaking the paste target; Maccy later removed its "avoid taking focus" option to always behave like Spotlight. Maccy #144. Apps: Maccy, ClipMenu. (high)
- **De facto abandonment.** Long update gaps, ancient minimum-macOS targets, unreliable on current OS. Pastebot last updated 2024, "no longer works reliably with Safari" (Oct 2025 forum); ClipMenu untouched since ~2009. Apps: Pastebot, ClipMenu. (high)
- **Breaks on each new macOS release.** Hover/selection states die, menus flicker, items unselectable after upgrade (ClipMenu after Sonoma; Clipy "doesn't work properly"). Apps: ClipMenu, Clipy. (high)
- **Performance collapses on large history.** Loads everything into the UI at once -> freezes, sluggish scroll, slow summon that feels like a crash. Maccy #1097 (~50k items); Pastebot "bogged down so bad." Apps: Maccy, Pastebot. (high)
- **Secrets captured into persistent on-disk history.** Passwords, API keys, PII land in a plaintext SQLite/plist file; macOS has no clipboard permission gate or audit log, so any app can read it. Apple is adding a read-warning banner in macOS 16 for this reason. All apps. (critical)
- **Inconsistent honoring of "do not record" / concealed-content hints.** Copied 1Password credentials still hit history unless the user manually excludes apps (Maccy #979). Apps: Maccy, Paste, Raycast/Alfred, CopyClip. (high)
- **Subscription pricing resentment.** Users expect a one-time small utility; reviews call the subscription "most definitely not worth paying for." Paste added a $90 lifetime plan under pressure; CopyClip 2 went free -> paid. Apps: Paste, CopyClip. (high)
- **Phones home / shows ads.** CopyClip 2 displays ads for the developer's other apps ("cheap and unprofessional") and connects to the internet, which users find unacceptable for clipboard data. App: CopyClip. (medium)
- **Misses rapid successive copies.** Quick back-to-back copies don't all land; user must re-copy. CopyClip reviews. Apps: CopyClip, Maccy. (medium)
- **Fragile iCloud / cross-device sync.** Items don't appear on the other device; fixes require disabling/re-enabling, keeping the app open for minutes, or resetting iCloud data (Paste has a dedicated "sync doesn't work" help page). App: Paste. (high)
- **No working iOS counterpart.** Can't sync Mac <-> iPhone; Pastebot users lament the dropped iPhone version. Apps: Pastebot, Maccy, CopyClip, Flycut. (medium)
- **Auto-paste fails into Spotlight / Electron / secure fields.** Keystroke injection doesn't reach the target. Maccy #754. App: Maccy. (medium)
- **Accessibility permission silently drops.** After updates/OS upgrades the permission resets and auto-paste dies with no clear reason; the standard support answer is "re-add it under Privacy & Security > Accessibility." Apps: Maccy, Paste, Pastebot. (high)
- **Duplicate entries clutter history.** Repeated identical copies create many rows instead of de-duping. Maccy #1257. App: Maccy. (low)
- **Broken/weak search.** Paste's Mac search "was broken... for 3 months"; trouble finding older items. Apps: Paste, Alfred. (high)
- **Two clipboard tools corrupt each other.** Running alongside another manager causes double-capture / wrong paste. Maccy #429 (conflict with Pure Paste). Apps: Maccy, Pure Paste. (medium)
- **History/settings don't persist across restart.** Flycut loses state after reboot unless told to save after each clip. App: Flycut. (medium)
- **Bulky / unattractive UI** for a tool that should be fast and invisible; awkward organizing of older items. Paste reviews. App: Paste. (low)
- **Poor image / non-text handling.** No previews, DB bloat, non-searchable; OCR modes cost real CPU (Raycast exposes a "Fast" low-CPU tradeoff). Apps: Alfred, Raycast, Maccy. (medium)
- **Low/hardcoded history caps silently drop clips.** Maccy's default 999-entry limit loses older items. Maccy #1097/#1028. App: Maccy. (medium)

### Windows app complaints

Real, specific problems users report with Windows clipboard managers (Ditto, ClipboardFusion, ArsClip, 1Clipboard, Windows 11 Win+V, ClipAngel, Comfort Clipboard). Sourced from SourceForge/GitHub issue threads, vendor support forums, Mozilla Bugzilla, WindowsForum, and security writeups. A few items are marked uncertain where only general (not app-named) evidence was found.

**Paste mechanics (the core job, and it breaks)**
- Auto-paste fails: choosing a clip does not paste into the previously focused field; user must still hit Ctrl+V manually (Ditto issue #245, ArsClip). Defeats the entire purpose of a picker.
- Wrong character pasted: Ditto sometimes injects a literal "v" instead of the selected clip (Ditto issue #649) - a keystroke-simulation/focus race.
- Picker opens behind other windows / steals or loses focus, so the hotkey summons a panel you cannot use or that hides the target app (Ditto).
- Paste hits the wrong target or overwrites instead of inserting: ArsClip reported replacing an entire filename rather than inserting at the caret, and a 3.1.2 bug that stopped file copy/paste in Explorer.
- Fragile per-app paste: works in most apps, breaks in specific ones (Firefox) because the tool can't detect the focused field - dev confirmed "Firefox fails to correctly report the text window with focus" (Mozilla Bugzilla 643378, ArsClip).

**Windows 11 Win+V native history limits**
- Hard cap of 25 items; oldest are silently dropped once full - "the 25-item limit is the biggest complaint."
- History wiped on every reboot unless each item was manually pinned.
- Intermittently misses items on fast successive copies, large/complex data (Excel), and delayed-rendering sources.
- The Clipboard User Service can crash/hang (whole history inaccessible) and OS cumulative updates periodically break it (24H2 cycle).
- 4 MB per-item cap; oversized items may fail to save.

**Performance / stability**
- ClipboardFusion: multi-minute high-CPU spike at boot/login (up to ~40% for 3-4 min, attributed to sync).
- ClipboardFusion: memory leak / unbounded RAM growth over a session (reports of 1-3 GB); vendor added forced GC, confirming it.
- ClipboardFusion: settings/tray UI unresponsive for 20+ seconds, must force-close and relaunch.
- ClipboardFusion: random crashes several times a day; one cause was the Triggers feature, which the vendor later disabled by default.

**Data integrity / launch**
- Ditto: an update (3.21.134) silently emptied the entire history and broke capture so nothing new was recorded (critical data loss).
- Ditto: clip ordering corrupts - newest no longer on top, history out of chronological order - tied to sticky/pinned-clip settings going wrong.
- Ditto: won't launch, or opens once then refuses to reopen ("Ditto won't load?").

**Hotkeys**
- "Failed to set hotkey" on every startup because another app grabbed the global shortcut; configured hotkey conflicts or silently stops working (ArsClip; general across managers and the OS).

**Security / privacy (cross-cutting, severe)**
- History stores passwords, 2FA codes, crypto addresses, and private URLs in plain text indefinitely - visible to anyone at the machine via the picker and scrapable by clipboard-watching malware (all managers; few exclude sensitive fields by default).
- Enabling sync uploads private clipboard contents to a vendor cloud: Microsoft servers (Win+V) or a personal Google Drive (1Clipboard).

**Sync**
- Cloud/network sync stalls, silently stops, or never updates the other device ("Cloud Clipboard not updating or syncing in Windows 11"; 1Clipboard is self-described beta; ClipboardFusion's boot CPU spike is itself sync-related).

**Maintenance / fidelity (some uncertain)**
- Abandonware / beta-forever risk: 1Clipboard (beta, stalled), and long-dormant freeware like ArsClip and Comfort Clipboard, so OS-update breakages go unfixed. (Inferred from project activity.)
- Rich-text/HTML formatting dropped or mangled on round-trip, especially in managers that advertise RTF/HTML capture (ClipAngel, Comfort Clipboard). (General format-loss evidence; specific app-named thread not found - uncertain.)
- Large items/images bloat the on-disk history DB and slow the app, or are truncated by per-item size caps (Win+V 4 MB cap confirmed; local-DB bloat partly inferred).

Sources: Ditto SourceForge discussions and GitHub issues (#227, #245, #649, "won't load"); ClipboardFusion vendor forum (boot CPU, memory after extended usage, crashes, unresponsive settings); ArsClip Mozilla Bugzilla 643378 and portablefreeware notes; WindowsForum and Microsoft Q&A on Win+V (limits, missed clips, service crashes); GhostVolt / cybernews / Mozilla security blog on clipboard plaintext exposure; TheWindowsClub on Cloud Clipboard sync; 1clipboard.io (beta/Google Drive sync).

### Linux clipboard pain

Concrete problems users report with Linux clipboard managers (CopyQ, GPaste, Klipper/Plasma, Clipman, Diodon, cliphist/clipcat, wl-clipboard) and the underlying X11/Wayland clipboard stack. Evidence is from upstream GitHub/GitLab/Launchpad/KDE/Mozilla bug trackers and distro forums.

- **Clipboard vanishes when the source app closes (critical).** X11 only stores a reference and fetches the bytes at paste time; Wayland destroys the data offer with the window. Closing the source app (or terminal) loses the copy. Forum consensus: "by design for many years." This is the reason users install a manager just to get baseline copy/paste. (Ubuntu/Arch/Mint forums)
- **Wayland security model locks managers out (critical).** Managers need `wlr-data-control`/`ext-data-control`; GNOME Mutter refuses to implement it, so cliphist/clipman/wl-clip-persist simply don't work on GNOME Wayland. The protocol is now deprecated too, fragmenting support. (Mutter issue #524, GNOME Discourse)
- **CopyQ silently captures nothing on Wayland (high).** Empty history, Ctrl+C not registering, "Failed to activate Wayland clipboard" / "Activating Wayland clipboard took 5000 ms" / "Null data in clipboard". Fine on Xorg on the same machine. (CopyQ #1243, #2847, #2798, #2963, #3587)
- **PRIMARY vs CLIPBOARD confusion (high).** Two independent clipboards; Ctrl+C sometimes lands only in the middle-click buffer, or paste returns the wrong selection. Trips up newcomers and managers alike. (shallowsky.com; GPaste Wayland reports)
- **Middle-click paste breaks after upgrades (medium).** GNOME flipped the `middle-click-paste` default true→false (Jan 2026); select-then-middle-click muscle memory died on Fedora 44 with no warning. (jmainguy.com)
- **CopyQ pastes into the wrong window on Wayland (high).** Showing its menu steals focus and it can't refocus the real target, so paste hits the last X-window or nowhere; auto-paste doesn't work on Wayland at all. (CopyQ #2136, #3387, #2557)
- **Global hotkeys to open the manager don't fire on Wayland (high).** No global hotkey grab; users must rebind in DE settings or lose the workflow. (CopyQ FAQ; dev.to quick-fix article)
- **Managers downgrade rich content to plain text (medium).** Reading only `text/plain` drops HTML/RTF/image MIME types the source offered; formatted browser copies paste as bare text. (linuxvox, clipboardinspector)
- **Browser image copies arrive as raw HTML, not pixels (medium).** Sync setups grab `text/html` instead of `image/png`; cliphist needs a separate `wl-paste --type image --watch` watcher to keep images. (Arch forum; cliphist docs)
- **Password managers leak secrets into history (high).** Copied passwords persist in searchable history; managers historically ignored `x-kde-passwordManagerHint`; clear requests unreliable. (Bitwarden community; Klipper 409366; Clipman open "Honour password hint" 2026/01)
- **KeePassXC auto-clear defeated by the manager (high).** Secret wiped from the live clipboard but remains in the manager's history; on Plasma 6 the live clear sometimes doesn't even happen. (keepassxc #10634, #3008)
- **Diodon records nothing on Wayland (high).** Runs and shows an indicator but history stays empty; Wayland-native copies (GNOME Terminal) never captured; source-app detection broken. Heavy X dependence. (Launchpad #1727042)
- **Diodon menu can't take focus / instant-paste crashes on Wayland (medium).** Arrow-key nav and ESC don't work in the popup; instant paste crashes and must be disabled. (Launchpad #1995847)
- **GPaste GNOME Shell integration breaks across releases (medium).** After a GNOME bump the panel integration stops, shows one entry, or the recommended extension is abandoned; must run `gpaste-client dr` after upgrades. (GNOME Extensions; Manjaro/Arch threads)
- **GNOME ships no built-in clipboard history (medium).** Forces reliance on third-party Shell extensions that break on every GNOME version bump. (GNOME Discourse #20308; Fedora Discussion)
- **wl-clip-persist (the standard Wayland fix) randomly fails to copy (medium).** Intermittent empty pastes; users remove it to restore reliability. (omarchy #1062)
- **CopyQ freezes/hangs on Wayland (medium).** ~1s freeze pasting right after select; "Freezes When Accessing Own Clipboard Data in Wayland"; under XWayland monitoring stops when the main window closes. (CopyQ #3125; known-issues docs)
- **CopyQ high CPU/RAM on large data, images, multi-monitor (medium).** 1-core-100% + 1.7GB after large text; "bad allocation"/300s+ timeouts; ~1 GiB/s growth with a second screen; freezes on large Excel copies. (CopyQ #3096, #1286, #1131, #795)
- **X11/Wayland/XWayland clipboard split (high).** Copies don't cross the toolkit boundary; a manager in one mode can't see the other's clipboard. (Diodon docs; CopyQ pasting to "previously active X-window")
- **Clearing history doesn't clear the live clipboard (high).** History shows empty but Ctrl+V still pastes the old (possibly sensitive) value. (KDE bug 409366)
- **Clipboard ownership feedback loops (medium).** Persistence/sync helpers that re-assert clipboard ownership race with apps, causing "clipboard stops working after a while" or dropped copies. (Mozilla 1631061, 1504689; Sway #5480)
- **Reliability requires stacking compositor + manager + persist daemon (medium).** Sway/Hyprland users must chain wlroots support, a manager, and wl-clip-persist; any layer breaking breaks the clipboard. The fragile multi-component stack is itself the anti-pattern. (Manjaro Sway thread; Sway #5480; Hyprland wiki)
- **GPaste rejects some content (low).** gpaste-client refuses text with certain special characters, so valid copies silently never enter history. (GPaste #202)

### OSS GitHub issues deep-dive

Recurring bug reports and feature requests mined from the GitHub trackers of Maccy, CopyQ, Ditto, GPaste, Flycut, Clipy, clipcat, and cliphist (plus 1Password/security community threads). Severity reflects how badly it breaks the core promise of a clipboard manager.

- **Auto-paste silently breaks after macOS updates** [critical] (Maccy, Flycut, Clipy). Selecting an item retrieves the clipboard but never types it; Maccy 2.5.1 on macOS 15.7 even shows a *notification* of the text instead of pasting (Maccy #1230, #882, #1280, #482, #148; Flycut #156/#187/#250; Clipy #559). Reinstall + re-permission often does not help.
- **Accessibility permission is fragile** [high] (Maccy, Flycut, Clipy). The prompt never appears, gets revoked on update/OS upgrade, or the checkbox is on but a corrupted TCC database still blocks paste (Maccy #159, #119, #835, disc #980). Users repeatedly remove/re-add the app.
- **Wayland fails to capture or loses content** [critical] (CopyQ, cliphist, GPaste, clipcat). Nothing is saved unless the manager window is open, content vanishes when the source app closes, and copying several things in a row keeps only the last (CopyQ #3587, #1243; Wayland-design limitation per Hyprland wiki, needs wl-clip-persist).
- **Paste lands in the wrong window** [high] (CopyQ, cliphist). On Wayland it pastes into the previously-active/background window instead of the intended target (CopyQ #2136). Silent wrong-target is worse than a no-op.
- **Invoking the paste window crashes the receiving app** [critical] (CopyQ). ~30% of the time on sway, sometimes pulling down multiple apps (CopyQ #1455; X11 I/O errors #3151).
- **Images/large binaries blow up memory and CPU** [high] (GPaste, Clipy). Copying an image consumes all RAM, pins a core, and can loop re-adding the same screenshot (GPaste #40, #156, #262; Launchpad #1714257). Many users disable image support; Clipy #319 doesn't store images at all.
- **Performance collapses with large history** [high] (CopyQ). Search, keyboard selection, and clearing the filter lag or freeze past ~1000-3000 items or with large single-line items (CopyQ #1238, #1202, #804, #1131). Maintainer confirms the storage/load design is the root cause.
- **Entire history lost** [critical] (Maccy). After an update, reboot, or glitch the whole history (including pins) disappears or goes blank with no recovery (Maccy disc #1122, #422, #1316 - still seen Jan 2026).
- **Pinned items are unreliable** [high] (Maccy, CopyQ). Pins vanish after OS update, switch order on reboot, can't be reordered, and pasting an all-pinned tab can shuffle/crash the app (Maccy #1220, #998, #989; CopyQ #765).
- **Passwords leak into history** [critical] (most apps). Managers don't honor, or sources don't set, the concealed/transient pasteboard flag, so credentials sit in plaintext history (1Password v8 extension copies are *not* marked concealed; "passwords no longer hidden from clipboard history"). Concealed-flag handling is inconsistent across managers.
- **No auto-scrubbing of secrets** [high] (most apps). History retains passwords, 2FA codes, and banking data indefinitely, usually unencrypted on disk, with no sensitive-type auto-expiry (Ctrl blog, Packetlabs, Mozilla security blog).
- **Rich-text/HTML/image captured instead of plain text** [medium] (cliphist, Maccy, CopyQ). cliphist saves the text/html rep of a copied Discord image, so recall pastes an HTML tag, not the image; recurring "pastes formatting when I wanted plain text" complaints. Wrong MIME selection corrupts the recalled item.
- **Global hotkey conflicts / swallowed** [medium] (Maccy, Flycut, CopyQ). Terminals/IDEs/OS eat the shortcut, or hotkey+Enter opens the picker but never pastes (Warp #2806; Flycut #89, #311).
- **Cross-machine sync is fragile** [high] (Ditto). Peer sync silently stops, and the DB on Google Drive/Dropbox produces conflict files and locking/corruption (Ditto docs/SourceForge).
- **Capture stops when another app grabs the clipboard chain** [high] (Ditto, CopyQ). RDP, VMs, AutoHotkey, or a second manager kill capture while the tray icon stays alive recording nothing (Ditto troubleshooting; CopyQ #3125 1s freeze on Wayland).
- **Blank/empty history items** [medium] (CopyQ, Maccy). Items appear with no visible content and can't be pasted (CopyQ #1728; Maccy #921 - some copies don't show).
- **Clumsy Linux picker integration** [medium] (cliphist, clipcat). cliphist has no built-in picker (pipes only) and its TAB-delimited list collides with fzf/rofi/fuzzel column splitting, so naive configs recall the wrong column (cliphist README).
- **Stale/abandoned maintenance re-breaks on each OS** [high] (Clipy, Flycut). Clipy went ~8 years between releases; snippet editor crashes, won't launch, fixes lag months (Clipy #217, #450, #559; Flycut won't-save-preferences reports). Every macOS bump re-breaks them.
- **Duplicate entries clutter history** [low] (Maccy). Repeated copies create identical entries instead of de-duplicating/repositioning; no built-in dedup (Maccy #1124, #1257).
- **No bulk select/copy/paste/delete** [low] (Maccy). Cleanup and multi-snippet assembly are one-at-a-time (Maccy #750, #239).

### Security & privacy failures

Clipboard managers sit on the most sensitive data path on a device, everything a user copies, including passwords, 2FA codes, API keys, card numbers, and crypto addresses, and a recurring pattern is that they persist, sync, or expose that data far more than users expect. Concrete, evidence-backed failures:

- **Plaintext / unencrypted history database.** Ditto, CopyQ (default), Windows clipboard history, and Samsung Clipboard store clips in a readable file. Ditto issue #657: "stores the data in an unsecured SQLite database that is open to anyone that has access to the system", a corporate security team banned it over this. CopyQ docs confirm tabs are "stored... unencrypted (unless Encryption is enabled)". **(critical)**
- **"Deleted" clips are still recoverable.** In Ditto, secrets reportedly remain in Ditto.db (visible via hex editor / SQL browser) even after delete + compact + repair. SQLite freelists, WAL/journal, and unallocated pages retain deleted records, recoverable with tools like FQLite/Belkasoft. **(high)**
- **Sensitive copies synced to the cloud, sometimes by a hidden default.** Windows cloud sync uploads copied text (incl. card/bank details) to Microsoft servers; users report it was "on and I didn't know". Clearing local history doesn't remove roamed copies. **(high)**
- **Clipboard sniffing by other apps.** The shared global clipboard lets any installed app read what you copied. Mysk (2020) found ~50 iOS apps (TikTok, AccuWeather, Truecaller, Overstock) reading it on launch; iOS 14 exposed TikTok reading it "every few seconds". Android <10 allowed background clipboard reads. **(critical)**
- **Clipper malware hijacks copied crypto addresses.** Android Play Store clippers (fake MetaMask), Laplas Clipper, and desktop clippers silently swap copied wallet addresses for lookalikes before paste, copy-paste workflows are the attack surface. **(critical)**
- **Password managers don't auto-clear the clipboard.** Bitwarden defaults to "Never", Keeper similar, so a copied credential lingers in the clipboard (and any manager's history) indefinitely; Android users report it persists even with a timeout. **(high)**
- **Concealed/transient pasteboard flags ignored.** Legacy macOS managers (Flycut, Jumpcut) and CopyQ pre-fix captured secrets the OS marked not-to-store. CopyQ #1744: KeePassXC passwords leaked into history until a fix honored concealed-type markers. **(high)**
- **OCR-on-everything turns copied images into searchable secrets.** Clipory runs OCR on every copied image ("a screenshot of a receipt... becomes fully searchable text"); KeepClips/ClipZ similar, indexing IDs, cards, and tokens. **(medium)**
- **History exposed with no authentication gate.** Most managers (Ditto, CopyQ, Win+V) show full history with no unlock; biometric-locked history is sold as a differentiator (SaneClip), implying open-by-default is the norm. **(high)**
- **Telemetry/analytics from an app over your most sensitive data.** ClipboardFusion sends "fully anonymous telemetry" (OS, resolution, features) by default, opt-out, not opt-in. **(medium)**
- **Unverifiable trust in closed-source sync vendors.** Paste/PastePal/PasteNow claim "private iCloud only, we never see it", but as closed source this can't be independently audited. **(medium)**
- **Forensic + memory recovery of copied secrets.** Windows ActivitiesCache.db logs clipboard payloads (Base64) with timestamps; memory forensics recovers clipboard contents; managers add a second persistent copy. **(high)**
- **Network/LAN sync can be unencrypted or MITM-able.** Ditto's network sync over an unencrypted local DB widens attack surface; partly uncertain on per-config in-transit defaults. **(medium)**
- **No default exclusion list.** Capturing from every source (password managers, banking apps, terminals) unless the user finds and configures ignore rules; safe filtering is opt-in (Paste, Maccy advertise it). **(medium)**
- **Residual data after uninstall / "clear all".** DB, WAL, and config dirs left behind; what users think was a wipe survives on disk. **(medium)**
- **Wayland isolation defeated to keep history.** Clipboard managers on Wayland use privileged listeners to monitor the clipboard, re-opening the cross-app leakage the compositor isolates against. **(low)**
- **Recall-style centralized history is a malware magnet.** XDA: an unencrypted clipboard history is the same risk people criticize in Microsoft Recall, a single searchable store of everything copied, ideal for infostealers. **(high)**

Self-rating: 8/10 - problems are concrete, attributed to named apps, and backed by primary sources (GitHub issues, vendor docs, Mysk research, ESET/BleepingComputer, forensic writeups). Weaknesses: a few items (Ditto network-sync encryption defaults, exact per-app exclusion defaults) are partly inferred rather than verified against current source/config, and direct verbatim user quotes are limited to the Ditto issue and Bitwarden threads rather than scraped at scale.

### Performance & reliability failures

Concrete, evidence-backed problems users report with clipboard managers under the lens of performance and reliability. Most evidence is from GitHub issues, SourceForge threads, and Microsoft Q&A; a few items are cross-app patterns marked "generic".

- **Linear memory leak, never released (Maccy):** Memory "gradually increases... grows linearly and there is no sign of release"; only fixed by quitting and relaunching. (GitHub #384, echoed in #937/#1240/#1294)
- **Clearing history does not free memory (Maccy):** 700MB-1GB RAM after a day of tiny text copies; after clicking Clear, RAM stays high and "grows even more after a while". (#1077)
- **Idle CPU spinning / battery drain (Maccy):** ~45% CPU in Activity Monitor and top 12-hour energy consumer; restart only temporarily helps. (#993)
- **Whole history rendered on every open (Maccy):** UI fetches and renders all rows regardless of relevance; 30k-50k items cause freezes and sluggish scrolling. Bottleneck is rendering, not storage. (#1097)
- **Slow search at scale (Maccy):** Noticeable lag between typing and results filtering even at ~999 items. (#189)
- **Unbounded SQLite growth from lingering deleted clips (Ditto):** "24 MB of clips in 1.7 GB database"; users report 14GB, 32GB, even a 216GB ditto.db. (SourceForge Help threads)
- **Must manually VACUUM/compact or it never shrinks (Ditto):** External SQLite compaction yields 2.5GB -> 5MB and 450MB -> 400KB; built-in Compact "effectiveness varies", forcing third-party tools/scripts.
- **Database corruption / freeze when the file grows too large (Ditto):** Documented recovery is to rename Ditto.db and let a fresh one be created, losing history.
- **Raw image (Retina TIFF) storage explodes disk/RAM (Ditto, Maccy, generic):** A single screenshot is 10-30MB of TIFF; raw-storing managers reach hundreds of MB/day, gigabytes/week unless they convert to PNG/JPEG.
- **Large copy = 100% CPU, ~1.7GB RAM, then crash (CopyQ):** Copying 700MB+ text triggers repeated "bad allocation", "Data access took too long" (323s), and the monitor subprocess crashes. (#3096)
- **Filesystem polling instead of notifications (CopyQ):** Wakes every ~5s burning ~1.5 cores once ~1,600 synced files accumulate; worsens as history grows. (#1131)
- **Background CPU runaway (CopyQ):** While idle, sometimes pegs 100% CPU indefinitely until killed. (#1609)
- **Multi-monitor memory blowup (CopyQ):** 7-8 GiB RAM within seconds of starting with a second screen. (#1286)
- **Startup crash loops / version-regression crashes (CopyQ):** Crashes and relaunches several times per second (#1401); v12.0.1 crashes instantly on Intel macOS (#3249); crashes without warning (#1311).
- **Manager holding clipboard lock causes global paste lag (generic, Windows):** Keyboard/typing freezes up to a second on copy/paste; disabling clipboard history fixes it for many. (Microsoft Q&A, forums)
- **Built-in history silently drops data (Windows Clipboard History):** Hard 25-item cap, 4MB/item, large files/images not saved, 30s delayed-render timeout drops complex data, and rapid Ctrl+C bursts outrun the service. (MS Learn)
- **Abandonware / no Apple-Silicon build (Clipy, Flycut):** Clipy is Intel-only Rosetta and "will completely stop working once macOS 28 ships" (#603); Flycut "not working in macOS 15.1" (#250), "stopped working altogether" (#219).
- **Manager degrades the source app (generic):** After a copy, the source app slows and uses more memory because the manager re-queries/transforms clipboard formats. (Inkscape GitLab #2722)
- **SwiftData/Core Data startup migration + inline blobs (Maccy, generic):** Large-store migration delays launch; storing images inline rather than external bloats memory and slows every save.
- **Login-item reliability (Flycut):** Launch-on-login silently unchecks after OS upgrades, so history capture quietly stops. (#206)

Cross-cutting root causes seen repeatedly: (1) polling the clipboard/filesystem instead of event notifications, (2) loading/rendering the full history into the UI rather than paginating, (3) keeping deleted/oversized data in an SQLite store that is never auto-vacuumed, (4) storing raw uncompressed images, and (5) holding clipboard locks or doing synchronous format conversion on the copy/paste hot path.

### Business, UX & trust failures

Concrete, user-reported business, UX, and trust failures in clipboard-manager apps. Evidence is drawn from App Store reviews, MacPowerUsers and Windows forums, GitHub issues, vendor support docs, and review roundups. Apps named are illustrative examples of each anti-pattern, not exhaustive.

**Pricing, subscriptions & paywalls**
- **Subscription conversion of a one-time-purchase app** (Paste): moved to recurring billing with no perpetual option; users on the MacPowerUsers thread "Paste joins the subscription bandwagon" called it a dealbreaker for "just a clipboard manager" and migrated to Copy'em Paste. [high]
- **Creeping price increases with no perpetual-license escape** (Paste): review roundups flag "subscription pricing that keeps creeping up, with no one-time purchase option" as the headline downside. [high]
- **Clipboard history paywalled as the premium upsell** (Alfred, Raycast): Alfred locks clipboard history behind the ~$43 Powerpack; Raycast gates cross-device clipboard sync behind paid Pro. The one feature most users want is the upsell. [high]
- **Bundle-only distribution with hard lock-in** (Paste via Setapp): no standalone perpetual purchase; per Setapp support, stop paying and "apps will still be installed... but you won't be able to use them" - you lose access to your own clipboard history. [high]
- **Paid 2.0 rewrites that force re-purchase** (CopyClip 2): shipped as a separate paid listing from the original free app, pushing existing users to buy again for updates. [medium]
- **Charging later for the app's signature free feature** (ClipboardFusion): AlternativeTo review notes it "started to charge to use macros, which was its special place." [medium]
- **Silent Pro-trial dark pattern** (ClipboardFusion): same installer auto-grants a 30-day Pro trial; users build workflows on Pro features that then vanish at expiry. [medium]

**Intrusive monetization & trust**
- **In-app ads / cross-promos inside a sensitive utility** (CopyClip): App Store reviews say it "shouldn't randomly display ads for other apps the company sells, as it looks cheap and unprofessional," and a clipboard app "should not attempt to phone home." [high]
- **No encryption of stored history** (Ditto, Samsung clipboard, many): copied passwords/keys/bank info sit in a plaintext database; "Ditto is one such case where encryption could be added... but isn't there yet"; Samsung "saves passwords in plain text with no expiration." Malware like StilachiRAT actively scrapes clipboard history. [critical]

**Abandonment & maintenance**
- **Abandoned apps trusted with your clipboard** (Clipboard++, open-source "Clipboard"): Clipboard++ "no update for one year and refusing to respond to messages"; the OSS app's last release was 2020 - "no security or macOS-compatibility fixes are coming." [critical]
- **Slow/uncertain update cadence on a sensitive tool** (Pastebot): "last update shipped in 2024... worth weighing for something you'll trust with sensitive data." [medium]
- **Perpetual-beta shipped as finished** (1Clipboard): markets a universal cross-device clipboard while "currently in beta, so there might be a few unknown issues." [medium]

**Sync, defaults & platform lock-in**
- **Mac-only lock-in marketed as complete** (Maccy, Paste Pal, Raycast clipboard): "no iPhone, no iPad, no Vision Pro, no sync" - users discover the limit after adopting the tool. [medium]
- **Cross-device sync silently fails with no diagnosable error** (Windows 11 Win+V, Copied): users report copy never crossing despite same account/sync-on/restart, "no errors shown"; Copied's iCloud sync called "finicky." [high]
- **Undocumented sync limitation, text-only** (Windows 11): silently drops images/rich formats users expect to carry over. [medium]
- **History silently wiped after updates or by background cleaners** (Windows 11, old third-party managers): panel "goes blank after an update"; optimizers "clear clipboard history automatically"; outdated managers "silently hijacking the native clipboard." [high]

**Reliability, performance & onboarding UX**
- **Unbounded memory growth / slowdown with history** (CopyClip): "memory footprint keeps increasing... gets slower when there are many entries." [medium]
- **Hotkey conflicts breaking core paste** (CopyClip, CopyQ): "hotkeys don't work well... particularly Command-X," clashing with other extensions. [medium]
- **Blocked launch via signing/notarization gaps** (CopyQ): "CopyQ is damaged or CopyQ cannot be opened" dialogs stop first use. [medium]
- **Settings silently reset/corrupted** (CopyQ): tab properties lost on rename until a fix landed. [low]
- **Core capture silently stops on some platforms** (CopyQ on Wayland): records nothing unless the app is opened, storing only the latest item - silent data loss. [high]
- **Stability regressions tied to OS updates** (CopyClip): post-macOS-update failures "several times a day, requiring complete removal and reinstallation." [medium]
- **Obstructive UI glitches** (CopyClip 2): hover popovers that "don't disappear even when closing the list." [low]
- **Scope creep over simple paste-back** (Boards, Paste pinboards): "overkill" for straightforward pasting; pinboard system "dated compared to newer shortcut systems." [low]

Note: a few claims are vendor- or roundup-sourced rather than first-person review quotes (Pastebot cadence, 1Clipboard beta status, Setapp lock-in mechanics); these are marked by their evidence and are reasonably reliable but not direct user complaints. Where I could not verify a specific user quote I described the nature of the issue rather than inventing wording.