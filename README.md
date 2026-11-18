# Wisp

> NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.

A fast Minecraft: Java Edition launcher whose worlds, keybinds and settings follow you between computers.

> **Status:** early, but it plays. Create an instance with Vanilla, Fabric, Quilt, Forge or NeoForge, on any Minecraft version Mojang lists, and press Play: Wisp downloads the game, its libraries and assets and the right Java from Mojang, checks every file's SHA-1, and starts Minecraft. Mods, resource packs, shaders and modpacks install from Modrinth. Microsoft sign-in is next (it needs an Azure app ID that Mojang has approved); until then, **developer builds** can add an offline account for testing, and the installers on the Releases page can't start the game yet. CurseForge needs an API key from CurseForge first. Cloud sync works through a cloud folder you already have (iCloud Drive, OneDrive, Google Drive, Dropbox, Syncthing or any shared folder).

### Install

Installers for every system are on the [Releases](https://github.com/tejaskm-dev/wisp-launcher/releases) page:

| System | Download |
|---|---|
| macOS, Apple Silicon | `Wisp_<version>_aarch64.dmg` |
| macOS, Intel | `Wisp_<version>_x64.dmg` |
| Windows, x64 | `Wisp_<version>_x64-setup.exe`, or the `.msi` |
| Windows, ARM64 | `Wisp_<version>_arm64-setup.exe` |
| Linux, x64 | `.AppImage`, `.deb` or `.rpm` |
| Linux, ARM64 | `.AppImage`, `.deb` or `.rpm` |

They aren't code-signed yet, so the first launch needs a nod: on macOS, choose Open Anyway in System Settings > Privacy & Security; on Windows, choose More info, then Run anyway if SmartScreen stops the installer.

### What's in it

- **Home**: hero with your worlds and Play, your servers with who's online, cloud sync, quick actions, featured modpacks, Minecraft news.
- **Instances**: create instances with any loader, Minecraft version (releases, snapshots, betas and alphas) and loader version, give each one of 50 icons, rename or delete them, and play them. Progress shows on the Play buttons and in a panel at the bottom right; Stop cancels a download or quits the game. Forge and NeoForge run their official installer, headlessly, the first time a build is played.
- **Discover**: modpacks, mods, resource packs, shaders, data packs and worlds from Modrinth and CurseForge, with the filter rail on the right (source with each site's logo, kind, fits an instance, loaders, Minecraft version, client or server, categories), sorting, a grid or list view, and pages. Every project has a page with its description, gallery, versions and changelogs. "Add" puts a project in an instance with the newest version that fits plus anything it requires; modpacks install as new instances. Type in the top search bar from any page. A Servers tab lists popular public servers, live, to join or add to your list.
- **Worlds**: every world in every instance, read from its level.dat (name, mode, hardcore, cheats, difficulty, version, time played) with its icon or latest screenshot. Play goes straight into the world (Quick Play, Minecraft 1.20 and later; older versions open on the title screen). Each world has a page with its journal, restore points and screenshots, and can be exported as a zip, opened in its folder or moved to the Trash. Drop world zips or folders on the window to import them. Home's hero and worlds row play worlds directly too.
- **Servers**: your server list (the same `servers.dat` the game keeps) with each server's icon, message of the day, players and ping, refreshed live. Add a server by its address (SRV records work) and Join goes straight in with the instance you pick (Quick Play on Minecraft 1.20 and later, the older join flags before that). Servers sit with Worlds, as a tab.
- **Mod fixer**: before every launch, alongside the downloads, Wisp checks the instance's mods: dependencies that are missing or at the wrong version, mods made for another loader or Minecraft version, duplicates, a loader that's too old. It fixes what it safely can (fetching what's missing from Modrinth, swapping in versions that fit, turning off duplicates and mods for the wrong loader, updating the loader). If the game still crashes, Wisp reads the log for the cause (Fabric, Forge and NeoForge errors, the wrong Java, memory, an unreadable config), fixes it, and starts the game again once if the window never opened. Every fix lands in the journal with Undo; anything that could touch your worlds, like turning off the mod a crash points at, is offered as a button instead.
- **Screenshots**: from every instance, grouped by day and filed under the world they were taken in, with thumbnails made in Rust, filters, and a full-size viewer (arrow keys, copy, show in folder, move to the Trash).
- **Journal**: your game's history drawn like a git graph, a lane per instance: play sessions (worlds, time, screenshots, crashes), notes, restore points, mods and modpacks coming and going, and every fix the mod fixer makes, with Undo. When a game closes Wisp asks for a note (shown on Home next time) and keeps a restore point of each world you played. Restore points are stored like git commits: each file once, by its SHA-1, so a session only adds the region files it touched. Restoring keeps the current world as a restore point first.
- **Skins**: your skin in 3D (skinview3d) on the Wisp Hollow, a floating island built block by block in Minecraft's style: a calcite statue of Wisp, ember and gold trees, stone lanterns, will-o'-wisps, a moon over a sea of clouds. Every texture is drawn in code, and only the block faces you can see are meshed, with soft shading in the corners. Wisp's ghost lives there as a little mob: while you stand about it wanders, circles you, peeks over your shoulder, boops you and sits on your head, it joins in with every emote, and your head follows it. Five emotes, each easing into the next: Throne (a throne builds itself behind you, block by block, and you take your seat), Dance (on stained glass that lights up with the beat), Aura (fire gathers at your feet and bursts into a ring), T-pose and Crouch spam. Turntable turns you on the spot; Free roam drops the island away into a lit void where you can get close to every pixel, underneath included. Import PNGs (or drop them), grab any player's current skin by name, or paint your own on the model or the flat texture (pencil, eraser, fill, picker, mirror, base and overlay, undo) in layers, like a paint program, with ready-made ones so a skin isn't flat: shading, texture, outlines, and clothes in your colour. The skin you wear becomes your avatar in Wisp, shown in the sidebar (Wisp's own skin until you add one); wearing it in game needs Microsoft sign-in.
- **Friends**: off until you turn it on. Wisp makes you a friend code (a Nostr key, kept on your computer); requests, presence ("playing Fabric 26.3 · Island") and party invites travel through public Nostr relays as NIP-59 gift wraps, end-to-end encrypted and sent from a throwaway key, so relays can't read them or see who sent them. Nobody at Wisp runs a server. Parties work today; chat comes later.
- **Cloud sync**: pick a cloud folder already on the computer and your worlds, instances (mods download from Modrinth on each computer), options, keybinds and server lists follow you. The newest copy comes down before Play and changes go up when the game closes; a world changed on two computers is never merged (keep one copy, or both); worlds open on another computer are left alone. Screen and graphics settings stay on each computer.
- **Settings**: memory, downloads, startup animation, sidebar, journal and restore points, data folder.
- **Sidebar**: six pages (Servers sit with Worlds and Screenshots with the Journal, as tabs), recent instances for quick play, and an account menu with the face of the skin you wear, holding Skins, Cloud sync and Settings. It collapses to an icon rail.
- **Startup animation**: a wisp of light bursts into the logo over moonlit art, then the logo docks in the sidebar as the art settles into the home hero. Full, Quick or Off; any key skips it.

### Shortcuts

| | macOS | Windows / Linux |
|---|---|---|
| Search Modrinth | ⌘K | Ctrl+K |
| Go to a page (sidebar order) | ⌘1 to ⌘6 | Ctrl+1 to Ctrl+6 |
| Settings | ⌘, | Ctrl+, |
| Collapse or expand the sidebar | ⌘B | Ctrl+B |

## Stack

- **Tauri 2 (Rust)** for the app shell, and soon for all launcher work: downloads, Java, launching and sync.
- **React 19 + TypeScript + Vite**, **Tailwind CSS 4**, and **Motion** for animation.
- Fonts ship with the app, so nothing loads from the network: Bricolage Grotesque, Figtree and Instrument Serif.

## Run it

You need Node 20+, pnpm and Rust ([rustup](https://rustup.rs)), plus the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (Xcode Command Line Tools on macOS, WebView2 on Windows, WebKitGTK on Linux).

```bash
pnpm install
pnpm tauri dev     # the desktop app, with hot reload
pnpm dev           # the UI alone in a browser at http://localhost:1420
pnpm tauri build   # a release bundle in target/release/bundle
cargo test         # Rust tests; add `-- --ignored` to also check the live Modrinth and Mojang feeds
```

To release, set the new version in `package.json`, `src-tauri/tauri.conf.json` and `Cargo.toml`, update `.github/release-notes.md`, and push a tag like `v0.1.5`. The Release workflow builds macOS (Apple Silicon and Intel), Windows (x64 and ARM64) and Linux (x64 and ARM64) and publishes them as a pre-release once all six pass. Running it by hand builds without releasing; name some of `macos-arm64 macos-x64 windows-x64 windows-arm64 linux-x64 linux-arm64` to make only those.

In the browser preview, add `?state=loading` or `?state=error` to the URL to see the skeletons and error states. The preview keeps its settings and instances in the browser's local storage, and can't launch the game.

To test launching without the UI, `cargo run -p wisp-core --example launch_smoke -- 26.3 fabric` (or `quilt`, `forge`, `neoforge`) downloads a version into Wisp's data folder, starts it, and closes it once its window is up. `cargo run -p wisp-core --example content_smoke` installs a mod with its dependencies and a modpack into a throwaway folder.

Game files (versions, libraries, assets, Java runtimes) live once in Wisp's data folder and are shared by every instance; each instance's own worlds, options and mods are in `instances/<id>/minecraft`. The journal and restore points are in `journal/`, skins in `skins/`, friends in `social/` (with your friend key, readable only by you), and screenshot thumbnails in `cache/thumbs/`.

`cargo run -p wisp-core --example library_check` prints what Wisp reads from your worlds and screenshots without changing anything, and `cargo run -p wisp-core --example social_smoke` sends a friend request, an accept and a presence update between two throwaway identities over the public relays.

## Layout

```
crates/wisp-core/ launcher logic with no UI code: settings, instances, accounts, versions, launching
                  (downloads, Java, arguments, the game process), Modrinth search, Mojang feeds,
                  and cloud sync (sync/: an append-only folder of objects and commits; see sync/mod.rs)
src-tauri/        Tauri app: commands the UI calls, feed caching, config, icons, permissions
src/              React UI
  pages/          one file per page; every page but Home loads on first visit
  components/     sidebar, top bar, startup intro, hero, dashboard sections, instances, UI primitives
  state/          app-wide context (library, settings, navigation) and toasts
  data/           types (mirroring wisp-core), the IPC calls, and a dev-only browser fallback
```

## Performance rules

- Heavy work lives in Rust; React only renders.
- Animations only touch `transform` and `opacity`, and `prefers-reduced-motion` is respected (it also turns the startup animation off). The one exception is the startup art settling into the hero card, which resizes a single element once, for under a second.
- Pages other than Home load the first time you open them, so startup parses less code.
- Release builds use LTO, one codegen unit, `panic = "abort"` and stripped symbols.
- Ambient animations are GPU-only (no looping animations inside SVGs or with CSS variables in keyframes) and pause whenever Wisp isn't the focused window.
- While Minecraft runs, Wisp closes its window and keeps only its Rust process alive, which checks in with your other computers once a minute and syncs the worlds you played when the game exits.

Measured on macOS with a release build (Activity Monitor's "Memory" figure, idle on the home screen), before the live Modrinth and Mojang feeds and the pages in this build. Due for a re-measure; in the background the current build sits at about 172 MB and 0% CPU:

| | Memory | CPU, focused | CPU, background |
|---|---|---|---|
| Wisp (Rust + window) | 27 MB | 0.1% | 0% |
| WebKit page, GPU and network helpers | 158 MB | 0.2% | 0% |
| **Total** | **~185 MB** | **~0.3%** | **0%** |

The app bundle is 15 MB on Apple Silicon as of 0.1.5 (7.8 MB before friends, servers and the 3D skin stage; 4.8 MB before the networking code).
