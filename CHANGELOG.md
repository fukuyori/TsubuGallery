# Changelog

English · [日本語](CHANGELOG.ja.md)

A human-readable record of what changed in each release. What each feature
actually does is in [README.md](README.md); the fine detail is in `git log`.

There is a single version number, in `Cargo.toml` under `[workspace.package]`,
shared by all five crates. Dates are when the version was cut.

## 0.4.2 — 2026-08-22

### Fixed

- **FragCoord / ShaderToy-style GLSL such as sketch 59 did not run.**
  `mainImage`, `iTime`, and `iResolution` now map to the twigl runtime, and a
  compatibility conversion handles the single-vec4 `mat2` constructor that
  naga could not validate

## 0.4.1 — 2026-08-22

### Added

- `-Sign` for the Windows installer script. The certificate selected by
  `CODESIGN_CERT` signs the application executable, setup, and uninstaller

### Fixed

- **Tweet-sized GLSL with nested uninitialized loop variables rendered incorrectly.**
  naga lifts block variables to the start of the WGSL function, so an inner
  `for(int i; ...)` kept its value when the outer loop repeated. TsubuGallery now
  supplies the zero initializers that twigl-style golfed shaders rely on at each
  loop entry

## 0.4.0 — 2026-08-19

### Added

- **つぶやきGLSL can be formatted too.** `⌘F` / `⌘K` used to do nothing at all
  on a shader. Statement separators and brackets work alike in every C-family
  language, so the dialect never had to be told apart; only a line starting with
  `#` — a `#version` directive, or a `#つぶやきGLSL` tag — is passed through whole
- Strings spread in p5.js: `[...'#つぶやきProcessing']` is an array of 15 elements
- `p5.Vector` statics: `random2D` `random3D` `fromAngle` `add` `sub` `mult` `div`
  `lerp` `cross` `normalize` `dot` `dist` `mag` `angleBetween`. Unlike the
  instance methods, these **leave their arguments alone** and return a new vector
- A changelog (`CHANGELOG.md` / `CHANGELOG.ja.md`)
- `app/assets/icon-1024.png` for macOS. The macOS installer added in 0.3.6 looked
  for it, but `scripts/make-icon.py` never drew it

### Fixed

- **No icon in the Windows taskbar.** Two causes. winit's `with_window_icon()`
  only sets `ICON_SMALL` (the title bar), and the `ICON_BIG` the taskbar reads is
  actively cleared when not supplied. And setting it is not enough on its own:
  Explorer asks a newly appeared window for its icon with a short timeout, gives
  up while startup is blocking the message loop, and never asks again — so the
  icon is set once more after the first frame
- **Passing a builtin as a callback was broken**, as in `[1,4,9].map(sqrt)`.
  A builtin pushes no frame, so treating it like a sketch's own function ran the
  caller's remaining code as part of the callback. Three places: `map`, `reduce`
  and `sort`
- Spacing in the formatter: `2.-r` → `2. - r`, `9./dot(p,p)` → `9. / dot(p, p)`,
  `i++<9` → `i++ < 9`. It decided from the last character written, which cannot
  tell a decimal point from a property separator
- Compressing **expanded** `p.x` into `p . x`, by treating the dot as part of a
  number

## 0.3.6 — 2026-08-18

### Added

- **A macOS installer (.pkg).** `scripts/build-macos-installer.sh` runs a
  release build, assembles the .app, signs it, builds the .pkg, submits it for
  notarisation and staples the ticket. It builds universal (Apple Silicon and
  Intel) by default, and writes to `target/installer/` like the Windows one

### Fixed

- **Editing or deleting a different sketch in the Gallery threw away the canvas
  of the one on screen.** One `Graphics` is shared by every sketch, so resetting
  it loses the size `createCanvas()` chose, and since `setup()` never runs again
  the drawing ends up in the top-left corner. It is reset only when the sketch
  on screen is the one removed, and then the state of the sketch that moves up
  is restored

## 0.3.5 — 2026-08-17

### Added

- `switch` / `case` / `default` in p5.js. Falling through to the next case when
  there is no `break` works as it does in Java Mode
- `windowWidth` / `windowHeight`. `innerWidth`, `innerHeight`, `displayWidth`
  and `displayHeight` return the same values. They mean the display area
  itself, so they do not change after `createCanvas()`
- `QUADS` and `QUAD_STRIP` for `beginShape()`. Every four points become their
  own **closed** face. Joining them into one polygon leaves no stroke where two
  faces meet, and the outline of a ribbon or a mesh disappears
- Colours written as text: the 147 CSS colour names and `#rgb` `#rgba`
  `#rrggbb` `#rrggbbaa`. As in p5.js they ignore `colorMode()`
- JavaScript reserved words as property names (`{default:1}.default`)

### Fixed

- **Opening the Viewer from the Gallery for the first time ran a neighbouring
  sketch's `setup()` at a size of 1×1.** Preloading used "the size of the last
  frame drawn", which is still the placeholder while the Viewer has never been
  drawn. A sketch written as `createCanvas(innerWidth, innerHeight)` baked that
  1 into a global, and since `setup()` never runs again it stayed black for the
  rest of the session
- Calling a function that does not exist now names it: `createColorPicker() is
  not a function` instead of `void is not callable`
- `continue` inside a `switch` now belongs to the enclosing loop. Outside a loop
  it is still refused

## 0.3.4 — 2026-08-16

### Added

- GPU name and backend in the `I` overlay (`Vulkan · DiscreteGpu`), so it is
  clear which adapter was picked on a machine with two of them
- The mouse cursor disappears after three idle seconds in fullscreen. **It is
  never hidden while the cursor is not over this window** — with two monitors
  the cursor can leave for the other screen
- A screenshot in the README, and the terminology unified on
  つぶやきProcessing / つぶやきGLSL
- The OS in the installer's filename (`TsubuGallery-0.3.4-windows-x64.exe`)
- `docs/TsubuGallery_Expansion.md` — a study of what supporting three.js,
  Canvas 2D, WGSL and CUDA would take

### Fixed

- Debug builds panicked at startup. egui's `TexturesDelta` was dropped without
  being emptied, which trips its `debug_assert!`

## 0.3.3 — 2026-08-16

### Added

- An application icon. `scripts/make-icon.py` draws it and `app/build.rs`
  embeds it into the exe along with the version resource. It also becomes the
  winit window icon
- **A Windows installer.** `scripts/build-installer.ps1` runs a release build,
  signs it if given a certificate, and drives Inno Setup 6. It installs
  per-user without administrator rights, and ships `tsubugallery.exe` alone

## 0.3.2 — 2026-08-16

### Added

- When the window cannot be created or the GPU cannot be set up, the reason is
  shown in an **OS dialog** before exiting. Launched from a shortcut, the
  console closes with the process and standard error cannot be read

## 0.3.1 — 2026-08-16

### Added

- Playback speed from 0.25× to 4×, on `↑` / `↓` and in the settings. It is not
  the frame rate: the frame rate decides how often the screen is drawn, the
  speed decides how many real seconds a sketch second takes
- A per-sketch clock. This is GLSL's `t`. It stops on `Space` and does not
  advance while another sketch is on screen
- Thumbnails are always captured at 1× regardless of the speed, so the same
  sketch always yields the same picture

## 0.3.0 — 2026-08-16

### Added

- **つぶやきGLSL.** A single fragment shader, twigl's geekest dialect, can be
  dropped in as is. naga translates it to WGSL, wgpu builds the pipeline, and
  it is painted on one screen-filling triangle. `r` `t` `f` `m` `FC` `o` and
  `rotate2D` `hsv` `snoise2D` and friends come from a preamble, so they need no
  declaration
- Automatic detection between the three dialects (Processing, p5.js,
  つぶやきGLSL). Nothing has to be declared

## 0.2.4 — 2026-08-14

### Added

- **A log** at `<data>/logs/tsubu.log`. One event per line, always starting
  with `time level kind`, and sketch lines continue with `key=value` pairs — a
  shape an editor or an external agent can read to find what needs fixing, not
  just a human. It rotates at 1 MiB, keeping three generations. `RUST_LOG` sets
  the level
- Real lighting: `ambientLight()`, `directionalLight()`, `pointLight()`, up to
  five per frame. **The colour lands on the face as it is**, so a white solid
  under a yellow light comes out yellow. Only the camera transform is applied
  to a light's position and direction, matching p5.js
- `rect(x, y, w)` is a square, as in p5. `smooth()` / `noSmooth()` are accepted
  and do nothing
- `--version` prints the version and where the log lives

### Fixed

- A cap on how much geometry one frame may hold; over it the sketch is stopped
  and told why. The point is to **never ask the GPU for an allocation over the
  limit** — the device rejects it in validation and wgpu's default handling
  takes the process down with it
- Local variables in p5.js. Parameters, and `let` / `const` / `var` used only
  inside one function, are now locals, so a recursive function no longer
  clobbers the caller's loop variable

## 0.2.3 — 2026-08-14

### Fixed

- Sketches that lean on the canvas — static-mode ones, and the
  `f++ || background(0)` idiom that paints the ground on the first frame only —
  are restarted whenever the canvas is thrown away. Otherwise they go on
  drawing white lines on white
- A single `int` is read as a packed colour (`0xAARRGGBB`) in Processing only.
  p5.js has no such rule, and applying it makes `stroke(500)` alpha 0, drawing
  nothing
- Edge thickness on solids is measured in canvas units, not screen pixels, so a
  mesh no longer thins out as the window grows

### Added

- `sphereDetail()`. A sphere is divided 24 × 16 regardless of its radius (p5's
  default), and an edge shared by two faces is drawn once

## 0.2.2 — 2026-08-14

### Added

- A seventh argument to `arc()` for how it closes (`OPEN` / `CHORD` / `PIE`)
- `blendMode()` with `BLEND ADD MULTIPLY SCREEN DIFFERENCE EXCLUSION DARKEST
  LIGHTEST SUBTRACT REPLACE`. `DIFFERENCE` is approximated by exclusion, since
  the GPU's blender cannot choose the sign of a subtraction
- `drawingContext` shadows

### Fixed

- `beginShape()` fills by ear clipping. Fanning the vertices spilled outside
  the notches of a concave shape

## 0.2.1 — 2026-08-14

### Fixed

- **Drawing state is now kept per sketch.** One `Graphics` is shared by the
  whole gallery while `setup()` runs exactly once, so whatever it decided —
  `stroke(-1)`, `size()`, `colorMode(HSB)`, whether the sketch is 3D — was lost
  for good the moment the state was reset for another sketch. Preloading made
  it worse, because `setup()` ran against a *different* `Graphics`

### Added

- How the canvas is fitted to the window (contain / cover). It applies to
  thumbnails too, so a card and the Viewer agree
- CPU load, sketch execution time, and instructions and triangles per frame in
  the `I` overlay
- Arrays and their methods (`map`, `filter`, `reduce`, …) and spread (`...`) in
  p5.js
- `pushStyle()` / `popStyle()`. p5's `push()` / `pop()` save the transform and
  the style alike
- Symbol font handling — which symbol font is used changes the picture

## 0.2.0 — 2026-08-14

### Added

- Author and link. `O`, or the ↗ button in list view, opens the default
  browser. **Only `http://` and `https://` are opened** — a link arrives with
  the sketch, and opening it hands that value to an external program
- A search box matching title, id and author
- **Static mode.** A sketch with neither `setup()` nor `draw()`, just
  statements outside any function, runs. Short posted sketches are usually
  written this way
- **3D.** `size(w, h, P3D)` / `createCanvas(w, h, WEBGL)`, `box()`, `sphere()`,
  `rotateX() rotateY() rotateZ()`, `lights()`
- Java Mode arrays (`new float[r][c]`, `{1,2,3}`, the enhanced `for`), compound
  assignment, several declarations in one statement
- Colour components (`red()`, `hue()`, …), `clear()`, `resetMatrix()`
- The `I` overlay

## 0.1.0 — 2026-08-13

The first release.

- Gallery (grid and list, search, filters, sorting), Viewer (fullscreen
  playback, slideshow, screensaver), Editor (highlighting, error reporting),
  and settings
- The Processing Lite runtime: lexer → parser → AST → bytecode → VM
- The wgpu renderer, shared by the Viewer and thumbnail generation
- A SQLite library, a single-instance lock, and English / Japanese switching
