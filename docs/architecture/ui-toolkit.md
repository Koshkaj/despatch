# UI toolkit decision

Despatch uses Iced 0.14.x for its macOS interface. The app enables a deliberately narrow feature
set:

- Iced/winit supplies the native macOS window and event loop;
- Iced's wgpu renderer supplies GPU rendering and maps to Metal on macOS;
- basic text shaping renders the macOS system font without bundling a replacement font;
- Iced's built-in thread pool supplies its required futures executor without adding Tokio or Smol;
- Iced's codec-free image path uploads small decoded SF Symbol textures without adding a raster
  image decoder suite.

Default Iced features are disabled. The production app does not enable Tiny-Skia, WebGL,
unconditional rendering, Linux X11/Wayland support, debug tooling, or the bundled Fira Sans font.
This keeps the release runtime focused on one renderer and avoids a CPU-rendered fallback path.

The choice of Iced is an explicit product decision. Iced is MIT-licensed, Rust-native, event-driven,
and gives Despatch complete control over the visual system while keeping the domain and
application layers framework-free.

Primary sources:

- <https://github.com/iced-rs/iced>
- <https://book.iced.rs/>
- <https://docs.rs/iced/latest/iced/>
- <https://github.com/iced-rs/iced/tree/master/examples>

## Apple-style design contract

Iced does not provide AppKit controls or a built-in macOS theme. Despatch therefore owns a small
semantic theme rather than using Iced's stock visual presets. It must use the macOS system font,
semantic light/dark colors with the user's system accent, size-specific type hierarchy,
native-feeling spacing, 7–8 pt control radii, a 12 pt panel radius, and clear hover, pressed,
focus, disabled, success, and error states. The renderer selects the installed system font by its
font-database family name, `"System Font"` (`SFNS.ttf`), and its medium and semibold roles are
positions on that variable font's weight axis.

Press feedback begins on pointer-down. Motion is brief, interruptible, and functional; there is no
perpetual animation or unconditional redraw loop. Sheets and popovers enter and leave along the
same spatial path and remain anchored to their source control. Reduce Motion replaces spatial
motion with a short cross-fade or an immediate state change. Reduce Transparency makes structural
chrome solid, and Increase Contrast adds explicit boundaries without relying on color alone.

Structural chrome may use a narrowly isolated macOS material bridge if Iced cannot provide the
required backdrop behavior. Content tiles remain solid for readability. Any bridge lives in
`despatch-platform-macos`, never in view code.

## Menu-bar panel

Despatch runs as an Iced **daemon** behind an `NSStatusItem`. Iced and winit provide neither a
status item nor a way to show and hide a window that keeps its state, so both come from a narrow
bridge in `despatch-platform-macos`:

- `status_item.rs` creates the item (an SF Symbol template image), turns its clicks, its native
  right-click menu, and app deactivation into plain events, and reports where the item sits;
- `panel_window.rs` applies menu-bar-panel behaviour to the `NSWindow` behind Iced's view —
  every Space, over full-screen apps, the system shadow — and shows, hides, and places it.

The UI crate's `status_item.rs` turns those events into an Iced subscription and those calls into
tasks, so the rest of the interface deals only in messages.

The panel window is borderless and transparent; Iced paints no window background, and the panel is
a 12 pt rounded surface drawn by the view, so its corners and the shadow computed from them are
real. It is drawn opaque rather than over a material: the surfaces are solid semantic colors, and
the shadow comes from the window server.

Symbols use AppKit SF Symbols rendered once on the main thread by the isolated
`despatch-platform-macos` adapter onto a 64-point canvas, copied into RGBA coverage masks, and
cached as Iced image handles per tint. Provider marks are bundled. No web icon set, custom icon
renderer, or bundled image codec is involved.

## Performance contract

Phase 0 records the idle resident-set size of an optimized Apple Silicon build after the first
window has rendered. The measurement command, OS version, architecture, renderer evidence, and
result live in `docs/performance/baseline.md`. Later UI work compares against that baseline and
explains any material increase. Frame-time review uses release builds; debug-build behavior is not
accepted as performance evidence.

Iced remains event-driven. The `unconditional-rendering` feature is forbidden, and subscriptions
must not create an idle redraw loop. Large lists and tile grids are profiled before introducing a
custom virtualized widget.

## Boundary rules

Iced elements, messages, tasks, subscriptions, themes, and renderer types stay inside
`despatch-ui-iced`. Application state is converted to plain Rust view models, and UI messages are
translated into application commands. Iced never writes config files or starts processes directly.

macOS-only services—including user defaults, Finder reveal, terminal launch, native menus,
appearance/accessibility preferences, and any narrowly required material bridge—live in
`despatch-platform-macos`. Domain and application crates do not depend on Iced, winit, wgpu,
Objective-C, or AppKit types.

## Accessibility bridge

Iced 0.14 does not expose a built-in accessibility tree or AccessKit integration. Despatch keeps
a framework-free accessibility presentation model beside its visual presenter. On each relevant
state change, the UI passes ordered labels, roles, and selection state plus Iced's raw `NSView`
handle to a narrowly isolated adapter in `despatch-platform-macos`. That adapter publishes native
`NSAccessibilityElement` children for VoiceOver; all Objective-C ownership and selector assumptions
remain in one reviewed module.

The bridge covers structure, labels, roles, navigation order, selected state, **frames**, and
**press actions**. Each element is given the rectangle it is drawn in (`views::layout`, exact
because the panel is a fixed size; a row scrolled out of view gets none), so `VoiceOver` outlines and
steps through what is on screen. Every control is a `DespatchPressableAccessibilityElement`, whose
`accessibilityPerformPress` — what VO-Space and Switch Control send — reports its index back to the
shell, which carries out the action of the element in the tree that was installed, so a press
always means what was announced. The live self-test checks the frames on screen and presses
elements through `accessibilityPerformPress`.

Hearing `VoiceOver` read the panel is the one part no automated check covers; the manual VoiceOver
matrix in `AGENTS.md` remains a release step for a person.
