An early preview of Wisp, a fast Minecraft: Java Edition launcher.

> **This build can't start Minecraft yet.** Playing needs Microsoft sign-in, which is waiting on Mojang approving Wisp's Azure app. Until it's in, you can set up instances, install mods and modpacks, keep your server list and make skins, but Play won't open the game. Offline accounts are for developer builds only.

### Download

| System | File |
|---|---|
| macOS, Apple Silicon (M1 and later) | `Wisp_0.1.6_aarch64.dmg` |
| macOS, Intel | `Wisp_0.1.6_x64.dmg` |
| Windows, x64 | `Wisp_0.1.6_x64-setup.exe`, or the `.msi` |
| Windows, ARM64 | `Wisp_0.1.6_arm64-setup.exe` |
| Linux, x64 | the `amd64.AppImage`, `.deb` (Debian, Ubuntu) or `x86_64.rpm` (Fedora, openSUSE) |
| Linux, ARM64 | the `aarch64.AppImage`, `arm64.deb` or `aarch64.rpm` |

These builds aren't code-signed yet, so your system checks with you the first time:

- **macOS**: open the disk image and drag Wisp into Applications. If macOS says it can't check Wisp, open System Settings > Privacy & Security and choose Open Anyway.
- **Windows**: if SmartScreen stops the installer, choose More info, then Run anyway.
- **Linux**: install the .deb or .rpm, or make the AppImage executable (`chmod +x Wisp_*.AppImage`) and run it.

### What's new

- **Wisp updates itself from here on**: when a new version is out, Wisp offers it in the corner, then installs it and restarts right where you were (never while you're playing). 0.1.5 couldn't do that, so this one is the last you install by hand.
- **Bring your own modpacks and mods**: Add files on Instances, Add from file on Discover, or drop files anywhere on the window. `.mrpack` and CurseForge modpack zips become instances; `.jar` mods and resource or shader pack zips go into the instance you pick. Wisp tells files apart by what's inside them, and ones that are on Modrinth show their real name and icon.
- **Speed up** (in an instance's Edit dialog): Sodium, Lithium, FerriteCore, ModernFix, ImmediatelyFast, Entity Culling and Dynamic FPS in one click, where they have builds for that version. A vanilla instance moves to Fabric first.
- **Screenshots in albums**, one for each world, with the full timeline a tap away.
- **A quicker Skins page**: the page shows at once while the 3D stage builds and fades in, with far fewer shaders to build and lighter frames.
- **Tabs that stay put**: Worlds and Servers, and Timeline and Screenshots, now switch in place instead of reloading the page.
- **Ready for when Play works**: game starts will use the same memory settings as Mojang's launcher and a cache of the game's classes, and Wisp can close itself while you play (Settings, General) and come back when you quit.
