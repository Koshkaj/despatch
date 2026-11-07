# Facelift — Despatch as a menu-bar panel

Despatch stops being a standalone windowed app and becomes a **menu-bar (status bar) item**. Clicking
the item drops a panel under it that copies the structure of the reference screenshot: a header
with search and a pin, a source-list sidebar on the left, a list pane on the right with a segmented
control, and a footer bar.

> **Design reference: [`references/facelift.png`](references/facelift.png).** The new interface
> must look like this screenshot: its layout, proportions, AppKit control styling, type scale and
> palette. Use it as the visual source of truth for the facelift, with only the content changes
> listed in section 1. It replaces `references/ui.png` and `references/ui_2.png` as the layout
> reference. Read it before changing any view, and compare every phase's captures against it.

> **Revised after review.** Where the sections below disagree with this list, this list wins:
>
> - **Despatch is SSH only.** The Agents destination, the agent and custom-command editors, the
>   built-in agent presets and their logos are gone. A config from an earlier version that still
>   has an `agents` section opens; its groups stay in the file untouched and are never shown,
>   searched, or listed in Recently used.
> - **No "Library" heading and no "On this Mac" line.** The sidebar lists the groups; Recently used
>   and Settings sit at its foot, where the config-location line was. Settings is no longer in the
>   footer bar, which starts with the add action. Where the config lives is shown in Settings.
> - **Categories are pills**, not a segmented control: separate outlined capsules as wide as their
>   names, the selected one outlined in the accent over a faint accent wash.
> - **Launches open in** is a picker in Settings (System default or an installed terminal), stored
>   on this Mac.
> Like the other reference images, it is local only: `references/` is git-ignored.

Nothing changes in what a launcher **does**. SSH and agent launches, terminal selection, the config
file, its validation and conflict handling, the editors, and the reachability probe all keep
working as they do today. This is a change to the shell and the presentation.

---

## 1. Reading the reference screenshot

The screenshot is `references/facelift.png`. It is shown at 2×. All measurements below are in
points. The "Despatch equivalent" column lists the only things that differ from the screenshot.
Everything else (spacing, radii, control styling, hierarchy) should match it.

| Area | What the screenshot shows | Measured | Despatch equivalent |
|---|---|---|---|
| Status item | Template glyph in the menu bar, highlighted while the panel is open | 22 pt menu-bar slot | `NSStatusItem` with an SF Symbol template image |
| Anchor | Small caret pointing up at the item, panel centered under it | ~10 pt caret, panel top ~8 pt below the bar | Panel anchored under the item's button frame (caret is decision D3) |
| Panel | Rounded, shadowed, light vibrant surface | ~724 × 514, radius ~12 | Borderless, transparent window with a popover/menu material behind it |
| Header | App glyph + **"Shelf"** title · search field with `⌘ F` hint · pin button | 50 high, hairline under it | Despatch glyph + **"Despatch"** · "Search servers ⌘F" · pin |
| Sidebar header 1 | **Library** (secondary, semibold, small) | 11 pt | **Groups** — the server groups |
| Sidebar rows | Icon · name · count on the right. The selected row is filled with the accent color and has white text and icon | 30 high, radius 6 | One row per **server group**: tinted folder/stack symbol in the group's label color, name, launcher count |
| Sidebar header 2 | **Quick access** → Favorites, Recently used | | Revised at review: no heading; **Recently used** and **Settings** at the foot of the sidebar |
| Sidebar footer | Laptop icon · "On this Mac" | | Replaced at review by Recently used and Settings; where the config lives is in Settings |
| Pane heading | **Snippets** on the left, "24 snippets" on the right | 15–17 pt semibold | Group name · "12 servers" |
| Category pills | All · Email · Meetings · Code as separate outlined capsules; the selected one outlined in the accent with a faint accent wash | 28 high | **All** · each **category** of the selected group · trailing "+" to add a category |
| Rows | Rounded-square icon tile · title · one-line secondary detail · trailing tertiary tag (and a star on hover/favorite) | ~56 high, hairline separators inset to the text | Launcher row: SF Symbol or provider logo in a backplate · name · `user@host:port` or the command · trailing tag (category name in **All**, otherwise empty). Hover reveals Edit / Duplicate / Delete |
| Footer | Settings-style icon · `+ New snippet` · right: copy icon · "Click a snippet to copy" | 35 high, hairline above | Settings icon · `+ New server` / `+ New agent` (contextual) · right: launch hint, **or** the status banner when there is one |

What the screenshot does **not** have, and Despatch still needs: editors, delete confirmation, the
status/conflict banner, drag-to-move, and an empty state for each level. Section 4 places each one.

---

## 2. Target layout

```text
                                                    ┌──┐
 menu bar ……………………………………………………………………………………………………… │▣ │ ……  Sat 9:41
                                                    └──┘
        ┌──────────────────────────────────────────────────────────────────┐
        │ ▣ Despatch      [🔍 Search servers                     ⌘F]   📌  │
        ├──────────────────┬───────────────────────────────────────────────┤
        │ Groups        +  │ Production                          12 servers│
        │ ▓▓ Production 12▓│ ┌ All ─┬ Web ┬▓Workers▓┬ Bastions ┬ + ┐         │
        │  ▢ Staging     7 │                                               │
        │  ▢ Local       3 │ [▣] Worker 1                        ✎ ⧉ 🗑   │
        │                  │     deploy@worker-1.internal                  │
        │                  │ ───────────────────────────────────────────── │
        │                  │ [▣] Worker 2                                  │
        │                  │     deploy@worker-2.internal                  │
        │                  │ ───────────────────────────────────────────── │
        │  ◷ Recently used │                                               │
        │  ⚙ Settings      │                                               │
        ├──────────────────┴───────────────────────────────────────────────┤
        │ + New server                           ↩ Double-click to connect │
        └──────────────────────────────────────────────────────────────────┘
```

Right pane contents, by sidebar selection:

| Sidebar selection | Right pane |
|---|---|
| A server **group** | Group heading + count, segmented control (`All` + categories + `+`), launcher rows of the chosen segment |
| **Recently used** | "Recently used" heading + "Clear" action, server rows, newest first, each with a breadcrumb (`Production › Workers`) and a relative time (`2 h ago`) |
| **Settings** | Settings content in the pane (appearance, terminal, config file + Reveal/Reload/Open/Save copy, keyboard list, Quit), no longer a sheet |
| Search query not empty | "Results" across every group and agent, with breadcrumbs. Clearing the query returns to the previous selection |

Selection rules after the change:

- A sidebar row is a **destination**, not a toggle. Clicking the open group again does not close it.
  The "deselect a level to hide everything under it" rule goes away for groups.
- Opening a group selects **All**. A category selection is remembered per group for the session.
  Nothing is auto-selected inside the list.
- With nothing selected on first open (no stored selection), the first group is shown. If there are
  no groups, the Groups section shows an empty row "No groups yet — Add group" and the pane shows
  the empty state.

---

## 3. Decisions to confirm before work starts

These change architecture or contradict `AGENTS.md`. Each one has a recommendation. Phase 1 should
not start until D1 and D2 are confirmed.

**D1 — Render with Iced inside a panel window, or rewrite the UI in native AppKit.**
*Recommended: keep Iced.* Despatch keeps the Iced/wgpu renderer and draws the AppKit look. A
borderless window is positioned under an `NSStatusItem`, and a small `objc2` bridge in
`despatch-platform-macos` handles everything AppKit-specific. This keeps the editors, drag layer,
presenters and their tests. The alternative is real `NSPopover` + `NSOutlineView` + `NSTableView` +
`NSSegmentedControl`, which gives pixel-exact AppKit controls and VoiceOver for free. But it is a
full UI rewrite in `unsafe` objc2, and `AGENTS.md` forbids replacing Iced without explicit
approval.

**D2 — Typography and color.** The screenshot uses SF Pro and the system blue accent. Today the
app uses bundled Lilex (monospaced) and an ink/parchment two-color theme with no system accent.
*Recommended: switch to the system font (`.SF NS`, already resolved once per
`docs/architecture/ui-toolkit.md`) and AppKit semantic colors with the user's accent color
(`system_appearance()` already reads it).* Consequences:
- `presenters::text_width` / `ADVANCE` assume a monospaced font. In a list layout rows fill the
  width, so content-sized card widths go away, but `ellipsize` still needs a measurement. Replace
  it with a measured per-glyph width table for SF at the sizes used, or let `text` clip with
  `Wrapping::None` and fade. Iced 0.14 has no built-in ellipsis.
- The three `typography` tests that tie the Lilex files to `ADVANCE` are rewritten or deleted
  together with the fonts.
- Keep Lilex for **detail lines only** (`user@host:port`, commands)? This is a reasonable middle
  ground and matches how macOS shows technical strings. Optional.

**D3 — Caret.** Modern macOS menu-bar extras (Control Center, Wi-Fi) have no arrow. *Recommended:
no caret, 6 pt gap under the menu bar.* If the caret is wanted, the transparent window draws it
above the rounded rectangle, centered on the item's x, clamped to the panel's corners.

**D4 — Click model.** The screenshot's footer says "Click a snippet to copy", which is single-click
to act. `AGENTS.md` says one click selects and two clicks launch. *Recommended: keep
select-then-double-click / Return, as the brief says "after it is clicked we do not change the
functionality"*. The footer reads "Double-click or ↩ to connect". After a successful launch the
panel hides, unless it is pinned.

**D5 — Favorites.** The screenshot has Favorites and a star per row. The brief replaces Quick
access with Agents, Recently used and Settings and does not ask for favorites. *Recommended: no
favorites in this facelift; the trailing slot of a row is the category tag / hover actions.*
Name the second sidebar section **Library**, or leave it without a heading.

**D6 — Pin semantics.** *Recommended: pinned = the panel does not hide when it loses focus and
stays at its position. It is still attached to the menu bar, not a free-floating window. The pin
state is machine-local and is reset at app start.* Optional extra: dragging the header of a
pinned panel detaches it.

**D7 — Status item glyph.** An SF Symbol rendered as a template image (adapts to light/dark menu
bar and highlight), for example `terminal` or `server.rack`. The app icon cannot be used: menu-bar
images must be monochrome templates.

**D8 — Global hotkey and launch at login.** Both are normal for a menu-bar utility, but the brief
does not ask for them. *Recommended: launch at login (`SMAppService.mainApp`, macOS 13+) as a
Settings toggle in Phase 5, hotkey left out.* Without a Dock icon and without launch at login,
the user must start the app from Finder or Spotlight after every login.

---

## 4. What has to be implemented

### 4.1 App shell: from window app to menu-bar agent

1. **No Dock icon, no app menu.** Add `LSUIElement = true` to `apps/despatch/resources/Info.plist`.
   A bare `cargo run` is not inside a bundle, so the bridge also calls
   `NSApplication.setActivationPolicy(.accessory)` at startup. Iced/winit 0.14 does not expose
   this.
2. **Iced daemon instead of application.** Change `despatch_ui_iced::run` from `iced::application`
   to `iced::daemon`. A daemon opens no window at start and keeps running when its windows close.
   `view`, `title` and `theme` take a `window::Id`.
3. **The panel window.** Open it with `window::Settings`:
   `decorations: false`, `transparent: true`, `blur: true`, `resizable: false` (or a bounded
   resize, see 4.7), `level: AlwaysOnTop`, `visible: false`, `exit_on_close_request: false`, and
   a size of about 720 × 520. Then use the bridge (via `iced::window::run`, the same way the
   accessibility bridge gets its `NSView`) to:
   - set the window level to `NSPopUpMenuWindowLevel`;
   - set `collectionBehavior` to `canJoinAllSpaces | fullScreenAuxiliary | transient`, so the
     panel opens on the active Space and over full-screen apps;
   - put an `NSVisualEffectView` (material `.popover` or `.menu`, state `.active`) behind the
     content, with a 12 pt corner mask and `hasShadow = true`;
   - honour Reduce Transparency: give the material an opaque fallback.
4. **Show and hide.** Iced 0.14 has no "set visible" task (only open, close, move_to, gain_focus).
   Two options:
   - *Recommended:* keep one window for the life of the app and toggle it through the bridge
     (`orderOut:` / `makeKeyAndOrderFront:` + `NSApp.activate`). Widget state such as the scroll
     position and the search text survives, and there is no new wgpu surface on every click.
   - Alternative: `window::close` / `window::open` on every toggle. This is simpler, but it
     creates a new surface each time and loses all widget state.
5. **Positioning.** The status-item bridge reports the button's frame in screen coordinates. The
   panel's x is centered on it and clamped to the visible frame of *that* screen. Its y is the
   bottom of the menu bar + gap (D3). AppKit screen coordinates start at the bottom-left, so they
   are converted to winit's top-left logical points. Also handle multiple monitors and a menu bar
   that auto-hides.
6. **Dismissal.** The panel hides when:
   - the user clicks the status item again;
   - the user presses Escape and no dialog is open (with a dialog open, Escape dismisses the
     dialog first, as now);
   - the window gets `window::Event::Unfocused`, unless it is pinned or a system panel is
     currently open (see the guard below);
   - a launch succeeds, unless it is pinned.
7. **Focus guards.** `NSOpenPanel` (identity file, Open config, Save copy) and the Ghostty
   Automation prompt take focus from the panel. The shell sets a `system_panel_open` flag around
   these calls, so the focus loss they cause does not hide the panel or discard a half-filled
   editor.
8. **Quit.** Without a Dock icon or an app menu there is no default Quit, so it is added:
   - `⌘Q` inside the panel;
   - a **Quit Despatch** button in Settings;
   - an item in the status item's right-click menu.
9. **Status item menu (right-click / control-click).** Open Despatch · Settings… · Reveal Config in
   Finder · Quit. This is a native `NSMenu` built in the bridge. Left click toggles the panel.
10. **Events from AppKit to Iced.** Status-item clicks and menu choices happen in AppKit callbacks.
    The bridge sends them into a channel. The UI crate turns the channel into an Iced
    `Subscription` (`Subscription::run` with a stream), which produces
    `Message::StatusItem(StatusItemEvent)`. No Objective-C types leave the platform crate.
11. **Window frame and pin.** The panel always opens at its anchor, so the window-state
    preference is only "last panel size", if resizing is allowed. The pin state is not persisted
    (D6).

### 4.2 Header

- Despatch glyph: a small SF Symbol in the accent color, or the app icon. This is the AppKit
  "window title" spot, and it is **not** a drag handle unless D6's detach option is taken.
- Title "Despatch", about 15 pt semibold.
- Search field: rounded 7 pt, a field fill one step off the panel ground, a magnifier symbol, a
  placeholder "Search servers and agents", and a trailing `⌘F` hint in tertiary text that hides
  while the field has text. When the field has text, a clear (×) button replaces the hint.
  `SEARCH_ID` is focused whenever the panel opens, so the user can type at once.
- Pin button: an icon button with the `pin` / `pin.fill` symbol, a "Keep open" / "Pinned"
  tooltip and an accessibility label.

### 4.3 Sidebar (source list)

- Width ~174 pt, fixed. The panel is not resized narrow enough to need collapsing.
- **Section heading "Groups"**, with a `+` button that shows on hover/focus of the heading
  (Add server group, `⌘⇧N`).
- **Group row**: a symbol (`folder` or `square.stack.3d.up`) tinted with the group's label color
  (or the secondary label color when it has none), the name, and a right-aligned count of every
  launcher in the group. The selected row is filled with the accent color, with on-accent text,
  symbol and count. Hover is a faint wash. The row is 30 pt high with a 6 pt radius and an inset
  of 8 pt from the sidebar edges.
- Group actions (Rename, Change color, Delete): available on a hover `…` button at the right end
  of the row (it replaces the count while hovered). Also available by right-click if the native
  menu bridge from 4.9 exists. The existing name editor sheet is reused.
- Group reordering: drag a sidebar row (Phase 6, optional). `reorder_groups` already exists in the
  application layer.
- **Section heading "Library"** (D5) with rows: **Agents** (count), **Recently used**,
  **Settings**. These use the same row style.
- **No sidebar footer.** The screenshot's "On this Mac" line was dropped at review: where the
  config lives is shown and revealed in Settings, and Dirty / Conflict is said in the footer's
  status line.
- Keyboard: the sidebar is one focus group. ↑/↓ move between rows when the sidebar has focus.
  `⌘1…⌘9` jump to group N. `⌘0` opens Recently used.

### 4.4 Pane heading and segmented control

- Heading row: the destination's name (15–17 pt semibold) on the left, the count in secondary
  text on the right ("12 servers", "4 agents"). In a group, a `…` button after the name gives
  group actions without having to hover the sidebar.
- **Segmented control** (a new widget/style, it replaces the category chip rail):
  - revised at review to **pills**: separate 28 pt capsules, each as wide as its name, 8 pt apart,
    outlined in a quiet gray; the selected pill is outlined in the accent over a faint accent wash,
    with accent-colored medium text (lightened in dark mode so it reads against the wash);
  - segments: `All`, then each category in config order, then a narrow `+` segment
    (Add category);
  - categories have no color of their own (unchanged rule): segments are drawn neutral;
  - **overflow**: if the segments no longer fit at their minimum width, the control shows as many
    as fit plus a `More ▾` segment that lists the rest. This needs a simple popover anchored to the
    segment. Equal widths alone break at 6 or more categories in a 550 pt pane;
  - category actions (Rename, Delete): hover a selected segment for a small `…`, or right-click
    (4.9);
  - **drop targets**: while a row is carried (existing drag gesture), every segment except the
    row's own category and `All` offers itself, and the one under the pointer is filled. This is
    today's chip drop behaviour moved onto segments.
- A group with **no categories** shows no segmented control. It shows an empty state instead:
  "Production has no categories yet", with **Add category** as the primary action. A launcher
  must live in a category (domain rule, unchanged).

### 4.5 Launcher rows

- Row height ~52–56 pt, full pane width, hairline separators inset to start under the text. The
  row has no card border. Selection is an accent wash with a 6 pt radius (or a full accent fill
  with on-accent text, matching the sidebar; pick one and use it in both places).
- Leading **icon tile**: a 30 pt rounded square (radius 7) with a hairline border and a panel
  fill. It holds the SF Symbol (`server.rack` for SSH, `command.square` for custom) or the
  provider logo (Claude/Codex/Gemini/OpenCode). The launcher's label color tints the symbol and
  the tile border. It never floods the whole row (existing color rule, applied to the new tile).
- **Title**: name, 13 pt medium. **Detail**: `presenters::detail` (unchanged), 12 pt secondary,
  one line, ellipsized.
- **Trailing slot**: in `All`, search results, and Recently used, a tertiary tag naming where the
  launcher lives (category, or `Group › Category`). Recently used also shows a relative time. On
  hover or keyboard selection, the slot holds the action buttons Edit, Duplicate, Delete (and
  "Move to…", see below). The pointer tracker wraps the whole `stack`, as the hover rule in
  `AGENTS.md` already requires.
- Gestures (unchanged semantics):
  - press selects at once;
  - double-click / Return launches;
  - press-and-drag lifts the row into the `drag_layer` and it can be dropped on a segment.
  The floating copy becomes a row-shaped capsule instead of a card.
- "Move to…" (new): an action that opens a small picker of the group's categories. This makes
  moving possible from the keyboard and in `All`, where no segment is the obvious target.
- The **list is a `scrollable` column**. Measure it at 500 launchers in one category before
  deciding on virtualization (existing rule).

### 4.6 Footer bar

- Left: a settings icon button (`slider.horizontal.3` as in the screenshot, opens the Settings
  destination, `⌘,`), a short vertical separator, then a contextual **add** button:
  - in a group whose segment is a category → **+ New server** (adds there);
  - in a group with `All` selected → **+ New server**. If the group has one category it goes there;
    otherwise the editor gains a **Category** picker (new field, see 4.8);
  - in a group with no categories → **+ New category**;
  - in Agents → **+ New agent**;
  - in Recently used / Settings → hidden.
- Right: a hint with a symbol, in tertiary text: "Double-click to connect" (D4). **When a status
  exists, the banner replaces the hint here**: launch result, action error, or config
  state. It keeps the existing lifetime rules (event banners expire after 10 s, state banners
  stay; dismissal tokens still apply). Conflict state also shows its Reload / Save copy actions
  in this bar, or in a one-line strip above the list if they do not fit.

### 4.7 Panel geometry and appearance

- Size ~720 × 520 by default. Recommended: fixed for v1. If it is resizable, use min 620 × 420,
  max 1000 × 760, and keep the size as a local preference.
- Corner radius 12, system shadow, popover material. Sidebar and header sit on the material
  (structural chrome may be translucent); the list pane is a solid semantic surface (content
  stays opaque for readability; this rule is unchanged).
- Colors (D2): window ground, sidebar ground, separator, label/secondary/tertiary, accent,
  on-accent, field fill, segmented track, and selected segment, all as semantic tokens in
  `theme.rs` with light and dark values close to AppKit's (`windowBackgroundColor`,
  `controlBackgroundColor`, `separatorColor`, `labelColor`…). The accent is the user's system
  accent, with the existing fallback.
- Increase Contrast: borders on rows, segments and the search field. Reduce Motion: the panel
  appears without a scale/slide, using a short fade at most.
- Motion: a 120 ms fade + 4 pt drop when opening; hiding is immediate. No other animation is
  added.

### 4.8 Editors, confirmations and settings inside a panel

- Sheets stay overlays inside the panel. They originate from the control that opened them (for
  example, the footer add button → the sheet grows from the bottom-left), and keep a max height
  that fits the 520 pt panel with internal scrolling (the editors already scroll).
- **Launcher editor**: a new **Category** picker, shown when the editor was opened from `All` or
  search. It defaults to the selected category, or the first one. All other fields, validation,
  executable resolution, reachability check, color picker and disclosure behaviour are unchanged.
- **Settings** becomes a destination in the right pane instead of a sheet. `Dialog::Settings`
  is removed, `⌘,` selects the Settings destination, and the content adds:
  - **Open config…**
  - **New config…**
  - **Save a copy…** (the use cases exist: `open_config`, `create_config`, `save_copy`; the UI
    for them is incomplete today)
  - **Clear recently used**
  - **Launch at login** (D8)
  - **Quit Despatch**
- Delete confirmation, the name editor and the color picker are visually restyled to the new
  tokens only.

### 4.9 Native context menus (optional, Phase 6)

A small bridge function `show_context_menu(view, point, items) -> Option<usize>` that runs
`NSMenu.popUpMenuPositioningItem:atLocation:inView:` and returns the chosen index. It is used for
right-click on sidebar groups, segments, and rows. AppKit's menu tracking blocks the event loop
while the menu is open; that is how native menus behave, and the result returns as a `Message`.
Hover action buttons stay, so keyboard and trackpad users never depend on right-click.

### 4.10 Recently used (new feature)

- **What it records**: a successful launch (`LaunchOutcome::Success`) of any launcher, with the
  launcher id and a timestamp. A failed launch is not recorded.
- **Where it lives**: machine-local, never in the shared config (it is usage history, like
  window state). `NSUserDefaults` has safe setters only for numbers and URLs (per `AGENTS.md`), so
  a list of UUIDs + timestamps goes in a small JSON file:
  `~/Library/Application Support/Despatch/recents.json`, written with the existing atomic write.
- **Limits**: newest first, de-duplicated by launcher id, at most 20 entries. Entries whose
  launcher no longer exists in the loaded config are not shown. They are dropped on the next
  write, so switching configs does not show another config's launchers.
- **Port**: new `RecentLaunches` port in `despatch-application/src/ports.rs`
  (`load() -> Vec<RecentLaunch>`, `record(LauncherId, SystemTime)`, `clear()`).
  `Application::launch_selected` records on success. A failure to record is reported as a non-fatal
  status. It never fails the launch.
- **Presentation**: rows as in 4.5, with breadcrumb + relative time ("just now", "5 min ago",
  "yesterday", then a date). Empty state: "Servers and agents you open appear here."

---

## 5. What has to be created, changed and removed — by file

### New files

| File | Purpose |
|---|---|
| `crates/despatch-platform-macos/src/status_item.rs` | `NSStatusItem` + template SF Symbol image, a target/action class defined with `objc2::define_class!` for left click, the right-click `NSMenu`, button frame in screen coordinates, and the event channel to the UI. `#[allow(unsafe_code)]`, with `// SAFETY:` on every block |
| `crates/despatch-platform-macos/src/panel_window.rs` | Changes to the raw `NSWindow` behind the Iced window: level, collection behaviour, `NSVisualEffectView` + corner mask, shadow, `orderOut:` / `makeKeyAndOrderFront:`, `NSApp.activate`, activation policy `.accessory`, conversion of the visible screen frame |
| `crates/despatch-platform-macos/src/context_menu.rs` | *(optional, Phase 6)* `NSMenu` pop-up returning the chosen index |
| `crates/despatch-platform-macos/src/login_item.rs` | *(if D8)* `SMAppService.mainApp` register/unregister/status |
| `crates/despatch-config/src/recents_repository.rs` | JSON implementation of the `RecentLaunches` port, using `atomic_write` |
| `crates/despatch-ui-iced/src/status_item.rs` | Turns the platform event channel into a `Subscription<Message>`. Holds the `StatusItemEvent → Message` mapping |
| `crates/despatch-ui-iced/src/views/` (module dir) | Split of today's 2 433-line `views.rs` along the new layout: `mod.rs` (root, layers), `header.rs`, `sidebar.rs`, `list.rs` (rows, empty states), `segmented.rs`, `footer.rs`, `settings.rs`, `editors.rs` (moved from `views.rs` unchanged except styling), `recent.rs` |
| `tests/fixtures/recents.json` | Fixture for the recents repository tests |

### Changed files

| File | Change |
|---|---|
| `apps/despatch/resources/Info.plist` | `LSUIElement = true` |
| `apps/despatch/src/main.rs` | Wire the recents repository into `Application::new`. Create the status item before `run` (or pass a constructor into the UI so it is created on the main thread at daemon start) |
| `crates/despatch-ui-iced/src/lib.rs` | `iced::application` → `iced::daemon`, open the hidden panel window at start, new window settings, register new modules |
| `crates/despatch-ui-iced/src/app_shell.rs` | New state: `destination: Destination { Group(GroupId), Agents, Recent, Settings }`, `segment: Option<SubgroupId>` (None = All) remembered per group, `pinned`, `panel_visible`, `system_panel_open`, `recents`. New messages: `StatusItem(..)`, `TogglePanel`, `HidePanel`, `TogglePin`, `SelectDestination`, `SelectSegment(Option<SubgroupId>)`, `ClearRecents`, `Quit`, `OpenConfig`, `NewConfig`, `SaveCopy`, `MoveLauncherTo(..)`, `WindowUnfocused`. Remove `SelectSection` toggling and the group/category close-on-reselect logic. Keyboard map: Escape hides the panel when no dialog is open, ←/→ change segment, ⌘1…9 / ⌘0 jump destinations. Subscription adds the status-item stream and window focus events |
| `crates/despatch-ui-iced/src/presenters.rs` | New `SidebarRow`, `SegmentRow`, `ListRow` (row = `Tile` + breadcrumb + trailing tag + relative time), `config_location_label`, `recent_rows`, `search_rows` (cross-group), `relative_time`. Remove card-width presenters that only exist for content-sized cards. Rework `ellipsize`/`text_width` per D2 |
| `crates/despatch-ui-iced/src/style.rs` | New styles: `panel`, `header`, `sidebar_section_heading`, `source_list_row`, `segment_track`/`segment` (rework of the existing ones), `list_row`, `icon_tile`, `footer_bar`, `pin_button`. Remove `tile`, `tile_surface`, `add_tile`, `chip`, `add_chip`, `badge`, `add_badge` once unused |
| `crates/despatch-ui-iced/src/theme.rs` | Replace ink/parchment with AppKit-like semantic tokens for light and dark, using the system accent (D2). Keep `with_alpha`/`mix` helpers |
| `crates/despatch-ui-iced/src/typography.rs` | System font roles (D2); the Lilex weights stay only if used for detail lines |
| `crates/despatch-ui-iced/src/icons.rs` | New symbols: `Pin`, `PinFill`, `Folder`, `Clock`, `Sliders`, `Laptop`, `ICloud`, `Ellipsis`, `Move`, `StatusItem`. Symbols are rasterized for 16–18 pt list/sidebar use instead of the rail-sized canvas |
| `crates/despatch-ui-iced/src/drag_layer.rs` | Floating copy is row-shaped. Drop target lookup uses segments instead of chips. The threshold, one-message-per-gesture and inert layer rules are unchanged |
| `crates/despatch-ui-iced/src/accessibility.rs`, `accessibility_bridge.rs` | Rebuild the node model for the new tree (header, search, pin, sidebar list, segmented control as a tab group, list rows, footer) |
| `crates/despatch-ui-iced/src/shortcuts.rs` | Add `HidePanel`, `Quit`, `Destination(n)`, `PreviousSegment`/`NextSegment`; retarget ←/→ |
| `crates/despatch-ui-iced/src/capture.rs` + `capture_scene` | Capture renders the panel window. New/renamed scenes: `panel`, `panel-all`, `panel-empty-group`, `agents`, `recent`, `settings`, `search`, `add-server`, `add-server-advanced`, `add-agent`, `rename-group`, `rename-category`, `delete`, `drag`, `hover`, `segments-overflow` |
| `crates/despatch-ui-iced/src/editor.rs` | Category field on `LauncherDraft` when opened outside a single category |
| `crates/despatch-platform-macos/src/lib.rs`, `Cargo.toml` | Register new modules. Add `objc2-app-kit` features: `NSStatusBar`, `NSStatusItem`, `NSStatusBarButton`, `NSButton`, `NSControl`, `NSMenu`, `NSMenuItem`, `NSVisualEffectView`, `NSScreen`, `NSRunningApplication`, `NSCell`, `NSEvent` (and `objc2-service-management` if D8) |
| `crates/despatch-application/src/ports.rs`, `application.rs`, `lib.rs` | `RecentLaunches` port + `RecentLaunch` type. `Application::new` takes it. `launch_selected` records on success. `recent_launches()` / `clear_recents()` use cases |
| `crates/despatch-application/src/state.rs` | Remove `Dialog::Settings`. Selection: a group stays open when it is selected again. Add `select_segment` semantics, where `selected_subgroup = None` means **All**. `search_results` already searches every group and feeds the cross-group Results view |
| `crates/despatch-config/src/lib.rs` | Export the recents repository |
| `scripts/verify-app.sh` | Assert `LSUIElement` is true |
| `docs/architecture/ui-toolkit.md` | Status-item + panel architecture, system font/colors, material bridge |
| `docs/performance/baseline.md` | New idle RSS for a menu-bar agent, measured both hidden and shown. The hidden panel must not redraw |
| `AGENTS.md` | See section 7 |

### Removed

- The icon rail: `sidebar`, `rail_item`, `rail_divider`, `rail_symbol`, all `RAIL_*` and
  `SIDEBAR_WIDTH`/`TITLEBAR_INSET` constants, `style::sidebar_row`.
- The card grids: `group_card`, `add_card`, `card_body`, `card_rows`, `card_width_need(s)`,
  `uniform_card_width`, `label_width`, `pack_rows`, `category_rail`, `category_chip`, `add_chip`,
  `chip_width`, `add_chip_width`, and their tests.
- `Dialog::Settings` and `settings_sheet` (it becomes the Settings pane).
- The window-app `window::Settings` (title hidden, full-size content view, 940 × 620 size, min
  size), and `Message::WindowOpened` as the only window hook.
- The Lilex fonts and `typography` tests, if D2 drops Lilex entirely.

---

## 6. Phases

Each phase ends with the full gates (`make check`) green and a capture of its scenes in light and
dark. The captures are reviewed side by side with `references/facelift.png`.

**Phase 1 — Menu-bar shell (no visual change yet).**
`LSUIElement`, activation policy, daemon, status item with template symbol, left-click toggle,
right-click menu with Quit, panel window with material + corner radius + shadow, anchoring,
hide on Escape / focus loss / second click, focus guards, pin flag. The current content renders
inside the panel unchanged.
*Exit:* the app has no Dock icon; the panel opens under the item on every screen and Space, and
over a full-screen app; it hides correctly; Quit works; idle CPU is 0 while hidden.

**Phase 2 — Tokens, typography, header and footer.**
New theme tokens (D2), system font, header (title, search, pin), footer (settings, contextual
add, hint/banner slot).
*Exit:* light/dark/Increase Contrast captures match the reference's palette and type scale.

**Phase 3 — Sidebar and destinations.**
Source list with Groups (counts, colors, hover `…`), Library (Agents, Recently used placeholder,
Settings), config location footer. `Destination` state, Settings moved into the pane, shell tests
rewritten for "selecting a row is a destination, not a toggle".
*Exit:* every existing group/category/launcher CRUD path is reachable from the new layout.

**Phase 4 — Segmented control and list rows.**
Segments with `All` + `+` + overflow, list rows with icon tiles, trailing tags, hover actions,
selection, double-click/Return launch, cross-group search Results, editor Category picker,
"Move to…", drag from row to segment.
*Exit:* the manual matrix lines for one-click select, double-click launch, and drag onto
category / own category / nothing pass in the panel.

**Phase 5 — Recently used and settings completeness.**
`RecentLaunches` port + JSON repository + tests, Recently used pane, Clear, Open/New/Save copy
config, Quit button, launch at login (D8).
*Exit:* recents survive relaunch, never enter the shared config, and are pruned when their
launcher disappears.

**Phase 6 — Polish and accessibility.**
Open fade (Reduce Motion aware), native context menus (4.9), group reorder by drag, VoiceOver node
model for the new tree, keyboard map, capture scenes, performance re-baseline, docs and
`AGENTS.md` rewrite completed.

---

## 7. `AGENTS.md` sections that must be rewritten

The current text contradicts the new design in these places, and agents will follow it if it is
not updated:

- **Design reference**: "Read both reference images" and "a simpler version of the visual
  structure in `references/ui.png` and `references/ui_2.png`" (product goal 3, Design direction,
  working rule 1, Phase 6 and acceptance criteria). These must point to `references/facelift.png`
  as the design the UI follows.
- **Product goal 7 and 8**: drill-down "one level at a time" and "deselecting a level hides
  everything below it". This becomes: groups in the sidebar, categories as segments, launchers as
  rows.
- **Design direction**: Lilex + `ADVANCE`, ink/parchment theme, "the system accent is not used",
  the icon rail, cards sized to the longest label. Replace with the system font, AppKit semantic
  colors, the user's accent, source list, and list rows (per D2).
- **Required layout** diagram and the three **rail rules**: replace with the panel diagram from
  section 2.
- **Interaction requirements**: hover actions now apply to rows; drag targets are segments;
  Command-comma selects the Settings destination; Escape hides the panel.
- **App shell**: "native winit-backed window … default near 1120 × 720" becomes the menu-bar
  agent, panel window, `LSUIElement`, and dismissal rules. The "native macOS app menu" requirement
  no longer applies to an accessory app.
- **Sidebar / Main content** sections: describe the source list, segmented control, rows, footer,
  and Recently used.
- **Persistence**: add `recents.json` as machine-local state next to the existing preferences.
- **Capture scenes** list and the **manual test matrix**: add panel positioning (multi-monitor,
  full-screen Space, auto-hiding menu bar), pin, focus-loss dismissal while an open panel is
  showing, and Quit.

---

## 8. Tests to add or rewrite

- **Application**: a launch success records a recent entry; a failure records nothing; a recents
  write failure does not fail the launch; a clear empties the list; recents for unknown launcher
  ids are filtered out.
- **Config**: recents JSON round-trips; the 20-entry cap and de-duplication; a corrupt recents file
  gives an empty list and does not touch the shared config.
- **Presenters**: the config location label for Application Support / iCloud / Dropbox / an
  arbitrary folder; relative time formatting; sidebar counts; segment list with `All` first and
  `+` last; overflow split; trailing tag per destination; cross-group search rows with
  breadcrumbs.
- **Shell**: selecting the open group again keeps it open; opening a group selects `All`; segment
  memory per group; the footer add target per destination; Escape hides the panel only with no
  dialog; focus loss hides unless pinned or a system panel is open; a successful launch hides
  unless pinned; dropping a row on `All` does nothing; "Move to…" moves exactly one launcher.
- **Drag layer** (`iced_test`): a row lifted over a segment fills that segment; release elsewhere
  changes nothing.
- **Platform** (non-launching, manual-marked where they need a window server): screen-frame →
  logical-point conversion and clamping as pure functions with unit tests. Status item creation
  is a manual test.

---

## 9. Risks

- **Iced in a borderless panel**: text input must keep working in a borderless window
  (winit's `WinitWindow` reports `canBecomeKeyWindow = YES`; verify this in Phase 1 before
  building anything else). Clicks must work when the app is not active (first-click handling).
- **Material behind a wgpu surface**: the Metal layer must be transparent for the
  `NSVisualEffectView` to show through. `transparent: true` + clear color already do this for the
  current window, but check that the corner mask clips the Metal layer and not only the effect
  view.
- **Focus and Spaces**: the panel must open on the current Space and over full-screen apps, and
  activating the app must not move the user to another Space. This depends on the collection
  behaviour flags; test it explicitly.
- **Segmented control at scale**: groups with many categories are why the overflow is part of the
  core scope and not polish.
- **VoiceOver** remains a release blocker. The node model must be rebuilt for the new tree. D1's
  native alternative is the only option that removes this cost.

---

## 10. Implementation status

Implemented with the recommended answer to every decision in section 3: Iced drawn inside a
borderless panel window (D1); the system font and the user's system accent (D2); no caret, 6 pt
below the menu bar (D3); one click selects, a double click or Return launches, and a successful
launch puts the panel away (D4); no favorites (D5); a per-run pin (D6); the `terminal` SF Symbol as
a template image (D7).

**Built**

- Menu-bar shell: `LSUIElement`, an Iced daemon, the `NSStatusItem` with a left-click toggle and a
  right-click menu (Open, Settings…, Reveal Config, Quit), the panel window on every Space and over
  full-screen apps with the system shadow, anchoring under the item (with a fallback while the menu
  bar has not placed it yet), and dismissal by the item, Escape, a click elsewhere, or a launch.
- Header (title, search with `⌘F`, pin), source-list sidebar (groups with counts and colors, hover
  Rename/Delete, Library: Agents, Recently used, Settings; config location at the foot), pane
  heading, segmented control with All, `+`, and a "More" overflow, launcher rows with icon tiles,
  trailing tags and hover actions, footer with Settings, the contextual add action, and the hint or
  status line.
- Recently used: the `RecentLaunches` port, `JsonRecentLaunches` (`recents.json`), recording on a
  successful launch, the pane, and Clear History.
- Settings as a destination: appearance, terminal, config path with Reveal / Reload / Open… /
  New… / Save a Copy…, history, keyboard list, Quit.
- Cross-group search, a Category picker in the launcher editor (adding from All, and moving),
  drag from a row to a category segment, ⌘1–9, ←/→ segments, ↑/↓ rows, ⌘W, ⌘Q, and keys a focused
  field uses staying with the field.
- The accessibility model rebuilt for the new tree; capture scenes for every screen.
- Tests: application (recents), config (`recents.json`), platform (panel placement), presenters,
  shortcuts, accessibility, and the shell (panel behaviour, destinations, segments, editing,
  dragging).

- Launch at login (D8): a Settings switch over `SMAppService`, called through the Objective-C
  runtime so no new crate was needed; it says when the system wants approval, and that it is only
  available from the app bundle.
- Native right-click menus (4.9) on launcher rows, group rows, and category segments: a real
  `NSMenu` at the pointer, offering the same actions as the hover buttons.
- The opening fade (4.7): 120 ms on `NSWindow`'s animator, skipped under Reduce Motion.
- Reordering groups by dragging their sidebar rows (Phase 6), with the target row outlined.
- Increase Contrast: stronger hairlines and secondary text, stronger pill outlines, and an
  outlined selected row.

**Verified on the running app** (`DESPATCH_LIVE_TEST`, see `AGENTS.md`): the item opens and closes
the panel, Escape closes it, deactivation closes it unless pinned; segment clicks, a real pointer
drag of a row onto a segment, a group drag in the sidebar, ⌘N and typing into the editor with its
validation, a native context menu, a double-click launch through Terminal.app that ran its
command, the launch appearing in Recently used, Open at login on and off, the accessibility tree
VoiceOver receives, the fade and its absence under Reduce Motion, the Increase Contrast theme, and
the panel's transparent corners and shadow measured in the composited window, every pressable
accessibility element framed on screen inside the panel, and elements pressed the way VoiceOver
presses them — 37 checks, all passing, in each of the default, Reduce Motion, and Increase Contrast
runs.

The launch check passed both through Terminal.app and through Ghostty's scripting path, which is
the default terminal on the machine it was run on. The final build passed all 37 checks through
Ghostty as well, and the launched command ran in a new Ghostty window.

**Not automated**: listening to VoiceOver. The live test confirms the panel publishes its whole
accessibility tree in order, with every control framed where it is drawn and pressable the way
VO-Space presses it; hearing it read is the manual VoiceOver matrix in `AGENTS.md`, a release step
for a person with VoiceOver on (⌘F5).