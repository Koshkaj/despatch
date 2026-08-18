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
semantic light/dark colors, size-specific type hierarchy, native-feeling spacing, 7–8 pt control
radii, 10–12 pt surface radii, and clear hover, pressed, focus, disabled, success, and error states.
The renderer explicitly selects the installed `.SF NS` family—the backing family for AppKit's
system font—with regular, medium, and semibold roles plus size-specific leading.

Press feedback begins on pointer-down. Motion is brief, interruptible, and functional; there is no
perpetual animation or unconditional redraw loop. Sheets and popovers enter and leave along the
same spatial path and remain anchored to their source control. Reduce Motion replaces spatial
motion with a short cross-fade or an immediate state change. Reduce Transparency makes structural
chrome solid, and Increase Contrast adds explicit boundaries without relying on color alone.

Structural chrome may use a narrowly isolated macOS material bridge if Iced cannot provide the
required backdrop behavior. Content tiles remain solid for readability. Any bridge lives in
`despatch-platform-macos`, never in view code.

The app window uses Iced/winit's transparent macOS surface and system backdrop blur. The sidebar
and content header use restrained translucent surfaces with explicit borders; launcher tiles stay
opaque. macOS therefore owns the blur behavior and automatically supplies the appropriate system
material response instead of the UI simulating glass with a decorative gradient.

Launcher tile icons use AppKit SF Symbols rendered once on the main thread by the isolated
`despatch-platform-macos` adapter, copied into tiny RGBA buffers, and cached as Iced image
handles. SSH launchers use `server.rack`, agents use `terminal.fill`, and custom commands use
`command.square.fill`. Text fallbacks remain available if a symbol is unavailable on the running
macOS version. No web icon set, custom icon renderer, or bundled image codec is involved.

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

The bridge currently covers structure, labels, roles, navigation order, and selected-state
announcements. Native accessibility activation actions and the manual VoiceOver matrix remain
release blockers. Until those pass, the UI is not accessibility-complete and the product cannot
satisfy its final acceptance criteria.
