# vbuff - Competitive Analysis

## 1. Landscape Overview

Clipboard managers are a mature but deeply fragmented utility category. The market splits cleanly along several axes, and almost no product covers more than one or two of them well.

**Functional categories:**

- **OS-built-in history.** Windows 11 Clipboard History (Win+V) and Apple Universal Clipboard ship for free, require zero install, and cover the casual majority. They are deliberately shallow: hard item caps, no organization, no scripting.
- **Free local-first managers.** Ditto (Windows), CopyQ (cross-platform), and the entire Linux ecosystem (GPaste, Klipper, cliphist, etc.). These win on privacy, price, and power, but lose on polish and sync.
- **Commercial polished managers.** Paste (Apple-only), ClipboardFusion, Comfort Clipboard Pro. They compete on UI, formatting control, macros, and sync, charging either a one-time license or a subscription.
- **Cross-device sync tools.** CrossPaste, KDE Connect, 1Clipboard, Microsoft Cloud Clipboard. The clipboard becomes a sync transport across machines; architectures range from vendor cloud to LAN-only to "your own Google Drive / iCloud."
- **Power-user / adjacent automation.** Text expanders (Espanso, TextExpander, PhraseExpress, aText) and launcher/macro platforms (Raycast, Alfred, Keyboard Maestro) where clipboard handling is one module inside a larger engine.

**Platform fragmentation is the defining problem.** The best-in-class tool on each platform is different and usually *only* on that platform: Ditto on Windows, Paste on macOS, Klipper/GPaste on Linux. A user who works across macOS + Windows + Linux cannot carry one mental model, one keybinding scheme, or one synced history across all three. The few apps that span all desktop platforms (CopyQ) do so with a dated, scripting-heavy UI; the few that sync everywhere (CrossPaste) are young and LAN-bound.

**Free vs paid vs OS-built-in.** The free open-source tier is crowded and genuinely powerful (Ditto, CopyQ, Espanso), which compresses what anyone can charge for. Paid tools survive only by offering something the free tier and the OS do not: reliable cross-device sync (Paste, ClipboardFusion Pro), team snippet libraries (TextExpander), deep automation (Keyboard Maestro), or polish + support (Comfort Clipboard Pro). The OS-built-in option keeps raising the floor (Win+V now does cloud sync, emoji, pinning), which steadily erodes the bottom of the paid market.

## 2. Comparison of Key Apps

| App | Platforms | Price | Sync | Formats | Search | Snippets | Scripting | Standout |
|-----|-----------|-------|------|---------|--------|----------|-----------|----------|
| **Ditto** | Windows | Free (GPL) | P2P LAN (AES) | Text, image, HTML, files, RTF, custom | Regex, as-you-type | Pinned clips | No | De facto free Windows manager; local-only, no cloud |
| **CopyQ** | Win, macOS, Linux | Free (GPLv3) | None (file-dir hack) | Text, HTML, image, custom MIME | Full-text, incremental | Tabs + notes/tags | Full JS engine + CLI | Most powerful free cross-platform; scriptable |
| **Paste** | macOS, iOS, iPadOS | $3.99/mo or $29.99/yr | Private iCloud | Text, image, link, file | Full-text + image OCR | Pinboards | No | Polished Apple-only with elegant pinboards |
| **ClipboardFusion** | Win, macOS, iOS, Android, browser | Freemium; Pro $19-$899 lifetime | Vendor cloud (Pro) | Text, image, Unicode | History search | Pinned items | C#/VB.NET macros | Programmable macros + triggers |
| **CrossPaste** | Win, macOS, Linux, Android, iOS | Free (AGPLv3) | LAN, E2E encrypted | Text, RTF, HTML, URL, image, file, color | Full-text | None core | No (OCR optional) | Broadest true cross-platform; local-network sync |
| **Win+V (built-in)** | Windows 10/11 | Free (OS) | MS account cloud (text only) | Text, HTML, bitmap | Basic filter | Pinned items | No | Zero-install OS-native, emoji/GIF picker |
| **Apple Universal Clipboard** | macOS, iOS, iPadOS | Free (OS) | Proximity E2E (no history) | Text, image, video, files | N/A | N/A | No | Zero-config copy-here-paste-there |
| **Comfort Clipboard Pro** | Windows | $19.95 one-time | None | All formats, multi-format/item | Search/filter | Templates | Per-app rules | 448-bit Blowfish encryption + crash recovery |
| **PasteBar** | macOS, Windows | Free (CC BY-NC) | None (local only) | Text, image, file, code | Search | Collections, templated clips w/ form fields | 30+ paste ops | Open-source power organizer, fully local |
| **GPaste** | Linux (GNOME) | Free (GPL/BSD) | None | Text, file paths, image | Panel search | None | CLI client | GNOME-native daemon + Shell extension |
| **KDE Klipper** | Linux (KDE) | Free (GPL, bundled) | None | Text, image | Type-to-search | None | Regex actions | Default Plasma manager; QR-code generation |
| **Espanso** | Win, macOS, Linux | Free (GPLv3) | None (git/Dropbox) | Text, image (expansion) | ALT+Space bar | Full expander w/ forms | Shell/script exec, regex | Privacy-first cross-platform text expander |

*Adjacent power-user tools worth noting outside the table: Raycast (free core + $8/mo Pro launcher with clipboard history), Keyboard Maestro ($36 macOS automation engine with named clipboards), TextExpander (subscription team snippet libraries), and cliphist (Wayland-native byte-exact history).*

## 3. Per-Platform Notes

### macOS
The leader is **Paste**: polished pinboards, private iCloud sync across Mac/iPhone/iPad, rich previews, and OCR search inside image clips. Its weakness is that it is Apple-only and subscription-only (which drew backlash when it dropped its perpetual license). **Maccy** (free, minimal) and **CopyQ** cover the free end, while **Raycast** and **Alfred** fold clipboard history into a launcher. Power users reach for **Keyboard Maestro** (named clipboards, automation). macOS is the most commercially healthy platform: users here actually pay.

### Windows
The leader for free is **Ditto** - persistent unlimited history, all formats, regex search, P2P LAN sync, local-only privacy. **Win+V** is the zero-install default that covers casual users (and keeps adding features). Paid options - **Comfort Clipboard Pro** ($19.95, encryption + crash recovery) and **ClipboardFusion** (macros, Pro cloud sync) - survive on features Ditto and Win+V lack. The graveyard is large here: **ClipX** (dead since 2008), **1Clipboard** (dead since ~2016) signal how unforgiving this market is.

### Linux
No single leader; the winner is whatever matches your desktop environment. **GPaste** owns GNOME, **Klipper** ships by default with **KDE** Plasma, **xfce4-clipman** is the Xfce standard, and **cliphist** is the Wayland-native choice for tiling-WM users. **CopyQ** is the cross-DE power option. The entire Linux tier is free and open-source - there is no paid market - and fragmentation here is driven by DE integration and X11 vs Wayland rather than price or features.

## 4. Pricing & Positioning Patterns

- **Free open-source is the anchor.** Ditto, CopyQ, Espanso, and all Linux managers set a high free baseline. This caps pricing power for everyone and means a paid product must justify itself against genuinely capable free rivals.
- **OS-built-in raises the floor.** Win+V and Universal Clipboard ship free, improve yearly, and absorb casual users - so paid tools must target needs the OS structurally won't serve (organization, cross-OS sync, automation, teams).
- **Three paid models coexist:**
  - *One-time/lifetime* - Comfort Clipboard Pro ($19.95), ClipboardFusion Pro ($19-$899), Alfred Powerpack (~$46), Keyboard Maestro ($36), aText (~$30). Popular and well-liked.
  - *Subscription* - Paste ($29.99/yr), TextExpander (~$40/yr+), Raycast Pro ($8/mo). Reliably generates backlash when it replaces a perpetual license (Paste, TextExpander both took heat).
  - *Perpetual + paid maintenance* - PhraseExpress (~€100 + renewal). Seen as confusing/aggressive.
- **Positioning archetypes:** privacy/local-only (Ditto, CrossPaste, PasteBar), polish/ecosystem (Paste), automation/scripting (CopyQ, ClipboardFusion, Keyboard Maestro, Espanso), and teams (TextExpander). Almost nobody positions on "best experience across all three desktop OSes simultaneously."

## 5. Common Weaknesses & Recurring Complaints

- **Dated, utilitarian UI.** The most capable free tools (Ditto, CopyQ, ClipAngel, Klipper, Parcellite) all look old. Power and polish rarely coexist outside Paste.
- **Platform lock-in.** Best-in-class tools are single-platform: Ditto (Windows), Paste (Apple), GPaste/Klipper (one Linux DE). Cross-platform users are stranded.
- **Sync is hard, fragile, or paywalled.** LAN-only by default (CrossPaste, KDE Connect, Ditto), text-only (Win+V), no history (Universal Clipboard), paywalled (ClipboardFusion Pro, Raycast Pro), or dependent on a vendor backend that can die (Clipt shut down in 2023; 1Clipboard abandoned).
- **Privacy vs convenience tension.** Cloud sync routes clips - often passwords, tokens, and PII - through vendor servers, which privacy-conscious users reject. Few tools offer encrypted *and* convenient sync.
- **Steep learning curves on the powerful tools.** CopyQ, Keyboard Maestro, Espanso, and PhraseExpress all gate their power behind scripting or dense config. Casual users bounce off.
- **Subscription backlash.** Paste and TextExpander both alienated long-time users by moving off perpetual licensing.
- **Abandonment risk.** ClipX, 1Clipboard, Clipt, Parcellite/ClipIt are dead or stale. Users have been burned and are wary of new entrants.
- **Wayland gaps.** Many Linux tools are X11-only (Greenclip, Parcellite, clipman); Wayland support is the recurring sore spot.

## 6. Gaps & Opportunities for vbuff

The structural gap is clear: **no product is simultaneously truly cross-platform (macOS + Windows + Linux), genuinely polished, privately synced, and approachable.** Every competitor wins on at most two of those four. That intersection is vbuff's wedge.

| Gap in the market | Concrete bet for vbuff |
|-------------------|------------------------|
| **No polished tool spans all 3 desktop OSes.** CopyQ spans them but is ugly/scripting-heavy; Paste is polished but Apple-only; Ditto is Windows-only. | Ship one genuinely native-feeling, modern UI with identical keybindings and behavior on macOS, Windows, and Linux. Make "same tool everywhere" the headline. |
| **Sync is either insecure, paywalled, LAN-only, or dead.** | Offer **end-to-end encrypted sync that works over the internet, not just LAN**, on by default, with a self-host/local-only option. Beat CrossPaste (LAN-only) and Paste (Apple-only iCloud) on reach; beat vendor-cloud tools on privacy. Never depend on a single backend you can kill (the Clipt lesson). |
| **Privacy and convenience rarely coexist.** | Zero-knowledge E2E encryption as the default posture; clips with passwords/tokens/PII auto-detected and excluded or masked. Market "your clips never leave your control" the way Ditto/CrossPaste do, but with cloud-grade convenience. |
| **Power is gated behind ugly scripting (CopyQ) or expensive Mac-only apps (Keyboard Maestro).** | Provide a **scriptable CLI + transform pipeline** (case change, regex, strip formatting, JSON pretty-print) with a discoverable GUI on top, so casual users get one-click transforms and power users get automation - without learning a DSL. |
| **Snippets/expanders live in separate apps (Espanso, TextExpander).** | Fold **snippets + text expansion into the clipboard manager** so users do not run two tools. Dynamic fields (date, clipboard, cursor), fill-in forms, app-scoped behavior - matching expander depth inside one product. |
| **Linux is fragmented by DE and stuck on X11.** | First-class **Wayland *and* X11 support** plus a clean GTK/Qt-agnostic UI that works across GNOME/KDE/Xfce. Be the cross-DE Linux answer that does not exist today. |
| **OS-built-ins win on zero-friction defaults; third-party tools lose casual users.** | Nail the **zero-config first-run experience** (sensible hotkey, instant history, emoji/GIF parity with Win+V) so vbuff is as frictionless as Win+V on day one, then reveals depth. |
| **Mobile is an afterthought for desktop-first tools.** | Treat **mobile (iOS + Android) as a true peer** in the synced history, not a limited companion - the gap Paste (Apple-only) and most Windows tools leave wide open. |
| **Abandonment fear and subscription backlash.** | Open-source the core (trust + longevity signal, following Ditto/CopyQ/CrossPaste) and monetize optional encrypted sync/teams with a **one-time or fair-subscription** model. Avoid the perpetual-to-subscription bait-and-switch that burned Paste/TextExpander users. |

**The single highest-leverage bet:** be the first clipboard manager that a macOS + Windows + Linux user can install everywhere, sync privately end-to-end over the internet, and actually enjoy using. CopyQ has the reach but not the polish or sync; Paste has the polish but not the reach; CrossPaste has the reach and privacy but not the polish or internet sync. vbuff wins by occupying all four corners at once.

## macOS clipboard managers (detailed)

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
