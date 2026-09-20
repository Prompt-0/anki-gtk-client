# Rookery — A Native GTK4/libadwaita Client for Anki
### Design & Development Specification, v0.1

> "Rookery" is a placeholder codename (a group of crows/ravens — a nod to spaced-repetition mnemonics), used throughout this document instead of "Anki" because **Anki is a registered trademark of Ankitects Pty Ltd**, actively enforced against apps that use the name without being genuinely compatible or authorized (see §2.4). This project *is* genuinely compatible — it embeds Anki's own engine — which is exactly the case where asking Ankitects for naming permission is worth doing. Until/unless that's granted, ship under a distinct name and describe it as "a GTK client for Anki collections."

---

## 0. Summary

Rookery is a **first-class GNOME/GTK desktop client for Anki**, targeting Linux. It is not a clean-room reimplementation of Anki's data model or sync protocol — it **embeds Anki's own Rust core (`rslib`)** as a library dependency, giving it perfect `.anki2`/`.apkg` collection compatibility, the real FSRS scheduler, and a sync client that speaks the actual (versioned, closed) AnkiWeb protocol without reverse-engineering it. On top of that shared core, Rookery builds an entirely new UI in **Rust + GTK4 + libadwaita**, designed to feel like a native GNOME application rather than a ported Qt/web app — because that's the gap the official client (Qt widgets + embedded web views, cross-platform-first) leaves on Linux.

**Scope chosen:** full replacement — deck browser, reviewer, note editor, card browser/search, note-type/template management, stats, import/export, media, multi-profile, and an extension system (see §9, since Anki's existing Python add-ons cannot run in this stack).

**Stack chosen:** Rust, `gtk4-rs` + `libadwaita` (`adw`), embedding `rslib` via a pinned git dependency.

---

## 1. Prior Art & Why This Gap Exists

Searched at spec time: there is no maintained, native GTK4/libadwaita Anki client. The desktop options today are:

| Client | Tech | Linux-native feel |
|---|---|---|
| Official Anki desktop | PyQt5/6 widgets + embedded Chromium-based web views for card content | Functional, cross-platform-generic; not Adwaita/GNOME HIG; Qt theming often clashes with GNOME/Ubuntu |
| AnkiDroid | Java/Kotlin, Android | N/A (mobile) |
| Third-party sync servers (`anki-sync-server-rs`, `ankisyncd`, `anki-cloud`, etc.) | Various | Server-side only, not clients |

This confirms the niche is real and unclaimed, and that the sync-server side of the ecosystem is already comfortable with third parties depending on Anki's own AGPL codebase — which is the model Rookery follows for the *client* side too.

---

## 2. Architecture Decision Record

### 2.1 The core question: how do we talk to Anki's data and sync?

Three options were on the table; here's why "wrap `rslib`" wins decisively.

**Option A — Wrap `rslib` (chosen).**
Anki's own architecture is already a three-tier split: Rust core (`rslib`) → thin language bridge → GUI. The Qt client's bridge is Python (`pylib/rsbridge`, a PyO3-style extension module) talking to `aqt`. Rookery replaces that bridge+GUI tier with a **direct Rust dependency on `rslib`** plus a GTK GUI — no Python in the stack at all. Concretely:

```toml
# Cargo.toml (core crate)
[dependencies]
anki = { git = "https://github.com/ankitects/anki", tag = "25.xx.x" }
```

`rslib` is not published to crates.io (it isn't designed as a public stable API), so it must be pulled as a **git dependency pinned to an exact upstream tag**, matched to a specific Anki release. This is a deliberate, load-bearing constraint — see §2.3.

Benefits: perfect collection-format compatibility, the real scheduler (including FSRS), the real import/export code (`.apkg`/`.colpkg`), the real undo system, and — critically — a sync client that already speaks whatever the current AnkiWeb protocol version is, because it's *literally Anki's sync client*, not a guess at one.

**Option B — Reimplement against the AnkiWeb sync protocol independently.** Rejected. The protocol is undocumented and versioned; Anki's own manual states third-party sync servers "tend to take time to catch up" when the protocol changes and are explicitly "not recommended." A permissively-licensed clean-room client would inherit that same fragility on every Anki release, for no real benefit — and would still have to reimplement the entire collection schema, scheduler, and FSRS math to be useful, which is enormous, error-prone surface area with zero payoff versus Option A.

**Option C — AnkiConnect passthrough.** Rejected as primary architecture (though worth keeping as an *optional* transport, see §2.5). Requires the desktop Anki (Qt) app running in the background at all times, defeats "native client" framing, and inherits AnkiConnect's request/response latency and limited surface (no low-level scheduler access, no custom review UI hooks).

### 2.2 Resulting three-tier architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Presentation tier — rookery-gtk (Rust, gtk4-rs + adw)        │
│  Windows, widgets, WebKitGTK card view, GLib main loop        │
└───────────────────────────┬────────────────────────────────────
                             │ async channel (mpsc) + Arc<Mutex<>>
┌───────────────────────────▼────────────────────────────────────
│  Application tier — rookery-core (Rust, our code)             │
│  - Session/profile manager                                    │
│  - Sync orchestration & scheduling (daemon, retries, backoff) │
│  - Extension host (Wasmtime) — §9                              │
│  - Command layer: typed requests → rslib calls, mapped to     │
│    GObject-friendly async futures                             │
└───────────────────────────┬────────────────────────────────────
                             │ direct fn calls (no IPC — same process)
┌───────────────────────────▼────────────────────────────────────
│  Engine tier — rslib (upstream Anki Rust core, git-pinned)    │
│  Collection, scheduler/FSRS, SQLite storage, sync client,     │
│  import/export, undo stack, search parser, template rendering │
└──────────────────────────────────────────────────────────────┘
```

Because both our app tier and the engine tier are Rust in the same process, there is **no protobuf/IPC hop** the way the Qt client needs (Python ↔ Rust across a process-internal but language-crossing boundary). We call `rslib`'s Rust API directly. This is strictly simpler and faster than what the official client does.

### 2.3 The pinning problem (read this before writing any code)

`rslib` is an internal library, not a versioned public API. Its function signatures, module layout, and even its sync-protocol version constant can change between releases without SemVer guarantees. Mitigations, all mandatory:

1. **Pin to an exact git tag/rev, never a branch.** Bump deliberately, once per Rookery release cycle, never automatically.
2. **A `rslib-shim` crate** (thin, <500 LOC) is the *only* place in `rookery-core` allowed to import `anki::*` directly. Every other crate talks to `rslib-shim`'s own stable trait interface. This isolates the blast radius of upstream API churn to one crate.
3. **Sync-protocol canary in CI**: a scheduled job that does a real sync handshake against a disposable AnkiWeb test account (or a self-hosted sync server pinned to the same rev) on every dependency bump, so protocol drift is caught before release, not by users.
4. **Version compatibility matrix** published per Rookery release: "Rookery 1.4 embeds Anki core 25.xx — collections created/synced with desktop Anki ≥ 2.1.66 are supported."

### 2.4 Licensing & trademark (non-negotiable, read before choosing a license)

- `rslib` and the rest of Anki are **AGPL-3.0**. Statically/directly linking it into `rookery-core` and distributing the compiled result makes **Rookery as a whole a derivative work under AGPL-3.0** — there is no way around this while using Option A, and no reason to fight it. License the entire Rookery repository AGPL-3.0 from commit one. (This also means: if a hosted "sync bridge" web service is ever built around Rookery, AGPL's network-use clause applies and source must be offered to users of that service too.)
- **Naming:** do not ship under a name containing "Anki" without written permission from Ankitects. Their own FAQ singles out apps that used "Anki" in the name while being functionally incompatible (AnkiPro→Noji, AnkiApp→AlgoApp) as the specific harm they act on — Rookery is the opposite case (fully compatible, embeds their code, AGPL-compliant), which is a legitimate basis to *ask* for either a trademark license or at minimum a "not currently objected to" statement before v1.0 public release. Track this as a **hard release blocker**, not a nice-to-have. Fallback stance if permission isn't granted: nominative fair use only ("a GTK client for Anki collections"), never in the app's own name/logo/app-id.
- App ID for packaging must NOT be under Anki's namespace: use e.g. `io.github.<org>.Rookery`, not anything implying `net.ankiweb.*`.

### 2.5 Transport abstraction (why Option C isn't fully dead)

Even though sync goes through `rslib`'s real sync client by default, define a `SyncTransport` trait in `rookery-core` with two implementations: `RslibNativeSync` (default) and `AnkiConnectBridge` (optional, feature-flagged, for the narrow case of a user who wants Rookery as a *review-only front end* while keeping desktop Anki as the editor, without duplicate-syncing). This costs little and meaningfully de-risks the "what if `rslib` pinning becomes unworkable" scenario later.

---

## 3. Technology Stack

| Layer | Choice | Notes |
|---|---|---|
| Language | Rust (2021 edition) | Single language across app + engine tiers |
| GUI toolkit | GTK4 + libadwaita (`adw` ≥ 1.5) via `gtk4-rs`/`libadwaita-rs` | Adwaita widgets (`AdwApplicationWindow`, `AdwNavigationView`, `AdwToastOverlay`, `AdwPreferencesWindow`) for GNOME HIG compliance |
| UI definition | Blueprint (`.blp`) compiled to `.ui` via `blueprint-compiler` | More readable than hand-written GtkBuilder XML; used by most modern GNOME apps |
| Card/template rendering | WebKitGTK 6 (`webkit6` crate) | Card templates are HTML/CSS/JS (MathJax, TTS hooks, cloze overlays) — reuse a real web engine rather than reimplementing HTML/MathJax rendering in GTK widgets, same pragmatic choice the official client makes |
| Async runtime | `tokio` (app/engine tier) bridged into GLib's main loop via `glib::MainContext::spawn_local` + `async-channel` | GTK is single-threaded/non-Send; all cross-thread work returns through a channel pumped on the main context |
| Charts (stats) | `plotters` with its Cairo backend, rendered into a `GtkDrawingArea`/`gtk::Snapshot` | Native-drawn, themeable, no web view needed for stats |
| Extension runtime | `wasmtime` (WASM components) | See §9 |
| Credential storage | `libsecret` (Secret Service API / `org.freedesktop.secrets`) via `oo7` or `secret-service` crate | No credentials in plaintext config files |
| Build | Cargo workspace + Meson wrapper for the Flatpak/GNOME-Builder path | Meson is what GNOME's tooling (Builder, Flatpak manifests) expects to drive |
| Packaging | Flatpak (primary), native `.deb`/`.rpm` (secondary) | See §13 |
| i18n | Fluent (`fluent-rs`) for UI strings, reusing Anki's own `ftl/core` where terms overlap (deck/note/card vocabulary) so translations don't diverge gratuitously | |

### Workspace layout

```
rookery/
├── Cargo.toml                 # workspace
├── crates/
│   ├── rslib-shim/            # ONLY crate that touches `anki::*` directly (§2.3)
│   ├── rookery-core/          # session mgmt, sync orchestration, extension host
│   ├── rookery-gtk/           # binary crate: windows, widgets, main()
│   └── rookery-proto/         # shared types between core and gtk (plain structs, no GTK deps)
├── data/
│   ├── ui/*.blp                # Blueprint UI definitions
│   ├── io.github.org.Rookery.metainfo.xml
│   └── icons/
├── po/                          # Fluent/gettext translations
└── flatpak/
    └── io.github.org.Rookery.json
```

---

## 4. Data Layer

- The on-disk collection (`collection.anki2`, SQLite) and `media/` folder are **untouched, standard Anki format** — owned entirely by `rslib`. Rookery never writes SQL directly; every mutation goes through `rslib`'s `Collection`/`Backend` API so undo, sync-usn bookkeeping, and schema migrations stay correct.
- **Undo/redo**: expose `rslib`'s existing undo stack directly in the UI (a single global Ctrl+Z/Ctrl+Shift+Z, plus an `AdwToast` "Undid: Edited note" pattern) rather than building a parallel one.
- **Multi-profile**: mirror Anki's model (one SQLite collection per profile under `~/.local/share/rookery/<profile>/`), with a profile-picker `AdwWindow` shown when >1 profile exists, matching the XDG base directory spec (not `~/.anki2` — that path is desktop Anki's own state and should not be silently shared/mutated by a second, independently-versioned client).

---

## 5. Sync Design — "Always Synced"

The literal ask in the brief. Stock Anki syncs manually or on app open/close; Rookery's differentiator is a **background sync daemon** with real triggers, not just a menu item.

### 5.1 Daemon

- Runs as an async task inside the app process by default (no separate process needed for the common case); optionally installable as a `systemd --user` unit (`rookery-syncd.service`) for "sync even when the main window is closed," using D-Bus activation so it doesn't waste resources when idle.
- **Sync triggers** (all configurable, sane defaults on):
  1. App startup (after profile unlock).
  2. Every N minutes while idle (default 15; incremental/`normal_sync`, not a full sync — only USN-delta changes transfer).
  3. Immediately after a review session ends (so answers are never sitting unsynced longer than necessary).
  4. On network state transition from offline→online, detected via NetworkManager's D-Bus `PrimaryConnectionChanged` signal (`org.freedesktop.NetworkManager`) rather than polling.
  5. Manual "Sync now" — always available via a header-bar button and `Ctrl+Y` (Anki's own shortcut), for muscle-memory parity.
  6. Before quitting, with a blocking "Finishing sync…" state if a sync is in flight (never silently drop pending changes on exit).
- **Conflict/full-sync handling**: surface `rslib`'s existing conflict resolution (choice between local/remote when schema mismatch is detected) as a modal `AdwAlertDialog`, not a silent auto-pick — data loss from silent resolution is the single worst failure mode a sync client can have.
- **Status affordance**: a persistent header-bar icon with three states (synced / syncing — animated / offline or error, with the error surfaced via a click-to-expand `AdwToast` including the actual error text, not just "sync failed").
- **Backoff**: exponential retry (starting 30s, capped at 15min) on transient failures; immediate retry on network-reconnect regardless of backoff state.

### 5.2 Auth

- Login flow uses `rslib`'s existing AnkiWeb auth (username/password → session key) or a **self-hosted sync server URL**, exposed as a first-class preference (`AdwPreferencesWindow` → "Sync" page → "Sync server" with AnkiWeb as default and a custom-URL field for `anki-sync-server`-family self-hosted setups, matching what desktop Anki already supports under Advanced settings).
- Session key stored via **libsecret**, never in a plaintext dotfile.

---

## 6. Feature Specification (full scope)

| Area | Behavior |
|---|---|
| **Deck browser** | `AdwNavigationView` root: sidebar tree (decks, filtered/custom study decks visually distinct), main pane with due counts, "Study now" per deck. Drag-and-drop deck reparenting. |
| **Reviewer** | Full-window focused mode (distraction-free, hideable header bar), card rendered in an embedded WebKitGTK view sized to fill available space, answer buttons as large touch-friendly `AdwButton`s with keyboard shortcuts (1–4, Space) always shown as accessible labels, not just tooltips. Undo-last-answer always one keystroke away. |
| **Note editor** | Rich-text + raw-HTML toggle per field, cloze deletion tooling, image occlusion editor (canvas-based, GTK `Picture`/`Snapshot` composited), audio recording via GStreamer (`gst-plugins-*`), LaTeX/MathJax preview live in a WebKitGTK pane. |
| **Browser (search)** | Full support for Anki's search-query syntax (delegated entirely to `rslib`'s query parser — do not reimplement), `AdwOverlaySplitView`: query/tag sidebar, results list (virtualized `GtkColumnView` for large collections), editor pane. |
| **Note types / templates** | Manage fields, card templates (HTML/CSS editor with syntax highlighting via `GtkSourceView5`), styling shared across templates. |
| **Stats** | Per-deck and collection-wide: forecast, review count, hourly breakdown, retention/FSRS parameters, true retention — rendered via `plotters`+Cairo (§3), matching data `rslib` already computes for the official client's stats screen. |
| **Import/export** | `.apkg`/`.colpkg` (via `rslib`'s importer — zip-bomb-guarded, see §11), CSV import with field-mapping UI, export presets. |
| **Media** | Orphaned-media check, media DB re-sync, drag-and-drop image/audio into editor fields. |
| **Deck options / scheduler** | Expose FSRS parameters and legacy SM-2 options exactly as `rslib` models them; "Optimize FSRS weights" as a long-running task with progress reported via a cancellable `AdwToast`. |
| **Multi-profile** | Profile switcher at launch and via menu; independent sync credentials per profile. |
| **Keyboard-first** | Every action reachable without a mouse; shortcuts window (`Ctrl+?` → `GtkShortcutsWindow`) auto-generated from a central shortcut registry (also feeds the extension API, §9). |
| **Accessibility** | See §12. |

---

## 7. UI/UX Design Spec

### 7.1 Principles

- Follow the **GNOME HIG** rather than porting Anki's Qt layout verbatim. Concretely: use `AdwNavigationView`/`AdwNavigationSplitView` for the deck-list ↔ deck-detail relationship (collapses to single-pane on narrow windows automatically — a real usability win over the Qt client, which doesn't adapt to narrow/tiling window managers), `AdwToastOverlay` for transient feedback (sync status, undo confirmations) instead of modal dialogs where possible, and `AdwStyleManager` for automatic light/dark theme following the system preference.
- Typography: system default (Cantarell/Inter depending on distro), no custom font bundling except for card-content fonts the user's templates request.
- Iconography: symbolic SVGs (`-symbolic.svg`) matching GNOME's icon style, full app icon following the current GNOME app-icon guidelines (not a straight reuse of Anki's mascot artwork — separate original icon, both for trademark reasons and because Adwaita apps have specific icon-style conventions).
- Empty states use `AdwStatusPage` (e.g., "No cards due — nice work" on an empty deck) rather than a blank pane.

### 7.2 Screen inventory

1. **Profile picker** (`AdwWindow`, shown only if >1 profile) → 2. **Deck browser** (`AdwNavigationSplitView`: sidebar + main) → 3. **Reviewer** (full-bleed, WebKitGTK content + answer bar) → 4. **Browser/search** (`AdwOverlaySplitView`: 3-pane) → 5. **Note editor** (embedded in browser pane, or standalone `AdwDialog` when adding from the deck view) → 6. **Stats** (`AdwNavigationPage` with a view-switcher for date ranges) → 7. **Preferences** (`AdwPreferencesWindow`, pages: General / Sync / Scheduler / Extensions / Advanced) → 8. **Extension manager** (list + permission grants, §9).

### 7.3 Responsiveness

Target down to a 360px-wide window (GNOME's mobile-adjacent minimum) without functionality loss for the reviewer and deck browser at minimum — this is a place Rookery can concretely beat the Qt client, which is desktop-fixed-layout only.

---

## 8. Card Content Rendering (WebKitGTK)

Card templates are arbitrary HTML/CSS/JS (MathJax, `{{TTS}}`, cloze spans, user-authored JS in some templates). Reimplementing this in native GTK widgets is a well-known trap (partial fidelity, endless edge cases against the huge ecosystem of shared decks). Instead:

- Embed `WebKitWebView` (via the `webkit6` crate) for card *content* only — chrome (answer buttons, header bar) stays native GTK, so the app doesn't feel like "a browser wearing a GTK skin."
- **Sandbox it**: disable the JS clipboard/file-system APIs the WebView doesn't need, set a restrictive `WebKitWebsitePolicies`/CSP for loaded card content (untrusted HTML from shared decks is a real attack surface — see §11), block outbound network requests from card content by default (a "this card wants to load an external resource" prompt, opt-in per note type, mirroring desktop Anki's own "web page could not be loaded" network-restriction stance).
- MathJax bundled locally (already how desktop Anki ships it) — never fetched from a CDN at review time.

---

## 9. Extension System (replaces Anki add-ons)

Anki's add-ons are arbitrary Python running inside the Qt process with full hook access and no sandboxing — that model cannot run under a Rust/GTK stack, and porting it would mean embedding a full CPython + PyQt compatibility layer, which defeats most of the point of a native rewrite. Instead of silently dropping the feature, spec it properly:

- **WASM component model** via `wasmtime`: extensions are `.wasm` components with a capability-based manifest (`extension.toml` declaring exactly which hooks/APIs they need — e.g., `hooks.card_will_show`, `storage.kv`, no blanket "full access" option).
- **Stable hook API** (versioned, semver'd independently of Rookery's internal churn): `on_card_will_show`, `on_note_will_save`, `on_sync_did_finish`, `register_editor_button`, `register_deck_stats_widget`. Start with the ~10 hooks that cover the top add-ons in the existing ecosystem (per Anki's shared-add-ons rankings: image occlusion enhancements, review heatmaps, custom answer-button text, frequency/priority tagging) rather than trying to cover the long tail on day one.
- **Extension manager UI** (§7.2 item 8): install from a `.wasm` file or a registry URL, per-extension permission toggles, one-click disable — modeled on how Flatpak/GNOME Software present permissions, since users will already have that mental model.
- **Explicit non-goal for v1**: automatic compatibility with existing Python add-ons. Document this clearly in release notes and provide a migration guide/template for the small set of hooks above, rather than promising something the architecture can't deliver.

---

## 10. Testing & QA Strategy

| Layer | Approach |
|---|---|
| `rslib-shim` | Thin by design (§2.3); tested mainly via integration tests that exercise real `rslib` calls against throwaway collections — don't re-test `rslib`'s own logic, upstream already does. |
| `rookery-core` | Unit tests for sync orchestration (mock `SyncTransport`), retry/backoff timing (using `tokio::time::pause` for deterministic virtual-clock tests), extension host (`wasmtime` test components with deliberately over-scoped permission requests, asserting they're rejected). |
| `rookery-gtk` | GTK4 has first-class AT-SPI accessibility trees; drive UI tests via `dogtail`/AT-SPI automation for critical flows (add note → review → answer → check due count decremented), rather than screenshot-diffing, which is brittle across themes. |
| Sync | Scheduled CI canary against a disposable AnkiWeb test account **and** a self-hosted `anki-sync-server` instance pinned to the matching rev, both directions (push and pull), on every `rslib` version bump (§2.3). |
| Import/export | Fuzz `.apkg` import with `cargo-fuzz` (zip parsing is the classic vector — malformed/oversized archives, path traversal in media filenames, zip bombs — see §11) against a large corpus of real community-shared decks. |
| Scheduler/FSRS | Property-based tests (`proptest`) asserting invariants (due dates never regress on a "Good" answer, interval growth is monotonic within a card's history) — these are almost entirely `rslib`'s responsibility, but worth a thin confirming layer since scheduling bugs are the highest-trust-cost bug class for this kind of app. |
| Performance | Benchmark suite against synthetic 100k-note / 250k-card collections: cold start time, browser search latency, review-answer round-trip latency; regression-gated in CI (fail build if p95 review-flip latency exceeds budget, §14). |
| Accessibility | Manual Orca screen-reader pass on every screen before each release, in addition to automated AT-SPI checks. |
| i18n | Pseudo-localization pass (accented/expanded strings) to catch layout breakage before real translations land. |
| Memory | `heaptrack`/Valgrind runs across a scripted "long session" (hours of continuous review + sync cycles) watching specifically for leaks at the Rust↔GTK object-lifetime boundary (a known footgun class with `gtk4-rs`'s `glib::Object` wrapper types if `Rc` cycles form). |

---

## 11. Security Considerations

- **Untrusted card content**: shared decks are a common vector (embedded JS, crafted HTML). WebView sandboxing per §8 is mandatory, not optional-hardening.
- **`.apkg` import**: zip parsing must guard against zip bombs (cap decompressed size and file count before extraction, not after), path traversal via crafted media filenames (`../../etc/...`), and malformed SQLite files (open the extracted collection read-only for a validation pass before touching the user's real collection).
- **Credentials**: Secret Service/libsecret only, never plaintext config (§5.2).
- **Extensions**: capability-scoped WASM sandbox is the entire security model here — no extension gets filesystem or network access it didn't explicitly declare and the user didn't explicitly grant (§9).
- **Sync transport**: TLS by default for both AnkiWeb and self-hosted servers; for self-hosted, allow user-supplied CA/self-signed cert pinning (common in home-lab `anki-sync-server` setups) but require an explicit opt-in with a clear warning, never silent TLS-verification bypass.

---

## 12. Accessibility

- Full keyboard operability (§6, "keyboard-first" row) is treated as a correctness requirement, not a stretch goal.
- GTK4's native AT-SPI integration means most standard widgets are accessible for free; the two custom-drawn areas (stats charts via `plotters`/Cairo, image-occlusion editor) need **explicit `AccessibleRole`/`AccessibleProperty` annotations** since Cairo drawing has no semantic tree by default — budget real design time for these two screens specifically.
- Respect `org.gnome.desktop.interface` settings (text scaling factor, high-contrast) automatically via GTK4/libadwaita's built-in support — don't build a parallel font-size preference that can drift out of sync with the system one.
- Reduced-motion: honor `AdwStyleManager`'s system reduced-motion signal for the card-flip and toast animations.

---

## 13. Packaging & Distribution

- **Primary: Flatpak on Flathub.** Sandboxed by default; the WebKitGTK dependency and extension WASM sandbox both fit naturally into Flatpak's existing sandboxing story rather than fighting it. App-ID `io.github.<org>.Rookery` (§2.4).
- Flatpak permissions kept minimal: no host filesystem access beyond a user-chosen collection-storage portal folder, network access (required, for sync), `--talk-name=org.freedesktop.secrets` (credential storage), `--talk-name=org.freedesktop.NetworkManager` (sync-on-reconnect trigger, read-only signal subscription).
- **Secondary: native `.deb`/`.rpm`** for users who specifically want a `systemd --user` sync-daemon unit outside a Flatpak sandbox (Flatpak background activation exists but is less battle-tested than a native systemd unit for this use case) — lower priority, community-maintainable once the core app is stable.
- AppStream metainfo (`*.metainfo.xml`) with real screenshots and a correct `<content_rating>`/`<releases>` history from day one — required for Flathub review anyway, and this is exactly the kind of thing that's painful to retrofit.
- Reproducible builds: pin all Cargo dependencies via `Cargo.lock` committed to the repo; Flatpak manifest pins exact commit SHAs (including the `rslib` git dependency's SHA, doubly important given §2.3).

---

## 14. Non-Functional Requirements / Performance Budgets

| Metric | Target |
|---|---|
| Cold start to interactive deck browser | < 500ms on typical hardware (collection under 10k cards), < 1.5s at 100k+ cards |
| Review answer → next card shown | < 100ms p95 (excludes card-authored JS/MathJax render time, which is template-dependent and out of Rookery's control) |
| Search query in browser (10k-card collection) | < 150ms p95 |
| Idle memory footprint | < 150MB RSS with a mid-size collection loaded, WebView idle |
| Sync (incremental, typical daily delta) | < 3s on broadband, with visible progress after 1s |

---

## 15. Roadmap

- **M0 — Foundations**: workspace scaffold, `rslib-shim` compiling against a pinned tag, basic `AdwApplicationWindow` shell, profile open/create.
- **M1 — Read path**: deck browser, reviewer (WebKitGTK card rendering, answer buttons, FSRS scheduling via `rslib`), background sync daemon with the trigger set in §5.1 (sync *in*, no editing yet).
- **M2 — Write path**: note editor, browser/search pane, note-type/template management, import/export, undo/redo wired to `rslib`'s stack, media management.
- **M3 — Depth**: stats screen, deck options/FSRS-weight optimization UI, multi-profile, extension system MVP (WASM host + the ~10 launch hooks from §9).
- **M4 — Polish & release gate**: accessibility pass (Orca + AT-SPI audit), i18n, performance budget verification (§14), Flathub submission, trademark/naming resolution (§2.4) — **this gate blocks public 1.0**, not a post-launch cleanup item.

---

## 16. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| `rslib` API churn breaks builds on upstream Anki releases | Isolated `rslib-shim` crate, deliberate/manual version bumps, CI canary (§2.3, §10) |
| AnkiWeb sync protocol changes mid-cycle | Same canary; since we use Anki's *real* sync client code, protocol drift only breaks us if it breaks the pinned Anki version too — much lower risk than an independent reimplementation |
| Trademark/naming blocked | Ship under neutral name indefinitely if permission isn't granted (§2.4); this is a branding risk, not a technical or legal-viability one |
| WebKitGTK as a dependency is heavy | Accepted tradeoff (§8) — the alternative (reimplementing HTML/MathJax rendering) is strictly worse and is exactly the kind of scope creep that kills native-rewrite projects |
| Extension incompatibility disappoints existing add-on users | Set expectations explicitly in v1 release notes (§9); prioritize the highest-usage hook set first rather than promising full parity |
| AGPL obligations surprise a future contributor/fork | State the license prominently in `README.md` and `CONTRIBUTING.md` from commit one; this is why it's called out at the very top of this document (§2.4) |

---

## 17. Open Questions Worth Resolving Before M0

1. Has an actual conversation with Ankitects about naming happened yet, or should M0 start under the placeholder name with the assumption it may need a full rename before Flathub submission?
2. Target minimum GTK4/libadwaita version — pinning to whatever ships in the *current* Ubuntu LTS and Fedora stable at project start (rather than bleeding-edge `adw` APIs) trades a few nice widgets for a much larger install base; worth deciding explicitly rather than defaulting to "whatever's newest."
3. Is a `systemd --user` sync daemon (outside the Flatpak sandbox) actually a priority for the target users, or is in-process background sync (while the app is open) sufficient for v1 — this materially changes M1 scope.
4. Should the extension system's hook set be drafted by directly surveying the current top 20 shared add-ons' actual hook usage (concrete, evidence-based) before M3, rather than the illustrative list in §9?
