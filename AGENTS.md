# AGENTS.md - Despatch macOS app

This file is the authoritative repository guide for agents and contributors working in this
repository.


## Product goal

Build **Despatch**, a native macOS desktop utility for organizing and launching server and CLI
connections.

The finished app must:

1. Use **Rust** for the application and domain code.
2. Use **Iced** with its `wgpu` renderer for the GPU-rendered macOS UI.
3. Use a simpler version of the visual structure in `references/ui.png` and
   `references/ui_2.png`.
4. Show two distinct sections in the left sidebar: **Servers** and **Agents**.
5. Let users create, rename, reorder, and delete groups in either section, such as `Production`,
   `Staging`, or `Local agents`.
6. Let users split a group into subgroups (categories) such as `Grafana`, `Databases`, or
   `Relays`. Launchers always live in a subgroup, never directly in a group.
7. **Servers** drill down one level at a time: the section shows its groups, the selected group
   reveals its subgroups, and the selected subgroup reveals its launcher tiles. Deselecting a level
   hides everything below it.
8. **Agents** are one flat list. There is no group and no category on screen: the section shows its
   launchers and an "Add agent" card. The containers still exist in the config — a launcher always
   lives in a subgroup — but they are resolved for the user rather than navigated by them, and
   adding an agent creates the container if the config has none.
9. Let users create, edit, move, reorder, duplicate, and delete tiles.
10. Launch an SSH connection in the user's system-associated terminal when a server tile is
   activated.
11. Launch supported agent CLIs in the user's system-associated terminal. Initial presets are
    Codex, Claude, Gemini, and OpenCode.
12. Support custom CLI launchers in addition to the built-in agent presets.
13. Persist all groups and launchers in a human-readable, versioned JSON config file.
14. Let the user select where that config file lives so it can be placed in a shared, synced, or
    version-controlled folder.
15. Keep machine-local preferences, such as the selected config path and window state, outside the
    shared config.
16. Never store passwords, API tokens, SSH private-key contents, or other secrets in the shared
    config.

The working product name and bundle identifier are:

- Product name: `Despatch`
- Bundle identifier: `com.despatch.app`

Changing either is a product decision and should not happen silently.

## Design direction

Read both reference images before changing the main layout.

Design read: a focused developer utility with a calm, native macOS language. It should inherit the
references' sidebar, group list, and connection tiles without inheriting their dense toolbar,
vault, collaboration, port-forwarding, or credential-management features.

Design settings:

- Design variance: 4/10
- Motion intensity: 3/10
- Visual density: 7/10 — compact everywhere the app spends space on itself, generous only in hit
  targets. Every launcher card on screen shares one width: the width the longest label among them
  needs, so no host is ever cut short, bounded only by the panel itself. Nothing else stretches —
  the sidebar is an icon rail one symbol wide, cards and chips are sized to their content
  rather than to a comfortable grid, and editor fields and buttons use the smaller of the two
  native control sizes; rail cells stay large because they are pressed, not read.
- Typography: Lilex, bundled with the binary rather than taken from the system, so the interface is
  identical on every machine. It is monospaced, and the whole layout depends on that: every
  content-sized control is measured through `presenters::text_width`, which multiplies the
  character count by one advance (`ADVANCE`, 0.6 em). Replacing the font means measuring the new
  one and updating that constant, or every card and chip is laid out to the wrong width. All three
  weights ship (Regular, Medium, SemiBold) and all three are registered in `run`; a weight with no
  face behind it is synthesized by the shaper and reads as a smeared Regular. The files share one
  typographic family, so the weight — not the family name — selects the face. Three tests in
  `typography` hold the files, their family name, their weights, and their shared advance together.
- App icon: `apps/despatch/resources/icon.png` is the single source, a 1024pt canvas holding the
  824pt artwork on Apple's macOS icon grid with transparent corners. `bundle-app.sh` derives every
  rendition from it with `sips` and `iconutil`, so no size can go stale on its own, and no `.icns`
  is committed. The icon and the app name come from the surrounding bundle, so a bare
  `cargo run` shows neither; `make run` builds the bundle and launches the binary from inside it.
- Theme: two colors, `#03191E` (ink) and `#F2F5EA` (parchment), whose roles swap with the
  appearance. Dark paints ink and marks in parchment; light paints parchment and marks in ink. The
  system accent is not used — an arbitrary accent on this pair reads as an unrelated third color.
  The appearance follows the system by default and can be pinned to light or dark in Settings; that
  choice is a machine-local preference, never part of the shared config.
- The content column is the palette's ground, flat. It carries no gradient and no lift: it is the
  largest surface in the window, so anything mixed into it is read as the app's color rather than
  as depth, and a wash toward white turns the ground grey next to the true dark of the rail. Dark
  surfaces that do lift are lifted toward parchment, never toward white, for the same reason.
- Accent: one restrained accent color, and never assume it is dark — a symbol sitting on an
  accent-filled control uses the legible-on-accent tint, not white
- Shape system: 10-12 pt surfaces, 7-8 pt fields and buttons, round icon backplates
- Icons: SF Symbols exported as reviewed assets, with descriptive labels and no hand-drawn
  substitutes

Required layout:

The sidebar is an icon rail: one SF Symbol per destination, centered in a cell tall enough to aim
at without looking, and no text at all. Every name — including the config file name and keyboard
equivalents — lives in a tooltip, because a caption is what would force the rail wider. The open
destination is a large accent-filled rounded square.

Two rules the rail must keep:

- **Every cell runs in one sequence and fills its share of the height.** Nothing is pushed to the
  bottom edge with a filling spacer, and nothing is given a fixed height that leaves the remainder
  as dead space under the last cell: a taller window makes the cells taller. The divider is the
  only fixed measurement in the rail. The utilities follow the destinations after it.
- **The symbol is sized by its cell, not by a constant.** It fills the cell short of a few points
  of inset and stays square, so it grows with the rail rather than sitting as a small mark in a
  large target. This is why symbols are rasterized on a canvas sized for the rail: a mask cut for a
  20-point toolbar icon turns to mush when a rail cell stretches, and there is no sharper copy to
  fall back to.
- **The rail is never narrower than the traffic lights.** They float over the content view at a
  position macOS chooses, reaching about 68 points from the window's left edge, and a narrower rail
  leaves the green button hanging over the content while its neighbours sit on the rail. Measure
  before shrinking it.

```text
+------+-----------------------------------------------------------------+
| [::] | Servers                  [search]          + Add server group    |
| [**] |                                                                  |
| ---- | Groups                                                           |
| [::] |  [ Production ]  [ Staging ]  [ Add server group ]               |
| [::] |                                                                  |
|      | Production                                   + Add category      |
|      |  [ Grafana ]  [ Databases ]  [ Add category ]                    |
|      |                                                                  |
|      | Databases                                    + Add server        |
|      |  [ Primary ]  [ Replica ]  [ Add server ]                        |
|      |                                                                  |
+------+-----------------------------------------------------------------+
```

Interaction requirements:

- A tile responds immediately on pointer-down and launches on activation.
- Hover actions live in a `stack` layer above a card, and the pointer tracker must wrap that whole
  stack. `stack` levitates the cursor for the layers under whichever one reports a mouse
  interaction, so a tracker placed *under* the actions is told the pointer left the instant they
  appear, and the card flickers instead of letting the actions be pressed.
- A label color tints a control rather than flooding it: a wash of the color, a border in the same
  color, and the ordinary label color for the text. Only fixed backplates, such as a card badge,
  take the color at full strength. Chips wash with alpha; cards mix the color into their surface so
  they stay opaque above the content gradient.
- Color cascades down one path and is set in two places. A **group** carries a label color. Its
  **categories** have none of their own and are always drawn in the group's, so an open group reads
  as one block. A **launcher** is drawn in its group's color too, but may be given its own in the
  launcher editor, which then wins. Clearing a launcher's color returns it to the group. Do not add
  a color of its own to a category: it would be a setting that could never take effect.
- A status banner that reports an *event* — a launch result, a failed action — clears itself after
  ten seconds. A banner that reports a *state*, such as a conflicted or unsaved config, stays as
  long as the condition holds: hiding it would be lying about the file. Dismissals carry the token
  of the banner they were started for, so one that lands after its banner was replaced is ignored.
- Selection, hover, focus, disabled, empty, loading, success, and error states must all exist.
- Sheets and popovers must originate from the control that opened them and dismiss along the same
  spatial path.
- Motion is limited to useful state feedback and must respect reduced-motion preferences.
- Translucency is reserved for structural chrome such as the sidebar or toolbar. Content tiles use
  solid semantic surfaces for readability.
- Keyboard navigation and VoiceOver labels are required, not optional polish.
- Common shortcuts: Command-N adds an item, Command-Shift-N adds a group, Command-F focuses search,
  Command-comma opens settings, Return launches the selected tile, and Delete requests deletion.

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
          "name": "Databases",
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
- Store only the chosen config path, the appearance choice, and local window/sidebar preferences in
  `NSUserDefaults` via the isolated macOS preferences adapter. `NSUserDefaults` only exposes safe
  setters for numbers and URLs, so anything else would need an `unsafe` block: store enumerations
  as their stable integer code rather than reaching for one.
- Let users create a config, open an existing config, switch config, reveal it in Finder, reload it,
  and save a copy.
- Write to a sibling temporary file, flush and sync it, then atomically rename it over the target.
- Preserve the last valid in-memory state when decoding or validation fails.
- Keep a recoverable backup before a schema migration.
- Detect external changes. Reload automatically when there are no unsaved local edits; otherwise
  show a conflict state with Reload and Save Copy actions.
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
  macOS Automation permission, and it opens a window **in the Ghostty already running** — the argv
  path goes through `open -n`, which starts a second copy of the whole application per launch.
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

Ghostty 1.3's AppleScript surface API reuses the running application and opens a new window. It is
the only implemented reuse path. Do not replace it with `-e` (which restores the per-launch
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
- Provide a future-compatible preference for an explicit terminal override, but default to the
  system association.
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
- Use a native winit-backed window with a sensible default size near 1120 x 720 and a minimum near
  840 x 560.
- Build the sidebar and content region as a resizable two-column Iced layout whose divider and
  width constraints follow macOS conventions.
- Use a native macOS app menu with About, Settings, File, Edit, View, Window, and Help behaviors
  where Iced/winit supports it; isolate any required platform bridge.
- Retain the root component and every Rust callback/model owner for at least as long as the window
  displays them. All UI state transitions stay on the Iced event-loop thread.

### Sidebar

- A destination list, not a tree: one row per section, Servers and Agents. The row itself is the
  hit target and switching rows swaps the whole main panel.
- Settings/config status stays at the bottom and is visually separate from navigation.

### Main content

- Header: section name, its group and launcher counts, search, and one Add action.
- The panel drills down in place: a `Groups` card grid, then the open group's categories, then the
  open category's launcher tiles. A level is drawn only while its parent is selected, and pressing
  an open card closes it again.
- Card grids adapt between one, two, and three columns based on available width.
- Tiles show a system icon, name, concise destination/command detail, and optional tags only when
  tags convey actual data.
- One click on a launcher card selects it, two launch it, and Return launches the selection. A
  single click never launches: launching opens a terminal and takes the focus, which is too much to
  hang on the same gesture that merely picks something out of a list. An overflow/context menu
  exposes Edit, Duplicate, Move, and Delete.
- A launcher card is a `mouse_area` around a container, not a `button`. Three gestures start with
  the same press — select, launch, drag — and a `button` settles that press itself: it captures it,
  then publishes on release, and only if the release lands back inside its bounds. That last rule
  is why clicks used to be missed; a hand that drifted a few points between press and release
  launched nothing at all.
- A launcher moves between categories by holding the mouse down on its card and dragging it onto a
  category chip. Past a small threshold the card lifts and follows the pointer as a floating copy;
  below that threshold the gesture is an ordinary click. While a card is held, every category
  except the one it already lives in offers itself as a target, the one under the pointer is
  filled, and the card's own hover actions stand down. Releasing anywhere else abandons the gesture
  and changes nothing, the pointer leaving the window or the window losing focus abandons it too,
  and a dropped launcher lands at the end of the category it was dropped on. A drop moves one
  launcher: there is no multi-selection to drag.
- The card reports its own press, so the launcher being carried is the one under the pointer by
  construction. The release comes from the event stream through a subscription, which no widget's
  capture can hide, and which is only listened to while a card is actually held. Do not infer the
  pressed card from the last reported hover: a hover goes stale whenever the list redraws under a
  still pointer, and a stale one starts a drag on a card nobody touched.
- **The floating card follows the pointer without messages.** It is a widget of its own,
  `drag_layer`, which keeps the pointer position in its own state, asks for a redraw when it moves,
  and draws the card translated to where the pointer now is. A message is not a cheap thing here:
  every one rebuilds the view and lays the whole window out again, and a pointer reports movement
  many times a second, so a card that followed the pointer through messages fell behind the cursor
  it was supposed to be stuck to. The layer publishes exactly one message per gesture — the moment
  the press travels past its threshold and becomes a drag, which is a real change in what the
  interface shows. Any future "follow the pointer" behaviour belongs in a widget for the same
  reason.
- **The window is always the same two layers**, a base and an overlay, even when the overlay has
  nothing to show. Widget state — every scroll position among it — lives in a tree iced reconciles
  against the view, and a tree whose root changes type is rebuilt from nothing rather than
  reconciled. A window that were a plain container at rest and a `stack` while something floated
  above it would throw away where the user had scrolled the moment they pressed a card.
- Nothing reacts to a press until it becomes a drag. The categories offer themselves as targets, and
  the cards put their hover actions away, only once the card has left its place; doing either on the
  press itself makes an ordinary click flash the whole panel.
- The layer reports no mouse interaction. `stack` hands the cursor to the layers under the topmost
  one that claims one, so claiming it there would take the pointer away from the chips the card is
  being carried to.
- The add tile is visually quieter than real launchers and remains keyboard reachable.
- Large collections use measured virtualization behind an isolated custom widget when Iced's
  standard `scrollable` would retain too many offscreen elements. Do not build every offscreen
  tile in large collections.

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
- Agent editor: pick the provider, then optionally give it parameters. Everything a built-in
  provider already knows — executable, working directory, environment — sits behind the
  "Additional settings" disclosure, so adding an agent is choosing one and pressing Add.
- The four built-in providers are present in an `Agents` group, because an empty Agents section
  offers nothing but "create a group", which is not what someone opening the app came to do. A new
  config is seeded with them; an existing config with no Agents section gains them **in memory when
  it is opened**. They are ordinary launchers and can be edited or deleted like any other.
- The backfill writes nothing. Opening a file is not an edit: rewriting someone's config the moment
  they open it would bump its revision and race another machine editing the same shared file, which
  is a poor trade for launchers that work either way. The next real edit persists them. Seeding
  only at creation is not enough — it never reaches a config that already exists, which is every
  config but the first.
- Each provider is drawn with its own brand mark, bundled under `assets/logos/` in the
  single-color form published by [Simple Icons](https://simpleicons.org) (CC0). The marks are
  monochrome by design, which is what lets them go through the same coverage-mask tinting as a
  system symbol. Each trademark belongs to its owner and is used only to name the tool a launcher
  runs.
- A bundled mark may arrive as ink on transparency or as ink flattened onto white, depending on
  what rasterized it. Coverage is therefore alpha scaled by darkness, not alpha alone: a glyph
  flattened onto white is fully opaque everywhere, and reading alpha by itself renders it as a
  solid block.
- The environment block is `NAME=value`, one per line, blank lines and `#` comments ignored, and
  only the first `=` separates a name from its value so `FLAGS=--a=b` is one entry. Names are
  validated as shell identifiers, because a name carrying `=` or a space does not become a
  variable — it becomes another command. This is where a shell alias's variables go; see below.
- Saving an agent or a custom launcher first resolves its executable the same way launching would.
  A name that resolves to nothing is refused with the name in hand, because the alternative is
  finding out when the tile is pressed. **A shell alias is not a program**: it lives in the user's
  shell configuration and cannot be executed directly, so it fails this check. That is correct —
  such a launcher would never have run — and the way to reproduce one is an absolute path plus the
  environment the alias sets.
- Custom CLI editor: name, working directory, structured executable + argv by default, with an
  explicitly enabled advanced raw-shell mode.
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
- Window at minimum, default, and full-screen sizes
- One-, two-, and three-column tile layouts
- Keyboard-only navigation and every declared shortcut
- VoiceOver labels, roles, focus order, and editor error announcements
- Add/edit/delete flows for server, each built-in agent, and custom CLI
- One click selects a card without launching, two launch it, and a double click repeated on
  several cards in a row launches every time
- Dragging a launcher onto another category, onto its own category, and onto nothing at all
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

The scenes are `add-server`, `add-server-advanced`, `add-agent`, `agents`, `groups`, `rename-group`,
`rename-category`, `delete`, `drag`, `hover`, and `settings`. The `drag` scene shows what a held card
does to the categories — the drop targets offering themselves, the one under the pointer filled — but
not the floating card itself, which is drawn from real pointer events that a scene cannot produce.
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

- Sidebar visibly contains Servers and Agents sections.
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
- A user can create Codex, Claude, Gemini, and OpenCode tiles and launch each.
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
- Left sidebar has separate Servers and Agents sections.
- Both sections support persistent user-created groups.
- Selected group shows persistent tiles.
- SSH tiles launch the correct structured SSH command in the system-associated terminal.
- Codex, Claude, Gemini, and OpenCode tiles launch their respective CLIs.
- Custom structured CLI launchers work; raw-shell launchers are explicit and warned.
- Config survives relaunch and can be placed at a user-selected shared path.
- Shared JSON is versioned, readable, contains no secrets, and handles conflicts without silent
  overwrite.
- Light/dark mode, keyboard use, reduced motion/transparency, high contrast, and VoiceOver are
  verified.
- Format, Clippy, tests, release build, app bundle verification, and manual launch checks pass.
- The final UI has been visually inspected against both reference images at multiple window sizes.

## Working rules for future agents

1. Read this file and both reference images before changing architecture or the main UI.
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
