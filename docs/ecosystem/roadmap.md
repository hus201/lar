# Package-set roadmap (5-year app surface)

**Status:** Definition — catalog and build phases; packages not yet published as a public source

This is the roadmap for the first curated LAR package source (“main” ecosystem): a closed set of shared libraries, runtimes, and flagship apps sufficient for **typical Linux applications from roughly 2021–2026**.

It is not every library on Earth, and not browsers or full desktop environments.

Related: [architecture.md](../design/architecture.md) (package sources), [platform.md](../design/platform.md) (host vs LAR), [repos.md](../implementation/repos.md) (publish/consume).

---

## Goal

Grow a shared dependency ecosystem that many real apps can resolve against — libraries and installable apps with closed graphs — while the OS keeps kernel, drivers, desktop session, and hardware integration.

## Compatibility target (~2021–2026)

| Surface | Expectation |
|---------|-------------|
| Python | 3.11–3.13 class apps/tools |
| Node | 20 LTS / 22 class CLIs (no full npm registry dump) |
| Qt | Qt 5.15 LTS **and** Qt 6.x Widgets base |
| GTK | GTK 3 **and** GTK 4 + GLib/GObject stack |
| Media | FFmpeg 5.x–7.x class `libav*` + common image/audio codecs |
| Network/TLS | OpenSSL 3.x + curl |
| Desktop | Wayland-first; X11 still declared via `[platform]` for older apps |

## Host / LAR boundary

**Host (not packaged in LAR):** glibc, dynamic linker, kernel, desktop session, GPU/ICD (Mesa), CA store.

Apps and libs declare platform needs with `[platform]` (`wayland`, `x11`, `vulkan`, `opengl`, `dbus`, `dri`, `systemd-user`) — presence heuristics at install/launch. See [platform.md](../design/platform.md).

**LAR owns:** application packages, shared libraries, language runtimes, and composed runtimes in the SxS store.

## Version policy

- Ship **current + previous major** where the break is common in the target window (Qt 5.15 + Qt 6; GTK 3 + GTK 4).
- Language runtimes ship **one current + one previous supported line** (same package id; distinct semver pins in the store).
- Resolve prefers the **highest compatible** version; one version per id in a lockfile.
- Source order is priority; **never merge** package contents across sources.
- Exact patch versions are chosen when each family is built; this roadmap locks **ids and major lines** only.
- Build from upstream / staged trees — **not** `deb2lar`. Closed dependency graphs per family.

```text
Host OS
   │
   ├── Network / TLS ──► Language runtimes
   ├── Compression ────► Language runtimes, Media
   ├── Text / fonts ──► Qt, GTK
   ├── Media ──────────► Flagship apps
   ├── Qt 5 + 6 ───────► Flagship apps
   └── GTK 3 + 4 ──────► Flagship apps
```

---

## Testing apps

Fixed set of packages used to **prove** each phase. Runtimes/CLIs with `[entry]` count as apps; GUI flagships exercise install → `.desktop` → launch → `[platform]`.

| LAR id | Phase | Stack | What it proves |
|--------|-------|-------|----------------|
| `org.curl.curl` | A | CLI | Closed TLS/HTTP graph; entry binary in runtime |
| `org.python.python` | A | CPython 3.11 + 3.13 | Shared lib + stdlib (ssl/sqlite/compress/ctypes) |
| `org.nodejs.nodejs` | A | Node 20 + 22 | Runtime entry; dual line coexist |
| `org.git.git` | A | CLI | HTTPS via OpenSSL/curl |
| `org.ffmpeg.ffmpeg` | B | CLI + `libav*` | Curated codecs; convert wav/png in-runtime |
| `org.example.editor` | D | **Qt 6** Widgets | Design-sample desktop app; `[entry]` + `[desktop]`; may depend on FFmpeg |
| `org.lar.test.qt5-widgets` | D | **Qt 5.15** Widgets | Dual-major smoke (minimal window; not a product app) |
| `org.example.gtk-viewer` | E | **GTK 4** | Desktop flagship; `[entry]` + `[desktop]` |
| `org.lar.test.gtk3-smoke` | E | **GTK 3** | Dual-major smoke (minimal window) |

**Rules:**

- Prefer these ids over inventing new flagships per wave.
- `org.example.*` = small real-ish apps meant to stay in the source as demos.
- `org.lar.test.*` = throwaway-thin smokes whose only job is dual-major / ABI proof.
- Phase F installs **`org.example.editor`** and **`org.example.gtk-viewer`** from the published source (smokes may stay local/CI-only).

---

## Phase A — Language + CLI substrate

Unlocks network/TLS, interpreters, CLIs, embedders, and developer HTTPS workflows.

| LAR id | Lines | Role |
|--------|-------|------|
| `org.openssl.openssl` | current 3.x | TLS |
| `org.zlib.zlib` | current | compression |
| `org.brotli.brotli` | current | compression |
| `org.facebook.zstd` | current | compression |
| `org.nghttp2.nghttp2` | current | HTTP/2 |
| `org.gnu.libidn2` | current | IDNA |
| `org.gnu.libunistring` | current | Unicode strings |
| `org.rockdaboot.libpsl` | current | public suffix list |
| `org.curl.curl` | current | HTTP client / libcurl |
| `org.python.python` | **3.11** and **3.13** | shared `libpython` + `python3` entry |
| `org.nodejs.nodejs` | **20** LTS and **22** | `node` / `npm` entry; no global package dump |
| `org.sqlite.sqlite` | current | Python/Node native deps |
| `org.ncurses.ncurses` | current | TTY UX |
| `org.gnu.readline` | current | if not folded into the ncurses build |
| `org.bzip.bzip2` | current | stdlib bz2 |
| `org.tukaani.xz` | current | stdlib lzma |
| `org.libffi.libffi` | current | ctypes / FFI |
| `org.git.git` | current | HTTPS via OpenSSL/curl |

**Policy:** Build network/TLS first (closed curl graph), then runtimes. Python `--enable-shared` against packaged OpenSSL; defer tkinter/GUI until Phases D–E. Node ships the runtime only.

**Testing apps:** `org.curl.curl`, `org.python.python`, `org.nodejs.nodejs`, `org.git.git`

**Done when:**

- `curl` runs in a LAR runtime against the packaged TLS stack
- `python3 -c "import ssl,sqlite3,zlib,lzma,bz2,_ctypes"` works in a LAR runtime (for each shipped Python line)
- `node -v` works for each shipped Node line
- `git clone https://…` works in a LAR runtime using packaged TLS

---

## Phase B — Media surface

Unlocks players, editors, converters, and media-adjacent apps.

| LAR id | Role |
|--------|------|
| `org.ffmpeg.ffmpeg` | tools + `libav*` (curated codec set) |
| `org.libpng.libpng` | PNG |
| `org.libjpeg.turbo` | JPEG |
| `org.webmproject.libwebp` | WebP |
| `org.xiph.ogg` | Ogg container |
| `org.xiph.vorbis` | Vorbis |
| `org.xiph.opus` | Opus |
| `org.videolan.x264` | H.264 (**GPL**; document license in package metadata / advisories as needed) |

**Depends on:** Phase A compression/TLS packages where applicable.

**Policy:** curated codecs only — not “everything Debian enables.”

**Testing app:** `org.ffmpeg.ffmpeg`

**Done when:** `ffmpeg -version` and convert a sample wav/png inside a LAR runtime.

---

## Phase C — Text / GUI shared deps

Shared primitives for Qt and GTK. Host GL/Vulkan via `[platform]`; **do not** ship Mesa in this roadmap.

| LAR id | Role |
|--------|------|
| `org.freedesktop.freetype` | fonts |
| `org.freedesktop.fontconfig` | font config |
| `org.harfbuzz.harfbuzz` | text shaping |
| `org.icu.icu` | i18n |
| `org.xmlsoft.libxml2` | XML |
| `org.xmlsoft.libxslt` | XSLT |
| `org.cairographics.cairo` | 2D graphics |
| `org.pango.pango` | text layout |
| `org.gnome.gdk-pixbuf` | image loading |
| `org.gnome.glib` | GLib |
| `org.gnome.gobject-introspection` | GObject introspection |

**Done when:** packages install into the store with closed graphs and are usable as deps of Phase D/E testing apps (no separate testing app in this phase).

---

## Phase D — Qt dual-major

Unlocks Qt Widgets desktop apps across the 5-year window.

| LAR id | Lines | Role |
|--------|-------|------|
| `org.qt.qtbase` | **5.15.x** and **6.x** | Core/Gui/Widgets (+ Network as needed) |
| `org.qt.qtdeclarative` | matching majors | only if a flagship needs QML |

**Testing apps:** `org.example.editor` (Qt 6 Widgets, `[entry]` + `[desktop]`; optional FFmpeg dep) · `org.lar.test.qt5-widgets` (Qt 5.15 smoke)

**Depends on:** Phase C text/fonts; host GL via `[platform]` as required.

**Done when:** `org.example.editor` appears in the menu and launches via LAR PATH/desktop integration; `org.lar.test.qt5-widgets` launches against Qt 5.15.

---

## Phase E — GTK dual-major

Unlocks GTK 3 and GTK 4 apps.

| LAR id | Lines | Role |
|--------|-------|------|
| `org.gnome.gtk` | **3.x** and **4.x** | toolkit |
| Supporting GNOME libs | only as required by the testing apps | keep the graph closed |

**Testing apps:** `org.example.gtk-viewer` (GTK 4, `[entry]` + `[desktop]`) · `org.lar.test.gtk3-smoke` (GTK 3 smoke)

**Depends on:** Phase C (GLib, Cairo, Pango, gdk-pixbuf, …).

**Done when:** `org.example.gtk-viewer` installs and launches; `org.lar.test.gtk3-smoke` resolves and runs.

---

## Phase F — Prove the set

End-to-end validation of the curated source:

- Install **`org.example.editor`** and **`org.example.gtk-viewer`** from the published source
- `lar platform check` documents required caps for those apps
- Published tree via `lar repo init` / `publish` / `validate` as the **reference main source** layout

**Done when:** a clean prefix can `lar repo add` the tree, `lar install` both example apps, and launch them with platform checks passing on a typical Wayland host.

---

## Build / publish order

1. Phase A — Network/TLS, language runtimes, git  
2. Phase B — Media  
3. Phase C — Text / GUI shared deps  
4. Phase D — Qt dual-major + testing apps  
5. Phase E — GTK dual-major + testing apps  
6. Phase F — Publish and prove  

Within a phase: ship the testing app (or runtime) plus **only** the deps it needs (closed graph).

---

## Non-goals (this roadmap)

- Full desktop environments (GNOME/KDE sessions)
- Browsers (Firefox / Chromium) — revisit after Qt + media + GTK are proven
- Mesa / GPU stacks inside LAR (host `[platform]` only)
- Wine, full JVM app servers
- Expanding curl backends (SSH / Kerberos / HTTP/3) before Phases A–E
- Every optional codec Debian enables
- Exact public HTTP hosting layout (local published tree is enough until Phase F)

## Deferred

- Additional language runtimes (JRE, etc.) after Node/Python are stable
- Broader Qt modules (Quick Controls, WebEngine) only when a real app requires them
- Advisory corpus for the main source beyond yank/warn mechanics already in LAR
