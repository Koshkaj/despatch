<div align="center">
  <img src="apps/despatch/resources/icon.png" alt="" width="128">
  <h1>Despatch</h1>
  <p><strong>A native macOS menu-bar launcher for the servers you SSH into.</strong></p>
</div>

Despatch lives in the menu bar. Click its item and a glass panel drops down with your SSH
connections, kept in one organised, human-readable JSON file; double-click one and it opens in your
terminal. It stores no secrets, and it never owns the session it starts — it hands the command to
your terminal and gets out of the way.

- **Groups** fill the sidebar, and each group's categories are pills above its list of servers.
  **Recently used** and **Settings** sit at the foot of the sidebar.
- **Pick your terminal** in Settings — the system default, or any installed Terminal, iTerm2,
  Ghostty, WezTerm, kitty, Alacritty, or Warp. With Ghostty already open, a server opens as a new
  tab in its front window.
- The config file lives wherever you point it — a synced folder or a git repo, so the same set of
  connections follows you between machines. Edit it by hand, pull it, or let it sync: the open
  panel picks the change up by itself.
- Search reaches every group at once.
- Machine-local choices (which config file, appearance, terminal, launch history) stay out of that
  shared file.

The panel follows light and dark mode and your accent color, and sits on the system's glass —
Liquid Glass from macOS 26. Reduce Transparency, Increase Contrast, and Reduce Motion are all
respected.

## Install

Download the disk image from [Releases](../../releases), open it, and drag **Despatch** to
Applications. Run it from Applications, not from the disk image.

Despatch is **not signed with an Apple Developer ID**, so macOS quarantines the download and
refuses to open it. Clear the flag once:

```sh
xattr -dr com.apple.quarantine /Applications/Despatch.app
```

Right-click → Open no longer works as a bypass on macOS 15 and later; the alternative is
System Settings → Privacy & Security → **Open Anyway**.

Requires macOS 13 or later. The release build is universal (Apple silicon and Intel).

## Build from source

```sh
make run     # build the app bundle and run it, with output attached to this terminal
make bundle  # build Despatch.app only
make dmg     # build the universal release disk image
make check   # fmt, clippy, and the full test suite
```

Despatch has no Dock icon and no window of its own: it appears as a terminal symbol in the menu bar
and opens its panel once at launch. Right-click the item for Settings and Quit, or press ⌘Q while
the panel is open. A bare `cargo run` works too; the app name and icon come from the surrounding
bundle, which is why `make run` builds one first.

## How launching works

Settings → **Launches open in** chooses the terminal. Left on **System default**, Despatch follows
macOS's own answer: there is no "default terminal" setting as such, but there is a Launch Services
handler for `public.unix-executable`, which is what every terminal's "set as default terminal"
button writes. With nothing set there, the command goes to Terminal.app.

Ghostty is a special case: it is driven through its scripting interface rather than a command-line
flag, because its `-e` option asks you to confirm every executable. A running Ghostty gets a new tab
in its front window, as if you had pressed ⌘T; one that is not running, or has no window open, gets
a new window. That path needs Ghostty 1.3 or later with `macos-applescript` enabled, and the first
launch asks macOS for permission to automate it.

## Configuration

The config file defaults to `~/Library/Application Support/Despatch/config.json` and can be moved
anywhere from Settings. It is versioned, human-readable, and safe to keep under version control.

Changes made to the file outside Despatch are picked up automatically: the panel looks when it
opens and every two seconds while it stays open. If you have an unsaved change in the app at that
moment, it shows a conflict with **Reload** and **Save Copy** instead of overwriting either side.
Agent and custom-command entries written by earlier versions are kept in the file untouched, but
Despatch no longer shows them.

**Despatch never writes secrets to it** — no passwords, no API tokens, no private-key contents.
Identity files are referenced by path, not by content.

## Development

`AGENTS.md` is the authoritative guide to the codebase: the layer boundaries, the design system,
and the reasoning behind the launch path. Read it before changing the main layout or the way a
command reaches a terminal.

The workspace is a ports-and-adapters layout:

| Crate | Role |
| --- | --- |
| `despatch-domain` | Groups, launchers, validation. No I/O. |
| `despatch-application` | Use cases and the port traits every adapter implements. |
| `despatch-config` | Versioned JSON persistence with conflict detection. |
| `despatch-platform-macos` | Launch Services, terminals, `NSUserDefaults`, SF Symbols. |
| `despatch-probe` | TCP reachability checks. Never authenticates. |
| `despatch-ui-iced` | The Iced/wgpu interface. |

The UI reference images `AGENTS.md` points at are not in the repository; the design rules it states
in prose are the part that is binding.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you state otherwise, any contribution you intentionally submit for inclusion
in this work shall be dual-licensed as above, with no additional terms or conditions.

The interface is set in San Francisco, the macOS system font, and draws Apple's SF Symbols; both are
read from the system at run time, and neither is bundled.
