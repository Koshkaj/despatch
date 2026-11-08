# AGENTS.md - Despatch macOS app

This file is the authoritative repository guide for agents and contributors working in this
repository.


## Product goal

Build **Despatch**, a native macOS menu-bar utility for organizing and launching server and CLI
connections. It lives behind a status item in the menu bar — no Dock icon, no main window — and
opens a panel under that item.

The finished app must:

1. Use **Rust** for the application and domain code.
2. Use **Iced** with its `wgpu` renderer for the GPU-rendered macOS UI.
3. Look like `references/ui.png`'s successor, `references/facelift.png`: an `AppKit`-style panel
   with a header (title, search, pin), a source-list sidebar, a list pane with a segmented control,
   and a footer bar. `facelift.md` records how each part of that screenshot maps onto Despatch.
4. Show the server **Groups** in the sidebar, and at its foot **Recently used** and **Settings**.
   Despatch is SSH only: there are no agent or custom-command launchers in the interface.
5. Let users create, rename, and delete server groups, such as `Production` or `Staging`.
6. Let users split a group into subgroups (categories) such as `Web`, `Workers`, or
   `Bastions`. Launchers always live in a subgroup, never directly in a group.
7. A selected **group** fills the pane: a segmented control offers **All** and each of its
   categories, and the pane lists the launchers of the chosen segment as rows. Selecting a group is
   choosing a destination, not a toggle — selecting it again keeps it open.
8. A config written by an earlier version may hold an `agents` section with agent and custom
   launchers. It still opens: those groups are kept in the file untouched and never shown,
   searched, or listed in Recently used. Nothing new is ever added to them, and nothing seeds them.
9. Let users create, edit, move, reorder, duplicate, and delete launchers.
10. Launch an SSH connection in the user's system-associated terminal when a server tile is
   activated.
11. (Removed: agent CLI launchers.)
12. (Removed: custom CLI launchers.)
13. Persist all groups and launchers in a human-readable, versioned JSON config file.
14. Let the user select where that config file lives so it can be placed in a shared, synced, or
    version-controlled folder.
15. Keep machine-local state — the selected config path, the appearance, the terminal picked for
    launches, and the launch history behind "Recently used" — outside the shared config.
16. Never store passwords, API tokens, SSH private-key contents, or other secrets in the shared
    config.

The working product name and bundle identifier are:

- Product name: `Despatch`
- Bundle identifier: `com.despatch.app`

Changing either is a product decision and should not happen silently.

## Design direction

The design reference is `references/facelift.png` (local only: `references/` is git-ignored). Read
it, and `facelift.md`, before changing the panel's layout.

Design read: a focused developer utility in the native macOS language of a menu-bar extra — an
`AppKit` popover's proportions, controls, and type, drawn by Iced.

Design settings:

- Design variance: 3/10
- Motion intensity: 2/10
- Visual density: 7/10 — a fixed 720 × 520 panel. Rows fill the pane's width; only the segmented
  control's segments are measured, to decide how many fit before the rest go into a "More" menu.
- Typography: San Francisco, the system font, selected by its font-database family name
  `"System Font"` (`/System/Library/Fonts/SFNS.ttf`). It is one variable font; the Medium and
  Semibold weights are positions on its weight axis, which the shaper applies. It is proportional,
  so `presenters::ADVANCE` is an *estimate* used only to decide where an ellipsis goes; a test in
  `typography` checks the estimate is never narrower than typical labels set in the installed font.
- App icon: `apps/despatch/resources/icon.png` is the single source, a 1024pt canvas holding the
  824pt artwork on Apple's macOS icon grid with transparent corners. `bundle-app.sh` derives every
  rendition from it with `sips` and `iconutil`, so no size can go stale on its own, and no `.icns`
  is committed. The menu bar shows an SF Symbol template image (`terminal`), never the app icon:
  menu-bar images must be monochrome templates the menu bar tints itself.
- Theme: `AppKit`-like semantic tokens in `theme.rs` — a light grey chrome ground, a brighter list
  pane, label/secondary/tertiary text, hairline separators — with light and dark values. The accent
  is the user's **system accent color**, read through `system_appearance()`. The appearance follows
  the system by default and can be pinned to light or dark in Settings; that choice is a
  machine-local preference, never part of the shared config.
- **Dark-mode hairlines and tracks are opaque grays, not translucent white.** The renderer blends
  in linear light, where a 10% white overlay comes out several times brighter than it does in
  `AppKit`, and every separator reads as a bright rule.
- Shape system: a 12 pt panel, 7–8 pt fields, buttons, segments and icon tiles, 6 pt source-list
  rows.
- Icons: SF Symbols rasterized once on a 64-point canvas and tinted per role, plus the bundled
  provider marks. No hand-drawn substitutes.

Required layout:

```text
┌────────────────────────────────────────────────────────────────────┐
│ ▣ Despatch      [🔍 Search servers                       ⌘F]   📌 │
├──────────────────┬─────────────────────────────────────────────────┤
│ Groups        +  │ Production                     2 servers in Web ✎🗑│
│ ▓ Production   6 │ ┌ All ┬▓Web▓┬ Workers ┬ Bastions ┬ More ▾ ┬ + ┐    │
│   Staging      2 │ [▣] API server                                  │
│   Home lab     3 │     deploy@api.example.com                      │
│                  │ ─────────────────────────────────────────────── │
│                  │ [▣] Frontend                                    │
│                  │     deploy@web.example.com                      │
│                  │                                                 │
│ ◷ Recently used  │                                                 │
│ ⚙ Settings       │                                                 │
├──────────────────┴─────────────────────────────────────────────────┤
│ + New server                       Double-click or press Return …  │
└────────────────────────────────────────────────────────────────────┘
```

- **Header**: the app's glyph and name over the sidebar's width; the search field starts where the
  pane does; the pin at the right keeps the panel open.
- **Sidebar**: a source list. The selected row is filled with the accent; others wash faintly on
  hover. A group's row trades its count for Rename and Delete while hovered. Where the config
  lives is shown and revealed from Settings, not from the sidebar.
- **Pane**: a heading with the destination's name and what it holds; for a group, the segmented
  control; then the rows. A search reaches across every group and tags each row with where it
  lives. Settings is a destination in the pane, not a sheet.
- **Footer**: the add action that fits the destination ("New server", "New category", "New
  group"), and on the right a hint — or the status line when there is news. Settings is not in the
  footer: it is at the foot of the sidebar.

Interaction requirements:

- A row responds immediately on pointer-down: one click selects, a double click or Return launches.
- Hover actions live in a `stack` layer above a row, and the pointer tracker must wrap that whole
  stack. `stack` levitates the cursor for the layers under whichever one reports a mouse
  interaction, so a tracker placed *under* the actions is told the pointer left the instant they
  appear, and the row flickers instead of letting the actions be pressed.
- A label color tints a control rather than flooding it: a row's icon tile takes a wash and a border
  in the color, and the symbol is drawn in it; text keeps the ordinary label color.
- Color cascades down one path and is set in two places. A **group** carries a label color. Its
  **categories** have none of their own. A **launcher** is drawn in its group's color too, but may be
  given its own in the launcher editor, which then wins. Clearing a launcher's color returns it to
  the group. Do not add a color of its own to a category: it would be a setting that could never
  take effect.
- A status line that reports an *event* — a launch result, a failed action — clears itself after
  ten seconds. One that reports a *state*, such as a conflicted or unsaved config, stays as long as
  the condition holds: hiding it would be lying about the file. Dismissals carry the token of the
  line they were started for, so one that lands after its line was replaced is ignored.
- Selection, hover, focus, disabled, empty, loading, success, and error states must all exist.
- Sheets open above the panel behind a scrim that follows the panel's rounded corners.
- Motion is limited to useful state feedback and must respect reduced-motion preferences.
- The panel is glass: a system material stands behind it, and the chrome and list pane only tint
  it, the pane more strongly so rows read against a calmer ground. Reduce Transparency paints both
  solid.
- Keyboard navigation and VoiceOver labels are required, not optional polish.
- Shortcuts: Command-N adds an item, Command-Shift-N adds a group, Command-F focuses search,
  Command-comma opens Settings, Command-1…9 jumps to a group, ←/→ change the segment, ↑/↓ move
  through the rows, Return launches the selection (or the first search result), Delete requests
  deletion, Escape closes a sheet, then a search, then the panel, Command-W hides the panel, and
  Command-Q quits. Keys a focused text field uses — Delete, Return, ←/→ — stay with the field.

Do not add generic web-dashboard patterns, purple glows, decorative gradients, fake status dots,
custom-drawn icons, or perpetual animation.

## Technology decisions

### Language and toolchain

- Stable Rust, using the current stable edition supported by all locked dependencies.
- A Cargo workspace with a checked-in `Cargo.lock` because this is an application.
- Minimum supported macOS version: macOS 13 unless a required Iced/wgpu API proves otherwise.
- Architecture targets: Apple Silicon first; the packaging pipeline must remain ready for a
  universal `arm64` + `x86_64` build.

### GPU-rendered UI

- Use Iced 0.14.x with default features disabled and explicit `wgpu`, `basic-shaping`, and
  `thread-pool` features. The thread pool is Iced's required executor; do not add Tokio or Smol
  unless an implemented use case justifies it.
- Pin the exact compatible Iced version in `Cargo.lock` once the scaffold builds.
- The production renderer on macOS is Iced's wgpu renderer over Metal. Do not silently enable
  Tiny-Skia, WebGL, browser/webview, or unconditional-rendering backends.
- Keep Iced elements, messages, tasks, subscriptions, themes, renderer, and event-loop objects
  inside the UI adapter. Domain and application crates must not depend on Iced, winit, wgpu,
  AppKit, Objective-C, or macOS-only types.
- Use Iced's Elm-style update/view boundary: UI messages translate to application commands and
  rendered elements derive from presenter state. Views never mutate domain models directly.
- Iced has no macOS-native widget theme. Build a focused Despatch theme with semantic tokens,
  the bundled typeface, macOS spacing and control geometry, clear pressed/focus states, and
  restrained motion. Do not imitate macOS with web-dashboard chrome.
- Prefer Iced/winit APIs for windows, pointer and keyboard input, menus, and accessibility. If a
  required macOS capability is unavailable, isolate the smallest possible bridge in
  `despatch-platform-macos`; never scatter Objective-C calls through view code.
- Iced 0.14 does not provide a built-in accessibility tree. VoiceOver remains required: implement
  an isolated macOS accessibility adapter in Phase 4 and do not claim accessibility completion
  until labels, roles, focus, selection, and actions pass the manual VoiceOver matrix.
- Every `unsafe` block must include a precise `// SAFETY:` explanation of ownership, lifetime,
  thread, selector, and nullability assumptions.
- Keep idle memory and frame behavior measurable. Phase 0 records idle RSS for a release build;
  later UI phases must fail performance review if a feature causes an unexplained material
  regression.
- Do not replace Iced with Tauri, Electron, SwiftUI, AppKit bindings, or another GUI framework
  without explicit user approval.

### Core libraries

Use dependencies narrowly and prefer maintained crates with small API surfaces:

- `serde` and `serde_json` for the shared config format
- `uuid` for stable group and launcher identifiers
- `thiserror` for library error enums
- `anyhow` only at the executable/bootstrap boundary
- `tracing` and a macOS-appropriate subscriber for diagnostics with secret redaction
- `tempfile` for tests and atomic-write staging where appropriate
- `directories` for the default Application Support location
- `notify` only if external config watching cannot be implemented cleanly with native file events

Do not add a dependency until its use is justified in the change that introduces it.

## Architecture

Use a Cargo workspace with dependency direction pointing inward:

```text
apps/despatch
    -> despatch-ui-iced
    -> despatch-application

crates/despatch-ui-iced
    -> despatch-application
    -> despatch-domain
    -> despatch-platform-macos

crates/despatch-platform-macos
    -> despatch-application ports
    -> despatch-domain

crates/despatch-config
    -> despatch-application ports
    -> despatch-domain

crates/despatch-probe
    -> despatch-application ports

crates/despatch-application
    -> despatch-domain

crates/despatch-domain
    -> no project crate
```

Planned workspace structure:

```text
.
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── clippy.toml
├── references/
│   ├── ui.png
│   └── ui_2.png
├── apps/
│   └── despatch/
│       ├── Cargo.toml
│       ├── build.rs
│       ├── resources/
│       │   ├── Assets.xcassets/
│       │   └── Info.plist
│       └── src/
│           └── main.rs
├── crates/
│   ├── despatch-domain/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── group.rs
│   │       ├── launcher.rs
│   │       ├── path_template.rs
│   │       └── validation.rs
│   ├── despatch-application/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── commands.rs
│   │       ├── errors.rs
│   │       ├── ports.rs
│   │       ├── state.rs
│   │       └── use_cases/
│   ├── despatch-config/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── atomic_write.rs
│   │       ├── json_repository.rs
│   │       ├── lib.rs
│   │       ├── migration.rs
│   │       └── watcher.rs
│   ├── despatch-platform-macos/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── default_terminal.rs
│   │       ├── lib.rs
│   │       ├── local_preferences.rs
│   │       ├── script_file.rs
│   │       └── workspace.rs
│   └── despatch-ui-iced/
│       ├── Cargo.toml
│       └── src/
│           ├── app.rs
│           ├── controllers/
│           ├── lib.rs
│           ├── presenters/
│           ├── style/
│           ├── views/
│           └── widgets/
├── tests/
│   └── fixtures/
│       ├── config-v1.json
│       └── config-invalid.json
├── scripts/
│   ├── bundle-app.sh
│   ├── make-dmg.sh
│   └── verify-app.sh
└── .github/
    └── workflows/
        └── ci.yml
```

The structure is a target, not permission to create empty placeholder modules. Add a module when
the implementation needs it. Keep each crate cohesive and avoid circular dependencies.

### Dependency rules

- `despatch-domain` contains data and invariant logic only. No filesystem, process, UI, or Iced
  imports.
- `despatch-application` owns use cases and declares ports as traits. It does not know which JSON
  library, terminal app, or widget implements them.
- `despatch-config` implements the config repository and schema migrations.
- `despatch-probe` implements the host-reachability port over plain TCP. It is a separate crate
  rather than part of `despatch-platform-macos` because nothing in it is macOS-specific.
- `despatch-platform-macos` implements terminal launching, local preferences, Launch Services
  interaction, and any macOS-only paths.
- `despatch-ui-iced` renders state and translates user actions into application use cases. It
  does not write files or spawn processes directly.
- `apps/despatch` performs dependency wiring, logging setup, app startup, and packaging metadata.
- Cross-thread values must be `Send`/`Sync` where appropriate. Iced window and event-loop objects
  remain on the main thread; background work returns through Iced tasks or subscriptions.
- Avoid global mutable state. Prefer explicit ownership, `Arc` for shared services, and channels or
  a main-thread dispatcher for UI updates.

## Domain model

Use stable UUIDs and tagged enums. The initial conceptual model is:

```rust
struct AppConfig {
    schema_version: u32,
    revision: u64,
    groups: Vec<Group>,
}

struct Group {
    id: GroupId,
    name: String,
    section: Section,
    /// An optional `#rrggbb` label color; `None` follows the section accent. It cascades to the
    /// group's categories and to every launcher in them without a color of its own.
    color: Option<LabelColor>,
    subgroups: Vec<Subgroup>,
}

struct Subgroup {
    id: SubgroupId,
    name: String,
    items: Vec<Launcher>,
}

enum Section {
    Servers,
    Agents,
}

enum Launcher {
    Ssh(SshLauncher),
    Agent(AgentLauncher),
    Custom(CustomLauncher),
}

enum AgentProvider {
    Codex,
    Claude,
    Gemini,
    OpenCode,
    Custom,
}
```

Domain rules:

- IDs are unique across their entity type and never change during rename, reorder, or migration.
- Group names are trimmed, non-empty, and unique case-insensitively within a section.
- Subgroup names are trimmed, non-empty, and unique case-insensitively within a group. Two groups
  may each hold a subgroup of the same name.
- Launcher names are trimmed, non-empty, and unique case-insensitively within a subgroup.
- A group holds no launchers of its own; every launcher belongs to exactly one subgroup.
- Server groups accept SSH launchers. Agent groups accept built-in and custom CLI launchers.
- SSH ports are in `1..=65535`; the default is 22.
- SSH hosts and usernames are stored as structured values, never concatenated raw into a command.
- Agent arguments are stored as an argument vector, not as one interpolated string.
- Custom launchers may opt into a raw shell command, but the UI must clearly label it as trusted
  code that will execute with the user's privileges.
- Config files store private-key paths, never private-key contents or passwords.
- Paths may be absolute, relative to the config file, or portable templates such as `$HOME` and
  `${VARIABLE}`. Expansion is performed by Rust code using an allowlisted parser, never by `eval`.
- Unknown future schema versions are rejected without rewriting the file.

## Shared config format and persistence

The shared config is pretty-printed JSON with `snake_case` fields and an explicit
`schema_version`. The current version is **2**; version 1 (which hung launchers directly off a
group) is migrated on load into one subgroup named `General` per group, after a `.v1.bak` copy of
the original file is kept beside it. Keep the format reviewable in Git and stable across machines.

`color` is optional on a group and on each launcher, and never appears on a subgroup. It is a
`#rrggbb` string, omitted entirely when no color is set, so an uncolored config is byte-for-byte
what it was before label colors existed and no schema bump is needed.

Example shape:

```json
{
  "schema_version": 1,
  "revision": 4,
  "groups": [
    {
      "id": "018f46f2-bde0-7cc8-a875-6cbe1a29a227",
      "name": "Production",
      "section": "servers",
      "color": "#ff3b30",
      "subgroups": [
        {
          "id": "018f46f3-0000-7000-8000-00000000a001",
          "name": "Web",
          "items": [
            {
              "type": "ssh",
              "id": "018f46f4-1a77-7df2-a70a-85ad1c7cb842",
              "name": "API server",
              "host": "api.example.com",
              "user": "deploy",
              "port": 22,
              "identity_file": "$HOME/.ssh/id_ed25519",
              "color": "#34c759",
              "extra_args": []
            }
          ]
        }
      ]
    },
    {
      "id": "018f46f6-1d8d-719a-ab37-4cdf0a97924d",
      "name": "Project tools",
      "section": "agents",
      "subgroups": [
        {
          "id": "018f46f3-0000-7000-8000-00000000b001",
          "name": "Assistants",
          "items": [
            {
              "type": "agent",
              "id": "018f46f7-319c-7ce2-8111-94aad70bb783",
              "name": "Codex",
              "provider": "codex",
              "executable": "codex",
              "working_directory": "../project",
              "args": []
            }
          ]
        }
      ]
    }
  ]
}
```

Persistence requirements:

- Default file: `~/Library/Application Support/Despatch/config.json`.
- Store only the chosen config path, the appearance choice, and the picked terminal in
  `NSUserDefaults` via the isolated macOS preferences adapter. `NSUserDefaults` only exposes safe
  setters for numbers and URLs, so anything else would need an `unsafe` block: store enumerations
  as their stable integer code rather than reaching for one.
- The launch history behind "Recently used" is machine-local and is **not** a preference: it is a
  list, which `NSUserDefaults` cannot hold safely. It lives in
  `~/Library/Application Support/Despatch/recents.json` (beside the *default* config, even when the
  config itself has moved to a shared folder), written atomically by `JsonRecentLaunches` behind
  the `RecentLaunches` port. A missing, damaged, or future-format history reads as empty and never
  stops the app; a history that cannot be written never fails the launch it records.
- Let users create a config, open an existing config, switch config, reveal it in Finder, reload it,
  and save a copy.
- Write to a sibling temporary file, flush and sync it, then atomically rename it over the target.
- Preserve the last valid in-memory state when decoding or validation fails.
- Keep a recoverable backup before a schema migration.
- Detect external changes. Reload automatically when there are no unsaved local edits; otherwise
  show a conflict state with Reload and Save Copy actions.
  `Application::check_config_file` does this: it asks the repository for the file's fingerprint
  (a hash of its bytes, nothing decoded) and only loads the file when that differs from the one the
  app last loaded or saved — every change made in the app is saved at once and its fingerprint
  kept, so the app's own saves never look external. The panel checks when it opens and every
  `CONFIG_CHECK_INTERVAL` (2 s) while it stays open, never while hidden, and never while a row or
  group is being carried. Polling a small file is used rather than file-system events because it
  behaves the same in sync folders and on network volumes, where those events are unreliable. A
  file that cannot be read is reported once, not on every check.
- Use a revision plus a content fingerprint to avoid silently overwriting a newer shared file.
- Debounce filesystem events and self-generated save notifications.
- Never silently reset a corrupt or future-version config to an empty config.
- Persistence errors must be visible and actionable in the UI.

## Command construction and terminal launching

Terminal launching is a port in the application layer and a macOS implementation in
`despatch-platform-macos`.

There are three ways a command reaches a terminal, and the terminal decides which:

- **On its command line.** A terminal known to accept `-e <argv...>` is opened with the command as
  arguments and nothing is written to disk. Environment variables ride along as arguments to
  `/usr/bin/env`, which execs the program, so they cost no process of their own.
- **A terminal is asked to do whatever it can for itself**, through `TerminalTraits`: where to
  start (`--working-directory=`), and whether it holds a window open when a command ends. Only what
  is left over is done by wrapping the command in `<shell> -c <fixed wrapper> despatch <dir>
  <command...>`, and that wrapper costs a shell process sitting between the terminal and the program
  the user asked for — visible in every process list, and named instead of the program in the
  terminal's own prompts. The wrapper is a **constant**, and every launcher value arrives as a
  positional parameter, so a host or argument carrying `;` or `$(` is data and can never become
  another command.
- **Through Ghostty's scripting API.** Ghostty deliberately shows “Allow Ghostty to execute …” for
  every executable supplied through its `-e` startup option, so it must never be added to
  `ARGV_TERMINALS`. Ghostty 1.3 and later expose an AppleScript surface API protected by a one-time
  macOS Automation permission, and it opens the session **in the Ghostty already running** — as a new
  tab in its front window, like Cmd-T, or as a new window when Ghostty has none open or is only
  being started (a starting Ghostty opens its own default window, and a tab beside it would leave a
  stray shell). The argv path goes through `open -n`, which starts a second copy of the whole
  application per launch.
  Nothing is staged on disk: the command, `initial working directory`, `environment variables`, and
  `wait after command` are properties of the surface, so Ghostty exports the variables itself and
  holds the window when a command fails. Launcher values reach `osascript` as arguments, never
  inside the AppleScript source.

  Two shapes of that API are already known to be wrong, both measured:
  - **`command` is shell text, not an argv.** Ghostty runs it as
    `/bin/bash --noprofile --norc -c 'exec -l <text>'`, so every word is quoted by
    `shell_script::quote` before it is joined. Never hand it an unquoted value.
  - **`direct:` is a configuration-file prefix and the API does not parse it.** A surface asked to
    run `direct:/var/folders/…/despatch-X.command` executes a file *named* `direct:/var/…`,
    relative to the working directory, and fails with `No such file or directory`.
- **As a staged `.command` file**, for every other terminal. There is no portable way to hand one
  an argument vector, and guessing a flag would silently open an empty window. Terminals run these
  files by appending `; exit` to a shell line, which is where "press any key to close the terminal"
  comes from — the file's own contents cannot prevent it, which is exactly why the argv path
  exists. Extend `ARGV_TERMINALS` only for terminals whose reviewed command-line path does not add
  its own per-launch confirmation.

**An SSH launch announces `TERM=xterm-256color`**, not the terminal's own type. `ssh` copies the
local `TERM` into the remote session, and a server's terminfo database has never heard of
`xterm-ghostty`, `xterm-kitty`, or `alacritty` — the remote shell then loses `clear`, `Ctrl-L`, and
every other capability that terminfo describes. Terminals paper over this with shell integration
that only loads in an interactive shell, which a launched command is not. Local launchers keep the
terminal's own type, which is correct for them.

**A launch runs under the user's own shell**, taken from `SHELL` and resolved in Rust, because an
argument vector is never expanded by anything. Only POSIX-family shells (`sh`, `bash`, `zsh`,
`dash`, `ksh`) are used as-is; anything else falls back to `/bin/sh`, since the wrapper is a POSIX
script and `fish`, for one, fails on its first line. The staged `.command` file takes the same shell
in its shebang, raw-shell launchers are run by it, and the session a failed launch hands over to is
`$SHELL` either way.

**No argument in an argv launch may ever be empty.** `/usr/bin/open --args` drops an empty one, and
every parameter after it shifts down: the wrapper reads the program as its working directory, fails
to `cd` into it, and the launcher never runs. A launcher with no working directory sends
`NO_DIRECTORY` (`-`) instead, which the wrapper reads as "the user's home directory".

**A launch never inherits its working directory.** A terminal opened through `/usr/bin/open` is
started by launchd, whose working directory is `/`, so a session that inherits one opens at the root
of the disk — including the shell the wrapper hands over to when a command fails. Both paths `cd`
explicitly: to the launcher's own directory, or to `$HOME` when it has none.

A command that **fails**, or that **ends within `MINIMUM_VISIBLE_SECONDS`**, hands over to an
interactive login shell, so the window stays with the status above a usable prompt. Anything else
simply ends and the terminal closes the window. The elapsed-time half of that rule is what makes a
launch debuggable at all: a terminal closes its window the instant the command exits, so a session
that dies on the spot reads as "the terminal opened and vanished" with the explanation destroyed
along with the window. This is why the bare-argv path was removed — handing the terminal
`[program, args...]` named the process more honestly, but left nothing running to hold the window.
The behaviour lives in the wrapper and in the staged script, so both paths behave the same.

Every argv launch opens a **new instance** of the terminal, because `open` only delivers `--args` to an
application it is starting; a terminal already running ignores them, and the command would silently
never run. That is why the argv path passes `-n`.

An argv-launched starting instance then does two things nobody asked for: it opens its own default window, and it
reopens whatever windows the app had when it was last quit. Either one makes a launch appear to open
two tabs — the one that was asked for, and a plain login shell beside it, which runs the user's whole
shell startup and can put a permission prompt on screen for it. `TerminalTraits` hands a terminal
only reviewed switches that turn those behaviours off. A terminal with no such switches is handed
nothing: guessing a flag is worse than the extra window.

Ghostty 1.3's AppleScript surface API reuses the running application and opens a new tab in its
front window (a new window when it has none). It is the only implemented reuse path. Do not replace it with `-e` (which restores the per-launch
confirmation) or synthesised keystrokes (which need Accessibility permission and race focus).

Terminal.app and iTerm2 *can* be driven with Apple Events (`do script`, `create window`), which
would reuse a running instance for those two. That is the route to take if this is revisited; it
costs a one-time Automation prompt per terminal and must fall back to a new window when denied.

Expected flow:

```text
Tile activation
  -> validate launcher
  -> resolve portable paths and executable
  -> build a structured launch request
  -> render a temporary .command script with shell-safe argv quoting
  -> chmod 0700
  -> resolve which terminal the user chose
  -> for Ghostty, create a window through its Automation-authorized scripting API
  -> otherwise ask macOS Launch Services to open the file, naming that terminal
  -> the chosen terminal executes it
  -> temporary script removes itself
```

Rules:

- "The user's terminal" is whichever application they chose in Launch Services, read from
  `~/Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist`. Consult
  `public.unix-executable` first and `com.apple.terminal.shell-script` second, and pass the
  resulting bundle identifier to `open -b`.
- Do **not** open a `.command` file without naming an application. That content type is
  `com.apple.terminal.shell-script`, whose stock handler is Terminal.app, so Ghostty, iTerm2,
  WezTerm, and Kitty users would land in Terminal even though they set a default terminal — those
  apps claim `public.unix-executable` instead.
- When the user has chosen nothing, fall back to the plain system association.
- Invoke `/usr/bin/open` with the script path as a process argument, or use the equivalent
  `NSWorkspace` API behind the macOS adapter. Never build an `open` command through a shell.
- Settings → "Launches open in" picks the terminal: **System default** (the association above,
  named when it names one) or any installed terminal from `KNOWN_TERMINALS` in
  `default_terminal.rs` — Terminal, iTerm2, Ghostty, WezTerm, kitty, Alacritty, Warp — each of
  which has a reviewed launch path. The pick wins over the association, takes effect at once, and
  is stored as that list's stable integer code (the codes never change). Installed terminals are
  found through `NSWorkspace` when Settings opens, never on every frame.
- Render argv using one audited POSIX-shell quoting function. Do not concatenate untrusted values.
- SSH command order should be deterministic: executable, identity, port, extra args, destination.
- Agent presets initially resolve to `codex`, `claude`, `gemini`, and `opencode`, with an editable
  executable path and argv.
- Resolve executables using the login shell's expected environment or an explicit path. GUI apps
  often inherit a smaller `PATH`; this must be handled deliberately and tested.
- **Finding the file is only half of it: the session gets the shell's `PATH` too.** Launch Services
  starts Despatch with `/usr/bin:/bin:/usr/sbin:/sbin`, and a launched program inherits it. An
  agent CLI is often a `#!/usr/bin/env node` script — `codex` and `gemini` both are — so a launch
  that resolved the file perfectly still dies at the shebang with `env: node: No such file or
  directory`, while a compiled `claude` or `opencode` in the same list works. `ExecutableResolver::
  session_path` answers with the user's own `PATH`, and it is added to every local launch (never to
  SSH, whose environment belongs to the remote host, and never over a `PATH` the launcher itself
  sets). Ask the **interactive** login shell first: version managers install themselves in
  `~/.zshrc`, which `zsh -l` never reads, so the quiet answer is missing exactly the directory
  `node` lives in. It is asked once per run and cached.
- A missing CLI must produce an actionable error naming the unresolved executable.
- Do not log raw commands when they may contain user-entered sensitive arguments.
- Temporary scripts contain no secret material beyond the command the user explicitly requested,
  use mode `0700`, and self-delete as soon as the terminal has opened them.
- Launching is never performed in unit tests. Tests use a fake `TerminalLauncher` that captures the
  structured request.

## Application state and use cases

Use an event-driven application state rather than letting views mutate models directly.

Core state:

- Loaded config and its source path
- Current config fingerprint and dirty/conflict state
- Selected section, group, subgroup, and launcher, where deselecting a level clears everything
  below it and nothing is ever auto-selected on the user's behalf
- Search query and filtered presentation state
- In-progress sheet/dialog
- Last launch result or persistence error

Core use cases:

- Load default config
- Open or create config at a selected path
- Save config and save a copy
- Resolve an external-file conflict
- Add, rename, reorder, and delete group
- Add, rename, reorder, and delete subgroup
- Add, edit, move, reorder, duplicate, and delete launcher
- Filter groups and launchers
- Launch selected item
- Reveal config in Finder
- Restore local window and selection preferences

Use-case code returns typed results. Views decide how to present them, but they do not infer error
meaning from strings.

## Iced UI implementation

### App shell

- Initialize the dependency graph before starting Iced's main-thread event loop.
- Despatch is an Iced **daemon**: it opens one window at start and keeps running with no window on
  screen. `LSUIElement` in `Info.plist` keeps it out of the Dock and gives it no menu bar of its
  own; a binary started outside the bundle (`cargo run`) is made an accessory app through the
  platform bridge instead.
- **The panel window** is borderless, transparent outside the panel's 12 pt corners, floating
  (`Level::AlwaysOnTop`), not resizable, 720 × 520, and opened hidden. The iced style paints no
  window background, so those corners really are transparent. The platform bridge
  (`panel_window.rs`) joins it to every Space and lets it float over full-screen apps, gives it the
  system shadow, and recomputes that shadow once the first frame is on screen.
- **Showing and hiding go through the bridge**, not `window::open`/`close`: iced 0.14 has no
  visibility task, and keeping one window for the life of the app keeps the scroll position, the
  search, and any open sheet. Showing activates the app (an accessory app is not active while its
  panel is hidden) and focuses the search field; hiding with Escape or the item also hides the app,
  so the keyboard returns to the app the user came from.
- **The status item** (`status_item.rs` in both the platform and UI crates) is an `NSStatusItem`
  with a template SF Symbol. A left click toggles the panel; a right or Control click opens a native
  menu — Open Despatch, Settings…, Reveal Config in Finder, Quit Despatch. Its target object also
  observes `NSApplicationDidResignActiveNotification`. Events reach Iced through a channel turned
  into a subscription; no Objective-C type leaves the platform crate.
- **The item is laid out after it is created.** Its window reports an empty frame at the origin
  until the menu bar has placed it, so the panel never hangs from an unplaced item: the first show
  at launch waits a moment, and an anchor without a placed item falls back to the right end of the
  menu bar. `panel_top_left` centers the panel on the item, clamps it to the item's screen, and
  hangs it 6 pt below the menu bar.
- **Dismissal**: the item toggles; Escape closes a sheet, then a search, then the panel; another app
  becoming active (a click anywhere outside) hides it; a successful launch hides it. A **pinned**
  panel survives the last two. The pin is per run, never stored. File panels opened from the panel
  are modal to Despatch, so they do not deactivate it and do not hide the panel.
- Without a Dock icon or app menu, **Quit** lives in Command-Q, in Settings, and in the item's
  menu.
- **Open at login** is a Settings switch over `SMAppService.mainApp`, reached through the
  Objective-C runtime (`login_item.rs`). It only works from the app bundle; outside one the switch
  is disabled and says so, and a registration waiting for approval says where to approve it.
- **Glass**: `configure_panel` puts a system material behind the interface — Liquid Glass
  (`NSGlassEffectView`) from macOS 26, the popover `NSVisualEffectView` before — in the window's
  frame view, directly below winit's content view. winit requires its view to stay the content view,
  so the material cannot wrap it; below it, Iced's translucent pixels composite over the material and
  the content view still receives every click. The window's appearance is set to the interface's
  light or dark, so the glass and native menus match a theme pinned in Settings. The ground and pane
  colors are translucent tints (`theme::ground`, `theme::pane`); Reduce Transparency travels in the
  theme's name, like Increase Contrast, and makes them solid.
- **Motion**: the panel fades in over 120 ms on `NSWindow`'s animator, and simply appears when the
  system asks to reduce motion. Hiding is immediate.
- **Increase Contrast** selects a separate theme (the setting travels in the theme's name, since an
  Iced theme carries nothing else): stronger hairlines and secondary text, stronger pill
  outlines, and an outlined selected row.
- The panel opens once at launch, so starting the app visibly does something.
- Retain the root component and every Rust callback/model owner for at least as long as the panel
  displays them. All UI state transitions stay on the Iced event-loop thread.

### Sidebar

- A source list. **Groups**: one row per server group, with its label color on its symbol and its
  launcher count; hovering a row swaps the count for Rename and Delete; a `+` beside the heading
  adds a group. There is no "Library" heading.
- At the foot of the sidebar, below the groups however many scroll above them: **Recently used**
  (count once there is history) and **Settings**. Where the config lives is in Settings, and an
  unsaved or conflicted file is said in words in the footer's status line.
- A row centers its symbol and label vertically. A `button` lays its content out from the top, so
  a fixed-height row wraps its content in a container that centers it.
- Selecting a row is choosing a destination. Selecting the open group again keeps it open. Returning
  to a group returns to the segment it was left on (per run).
- A group row is pressed, not clicked: the press opens the group, and the same press can carry it
  up or down the list (through `drag_layer`) to take another group's place, which is outlined while
  it would receive it.
- **Hover exits name what they leave.** Moving up or left, the widget entered is told before the one
  left, so an exit clears the hover only if it still belongs to the widget leaving. An
  unconditional "hover: none" on exit wiped out the target of every upward or leftward drag.
- Right-clicking a launcher row, a group row, or a category segment opens a native `NSMenu`
  (`context_menu.rs`) with the same actions its buttons offer.

### Main content

- Heading: the destination's name, and what it shows ("6 servers", "2 servers in Web"). With a
  category open, that category's Rename and Delete follow the count.
- **Category pills**, for a group: **All**, each category in config order, and `+` to add one.
  Each pill is a separate capsule as wide as its name, outlined in a quiet gray; the selected one
  is outlined in the accent over a faint wash of it, with its label in the accent. Categories that
  do not fit go into a "More" pop-up at the end, so a pill never wraps or shortens a name. A group with no category shows an empty state
  asking for one, because a launcher always lives in a category.
- **Rows**: a 30 pt icon tile (the launcher's symbol or provider mark, tinted with its label color),
  the name, the destination or command, and at the right a quiet tag: the category in "All", the
  group and category in search results, how long ago in "Recently used". Hover or selection swaps
  the tag for Edit, Duplicate, and Delete.
- **Recently used** lists successful launches, newest first, at most 20, kept on this machine in
  `recents.json` beside the default config (see persistence). Launchers that no longer exist are
  not listed and are dropped on the next write.
- **Search** reaches across every server group from any destination; Return opens the selected
  result or the first one. Choosing a sidebar destination clears the search.
- One click on a row selects it, two launch it, and Return launches the selection. A single click
  never launches: launching opens a terminal and takes the focus, which is too much to hang on the
  same gesture that merely picks something out of a list.
- A row is a `mouse_area` around a container, not a `button`. Three gestures start with the same
  press — select, launch, drag — and a `button` settles that press itself: it captures it, then
  publishes on release, and only if the release lands back inside its bounds.
- A launcher moves between categories by dragging its row onto a segment, or by choosing another
  category in the launcher editor. Past a small threshold the row lifts and follows the pointer as
  a floating copy; below it the gesture is an ordinary click. While a row is carried, every category
  segment except the one it lives in (and never "All") offers itself as a target, the one under the
  pointer is filled, and the row's hover actions stand down. Releasing anywhere else, the pointer
  leaving the panel, the panel losing focus, or Escape abandons the gesture. A dropped launcher
  lands at the end of the category; a list showing one category follows it there.
- The row reports its own press, so the launcher being carried is the one under the pointer by
  construction. The release comes from the event stream through a subscription that is only
  listened to while a row is held. Do not infer the pressed row from the last reported hover.
- **The floating row follows the pointer without messages.** It is drawn by `drag_layer`, which
  keeps the pointer position in its own state and publishes exactly one message per gesture — the
  moment the press becomes a drag. Any future "follow the pointer" behaviour belongs in a widget
  for the same reason: every message rebuilds and lays out the whole panel.
- **The panel is always the same two layers**, a base and an overlay, even when the overlay has
  nothing to show, so widget state such as scroll positions survives a press.
- The layer reports no mouse interaction, so the pointer still reaches the segments under it.
- Large collections use measured virtualization behind an isolated custom widget when Iced's
  standard `scrollable` would retain too many offscreen elements.

### Editors

- Group and category editor: one shared name field. A group editor also offers a label color,
  chosen from a preset palette or typed as hex; a category editor does not, because a category
  wears its group's color.
- SSH editor: name, host, and username, with port override, identity path, and extra argv behind an
  "Additional settings" disclosure, and a non-secret command preview. The disclosure opens by
  itself when the launcher being edited already uses one of those settings. The identity path can
  be typed or picked with the native open panel, which stores it as `$HOME/...` when it sits inside
  the home directory. Every launcher editor offers a label color that overrides the one inherited
  from its group; clearing it returns the launcher to the group's color.
- The SSH editor offers a reachability check next to the host. It opens a TCP connection to
  `host:port`, reads the identification string the server volunteers, and closes. It must never
  authenticate, run `ssh`, or write a single byte to the socket: a check that could fail a login
  would land in an auth log and could lock an account. The check is offered, never forced, and
  never blocks saving — a server that is switched off today is still worth writing down. Its
  verdict is stamped with the destination it ran against, so editing the host or port drops a stale
  result instead of showing it against the new address.
- There is no agent or custom-command editor. A launcher that is not SSH (from an older config) is
  never shown, so it is never edited.
- Labels are above fields; placeholder text is never the only label.
- Validate inline before Save and preserve entered data when validation fails.
- Destructive confirmation names the exact group or launcher and explains the impact.

### Accessibility and appearance

- Every icon-only control has an accessibility label and tooltip.
- Focus order follows visual order.
- Interactive targets are at least 28 x 28 pt; primary row/tile targets are larger.
- All text and focus states use semantic colors with sufficient contrast in light and dark mode.
- Respect Reduce Motion, Increase Contrast, and Reduce Transparency.
- Do not encode section, selection, validation, or launch status by color alone.
- Support keyboard-only creation, editing, navigation, launch, and deletion.

## Error handling and observability

- Library crates use typed `thiserror` enums.
- The executable may add context with `anyhow`, then translate failures into user-facing errors.
- Never panic for invalid config, missing executables, filesystem errors, or launch failures.
- Panics indicate violated programmer invariants only.
- Log lifecycle, config source, migration version, save outcome, and launch outcome with `tracing`.
- Redact hosts only if a future privacy mode requests it; always redact passwords, tokens, private
  key contents, and potentially sensitive argv values.
- User-facing errors state what failed, preserve user data, and offer the next safe action.

## Security rules

- Treat shared config files as executable intent. Show a trust warning the first time an existing
  config containing raw-shell launchers is opened.
- Structured SSH and agent launchers must not invoke `eval`, `sh -c`, or unquoted string
  concatenation.
- Raw-shell mode is isolated, visually marked, and never enabled by migration or default.
- Reject NUL bytes and line breaks where a single argv or hostname field is expected.
- Validate temp-file ownership and permissions.
- Avoid TOCTOU bugs when replacing config files; use same-directory atomic rename.
- No network telemetry by default.
- Do not store secrets in config, logs, crash reports, fixture files, or snapshots.
- `unsafe_code = "forbid"` in all non-UI, non-platform crates.
- Platform/UI crates deny unsafe code by default and allow only narrowly reviewed modules.

## Test strategy

Tests are designed around ports so almost all behavior runs without opening a macOS window or
terminal.

### Domain unit tests

1. A valid empty v1 config passes validation.
2. Duplicate group IDs fail validation.
3. Duplicate group names in the same section fail case-insensitively, and duplicate subgroup names
   in the same group fail the same way while two groups may reuse a name.
4. The same group name may exist once in Servers and once in Agents.
5. Empty or whitespace-only group and launcher names fail.
6. Server launchers cannot be inserted into agent groups and vice versa.
7. SSH ports 1 and 65535 pass; 0 cannot be represented as valid input.
8. Hosts, usernames, argv values, and paths containing NUL or forbidden line breaks fail.
9. Agent provider presets map to the expected default executable names.
10. Reorder operations preserve IDs and contain every entity exactly once.
11. Moving a launcher updates only its parent and ordering metadata.
12. Portable path expansion handles `$HOME`, `${VAR}`, relative paths, missing variables, and
    literal dollar signs without shell evaluation.

### Command-rendering unit tests

1. Basic SSH renders `ssh user@host`.
2. Non-default port renders `-p` with a separate quoted argv value.
3. Identity file renders `-i` with spaces safely quoted.
4. IPv6 destinations render correctly.
5. Hostnames, paths, and args containing spaces, quotes, dollar signs, semicolons, backticks, and
   command-substitution syntax remain a single inert argv value.
6. Empty optional values do not create empty flags.
7. SSH argument order is deterministic.
8. Each built-in agent preset renders the expected executable.
9. Working directories with spaces are resolved and quoted safely.
10. Structured launchers never emit `eval`.
11. Raw-shell launchers preserve the exact user-authored command only after explicit raw mode is
    present in the model.
12. Generated scripts set a useful terminal title, launch the command, report a non-zero exit, and
    self-delete.

### Config repository tests

1. JSON v1 fixture decodes and re-encodes without semantic changes.
2. Pretty output is deterministic for the same model.
3. Unknown future schema versions return a typed error and leave the file untouched.
4. Invalid JSON returns a typed error and preserves the last valid in-memory state.
5. Semantic validation failure identifies the offending group or launcher.
6. Atomic save creates the file with expected permissions.
7. An interrupted staged write leaves the prior config readable.
8. Migration creates a backup before replacing the source.
9. A stale fingerprint refuses to overwrite a newer external revision.
10. Self-generated filesystem notifications do not trigger a reload loop.
11. External edits reload when local state is clean.
12. External edits produce a conflict when local state is dirty.
13. Relative path resolution uses the selected config file's directory.
14. Switching config paths does not copy machine-local preferences into shared JSON.

### Application/use-case tests with fakes

1. First launch creates or loads the default config.
2. Add, rename, reorder, duplicate, move, and delete operations persist exactly once.
3. Repository failure keeps the edited state dirty and reports a recoverable error.
4. Selecting a different section clears an invalid group/item selection.
5. Search is case-insensitive and matches names, hosts, providers, and command details.
6. Launch selected item passes one structured request to the fake terminal launcher.
7. Invalid or missing launcher data never calls the terminal port.
8. Missing executable reports the executable name and keeps app state usable.
9. Save conflict exposes Reload and Save Copy actions.
10. Delete confirmation removes only the named entity.

### Reachability tests

The probe is exercised against a real listener on `127.0.0.1:0`, never against a remote host:

1. A port that sends an identification string reports it.
2. A port that accepts and stays silent is still reachable.
3. A closed port is `Refused`, not an error — an unreachable host is a fact, not a failure.
4. A hostname that does not resolve is `Unresolved`.
5. The probe writes nothing to the socket. This one is not optional: it is what keeps a check from
   ever looking like a failed login.

### macOS adapter tests

1. Temporary command files are created with mode `0700`.
2. Temporary filenames contain no user-authored path separators.
3. `/usr/bin/open` is invoked with a path argument, never a shell-formatted string.
4. Launch failure maps exit status and stderr to a typed platform error.
5. Local preferences round-trip the selected config path without entering shared JSON.
6. Finder reveal requests target the config file.
7. Tests clean up all temporary scripts and preference suites they create.

Any test that would open a terminal or Finder is ignored by default and clearly marked as a manual
macOS integration test.

### Presenter and UI-state tests

1. Empty Servers and Agents sections produce the correct creation prompt.
2. Group item counts and tile details are formatted correctly.
3. Validation errors attach to the correct editor field.
4. Loading, saving, dirty, conflict, launch-success, and launch-failure states map to visible UI
   states.
5. Keyboard actions map to the correct application command.
6. Context-menu availability matches selection and item type.
7. Light/dark semantic style tokens never use hard-coded appearance-specific text colors.

### Manual UI and accessibility test matrix

Before release, verify on at least the oldest supported macOS and the current macOS:

- Fresh install and empty state
- Light mode, dark mode, Increase Contrast, Reduce Transparency, and Reduce Motion
- The panel opening under the menu-bar item on every display, on a full-screen app's Space, and with
  an auto-hiding menu bar; closing on a second click, on Escape, on a click elsewhere, and after a
  launch; staying open while pinned; Quit from Command-Q, Settings, and the item's menu
- A group with more categories than fit (the "More" segment) and one with none
- Keyboard-only navigation and every declared shortcut
- VoiceOver labels, roles, focus order, and editor error announcements
- Add/edit/delete flows for server, each built-in agent, and custom CLI
- One click selects a row without launching, two launch it, and a double click repeated on
  several cards in a row launches every time
- Dragging a launcher onto another category segment, onto its own, onto "All", and onto nothing
- SSH launch through Terminal.app and at least one alternative terminal registered for `.command`
- Missing SSH key, missing CLI executable, invalid config, read-only config, and external-edit
  conflict
- Config in Application Support, iCloud Drive, Dropbox-like sync folder, and a Git working tree
- App relaunch restores selected config and valid selection without storing shared state locally

### Build and quality gates

Run these before handing off a feature:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release
```

When packaging exists, also run:

```bash
./scripts/bundle-app.sh --release
./scripts/verify-app.sh target/release/Despatch.app
codesign --verify --deep --strict --verbose=2 target/release/Despatch.app
```

The Makefile wraps the everyday commands: `make run` (bundle, then launch from inside it, with
output still attached to the terminal), `make bundle`, `make check` (the gates above), and
`make dmg`. `make-dmg.sh` bundles and verifies first, then stages the app beside an `/Applications`
symlink and writes a compressed, verified `target/release/Despatch-<version>.dmg`; the version in
the name comes from `CFBundleShortVersionString`, so `Info.plist` remains the one place a release
is numbered. The image is signed only ad-hoc, like the bundle — notarization is still open.

Debug builds compile **dependencies** at `opt-level = 3` (`[profile.dev.package."*"]`) while
workspace crates stay unoptimized and steppable. The renderer and the text shaper run on every
frame, and unoptimized they cost enough that the interface stops keeping up with a pointer. The
first build after changing that profile recompiles every dependency, which takes a few minutes;
afterwards it is incremental as usual.

CI must run format, Clippy, unit/integration tests, and a release build on macOS. Dependency audit
and license checks should be added before external distribution.

### Reviewing the interface without a human at the keyboard

`despatch-ui-iced` has an optional `capture` feature that renders one frame of the real GPU
interface to a file, so layout and appearance can be reviewed from a terminal:

```bash
DESPATCH_CAPTURE=/tmp/shot.bmp \
DESPATCH_CAPTURE_SCENE=add-server \
DESPATCH_CAPTURE_APPEARANCE=dark \
  cargo run -p despatch --features capture
sips -s format png /tmp/shot.bmp --out /tmp/shot.png
```

The scene is opened before the first frame is built and the app quits after writing the file. An
unfocused window is never asked to redraw, so only the first frame can be captured; add a scene to
`capture_scene` rather than trying to drive the app after launch. The feature is off by default and
must never be enabled in a release build.

A capture run shows the panel window centered on screen instead of under the menu-bar item, and
installs no item. Window corners outside the panel are transparent on screen but come out black in
the BMP, which has no alpha.

**The live self-test** checks the real menu-bar behaviour the same way, without Accessibility or
Screen Recording permission:

```bash
DESPATCH_LIVE_TEST=/tmp/live cargo run -p despatch --features capture
```

It installs the status item and presses it through `AppKit` (`performClick`, the path a real click
takes to the item's target); posts pointer and key events to the app's own queue, which winit reads
as real input, to click segments, drag a row onto a segment and a group up the sidebar, open the
editor with Command-N and type into it, and double-click a launcher; answers a native context
menu from the keyboard; turns Open at login on and off; reads back the accessibility tree, checks
every pressable element's frame on screen, and presses elements through
`accessibilityPerformPress` the way VoiceOver does; samples
the window's opacity during the fade; and photographs the composited panel and measures its
corners and shadow. Every check prints `live: PASS …` or `live: FAIL …`.

A process may message its own controls, post events to itself, and capture its own windows
without Accessibility or Screen Recording permission, which is why this works where synthetic
system events and `screencapture` do not. The script expects a purpose-made config (three server
groups, Production holding Web with two servers, Workers with one, and an empty Bastions; Staging
opening with a server whose only effect is a local `ProxyCommand` that touches a marker file; and an
agents group left over from an older version, which must stay hidden) under a throwaway `HOME`, and is run from a copy of the
bundle with its own bundle identifier so it never shares preferences with the real app. Never run
it against the user's config: it moves launchers and groups. `DESPATCH_LIVE_TERMINAL=<bundle id>`
launches through that terminal instead of the user's (Terminal.app needs no Automation permission),
`DESPATCH_LIVE_REDUCE_MOTION=1` and `DESPATCH_CAPTURE_CONTRAST=increased` force those settings, and
`views::probe` holds the panel coordinates the pointer events aim at.

The scenes are `all`, `category`, `add-server`, `add-server-all`, `add-server-advanced`,
`reachability`, `recent`, `settings`, `search`, `rename-group`,
`rename-category`, `delete`, `drag`, `hover`, and `pinned`. The `drag` scene shows what a held row
does to the segments — the drop targets offering themselves, the one under the pointer filled — but
not the floating row itself, which is drawn from real pointer events that a scene cannot produce.
Its behaviour is covered by the `drag_layer` tests instead, which drive a headless interface with
`iced_test` and compare drawn frames. A capture runs against the config the app
would really open, so reviewing something the current config does not contain — label colors, for
instance — means pointing the app at a throwaway config for the run and putting the real one back
afterwards, never editing the user's config to stage a screenshot.

## Implementation plan

### Phase 0 - repository and build scaffold

- Create the Cargo workspace and crate skeleton only as modules acquire real code.
- Add `rust-toolchain.toml`, formatting/lint settings, macOS target metadata, and CI.
- Add Iced with explicit wgpu, basic text-shaping, and built-in thread-pool features; disable
  default and fallback renderer features.
- Build a minimal Iced window and prove that its macOS release build uses the GPU renderer.
- Add a small bundling script that produces a runnable `.app` with `Info.plist` and resources.

Exit criteria:

- `cargo build --workspace` succeeds on macOS.
- The bundled app opens a native titled window and quits cleanly.
- The release app uses Iced's wgpu renderer over Metal and its idle RSS is recorded as a baseline.
- No domain/application crate imports Iced, winit, or wgpu.

### Phase 1 - domain and versioned config

- Implement IDs, sections, groups, launcher variants, providers, validation, and path templates.
- Define JSON schema v1 and fixtures.
- Implement repository load, validation, deterministic encode, atomic save, fingerprints, and typed
  errors.
- Implement default config location and local selected-path preference.

Exit criteria:

- Domain and config test suites pass.
- A sample shared config can round-trip byte-stably after canonical formatting.
- Invalid/future config never gets overwritten.

### Phase 2 - application state and CRUD use cases

- Define application ports for config, local preferences, terminal launch, and file reveal.
- Implement app state, selections, dirty/conflict state, search, and use cases.
- Add fakes and comprehensive behavior tests.

Exit criteria:

- All CRUD and selection behavior is proven without a UI.
- Save errors and external conflicts preserve recoverable state.

### Phase 3 - safe terminal launch adapters

- Implement portable path and executable resolution.
- Implement audited shell argv quoting and `.command` script generation.
- Implement system-associated terminal launching and Finder reveal on macOS.
- Add fake-based unit tests and non-launching macOS integration tests.

Exit criteria:

- SSH and all four initial agent providers generate correct scripts.
- Injection corpus tests pass.
- Temp files are `0700`, launch arguments bypass the shell, and failures are typed.

### Phase 4 - Iced app shell and navigation

- Implement AppDelegate, app menu, window, split view, sidebar sections, group selection, and main
  content container.
- Build presenters that convert framework-free app state into UI-specific rows and tiles.
- Implement system appearance, design tokens, keyboard commands, empty/loading/error states, and
  accessibility labels.

Exit criteria:

- The sidebar visibly contains the server groups, and Recently used and Settings at its foot
  destinations.
- Groups can be selected with pointer and keyboard.
- Layout matches the reference structure at minimum and default window sizes.

### Phase 5 - tiles, editors, and complete workflows

- Implement adaptive tile grid and context menus.
- Implement group, SSH, agent, and custom CLI editors with inline validation.
- Wire create/edit/move/duplicate/delete and launch flows.
- Add config chooser, Save Copy, reload/conflict UI, and Finder reveal.

Exit criteria:

- A user can create a Production group, add several SSH tiles, relaunch the app, and launch each
  connection in the associated terminal.
- Config location can be moved to a shareable path and reopened.

### Phase 6 - sharing, polish, and resilience

- Implement external change watching and conflict recovery.
- Finish focus behavior, VoiceOver, reduced motion/transparency, high contrast, and visual polish.
- Validate alternate terminal associations and GUI `PATH` behavior.
- Profile large group/tile collections and add virtualization only if measured.

Exit criteria:

- The manual accessibility and shared-config matrices pass.
- No data loss occurs across external edits, invalid JSON, or failed writes.
- Visual review shows a simpler native interpretation of both references, not a generic dashboard.

### Phase 7 - distribution

- Produce universal release builds where dependencies permit.
- Finalize app icon, bundle metadata, versioning, signing, notarization, and update strategy.
- Add release verification and a clean-machine smoke test.

Exit criteria:

- A signed/notarized app launches on a clean supported Mac.
- Build, test, bundle, and verification steps are reproducible.

## Acceptance criteria

The project is not complete until authoritative evidence proves every item below:

- Native Rust + Iced app launches as a GPU-rendered macOS `.app`.
- Despatch runs from the menu bar with no Dock icon, and its panel follows
  `references/facelift.png`.
- The sidebar lists the server groups, and Recently used and Settings at its foot.
- Both sections support persistent user-created groups.
- Selected group shows persistent tiles.
- SSH tiles launch the correct structured SSH command in the system-associated terminal.
- Config survives relaunch and can be placed at a user-selected shared path.
- Shared JSON is versioned, readable, contains no secrets, and handles conflicts without silent
  overwrite.
- Light/dark mode, keyboard use, reduced motion/transparency, high contrast, and VoiceOver are
  verified.
- Format, Clippy, tests, release build, app bundle verification, and manual launch checks pass.
- The final UI has been visually inspected against `references/facelift.png` in light and dark mode.

## Working rules for future agents

1. Read this file, `facelift.md`, and `references/facelift.png` before changing architecture or the
   panel's layout.
2. Keep the current phase and acceptance criteria visible in the active task plan.
3. Do not mark the product complete based on unit tests alone; verify the bundled app and real
   terminal behavior.
4. Use `rg` and `rg --files` for repository searches.
5. Use `cargo fmt`, Clippy, and focused tests during development; run the full gates before handoff.
6. Preserve user changes in a dirty worktree and avoid destructive Git commands.
7. Do not commit unless the user asks.
8. Do not add secrets or real infrastructure addresses to source, fixtures, screenshots, or logs.
9. Do not weaken validation, atomic persistence, shell quoting, or conflict protection to make a
   test easier to pass.
10. Keep UI logic thin and framework-specific code isolated so the domain can be extended with new
    launcher types, config backends, and platform adapters later.
11. If Iced lacks an API, document the gap and isolate the smallest macOS platform bridge. Do not
    change UI frameworks without approval.
12. Update this plan when a product decision changes, but do not silently redefine the goal around
    the implementation that already exists.

## Primary Iced references

- Iced repository: https://github.com/iced-rs/iced
- Iced guide: https://book.iced.rs/
- Iced Rust API: https://docs.rs/iced/latest/iced/
- Iced examples: https://github.com/iced-rs/iced/tree/master/examples

Despatch selects only Iced's wgpu renderer and basic text shaping. Treat main-thread ownership,
message/task lifetimes, renderer selection, accessibility, and any macOS bridge as first-class
engineering work.
