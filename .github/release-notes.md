An early preview of Wisp, a fast Minecraft: Java Edition launcher.

> **This build can't start Minecraft yet.** Playing needs Microsoft sign-in, which is waiting on Mojang approving Wisp's Azure app. Until it's in, you can set up instances, install mods and modpacks, keep your server list and make skins, but Play won't open the game. Offline accounts are for developer builds only.

### Download

| System | File |
|---|---|
| macOS, Apple Silicon (M1 and later) | `Wisp_0.1.5_aarch64.dmg` |
| macOS, Intel | `Wisp_0.1.5_x64.dmg` |
| Windows, x64 | `Wisp_0.1.5_x64-setup.exe`, or the `.msi` |
| Windows, ARM64 | `Wisp_0.1.5_arm64-setup.exe` |
| Linux, x64 | the `amd64.AppImage`, `.deb` (Debian, Ubuntu) or `x86_64.rpm` (Fedora, openSUSE) |
| Linux, ARM64 | the `aarch64.AppImage`, `arm64.deb` or `aarch64.rpm` |

These builds aren't code-signed yet, so your system checks with you the first time:

- **macOS**: open the disk image and drag Wisp into Applications. If macOS says it can't check Wisp, open System Settings > Privacy & Security and choose Open Anyway.
- **Windows**: if SmartScreen stops the installer, choose More info, then Run anyway.
- **Linux**: install the .deb or .rpm, or make the AppImage executable (`chmod +x Wisp_*.AppImage`) and run it.

### What's new

- **Servers**: your server list (the same one Minecraft keeps) with live player counts, pings and messages of the day. Join goes straight into a server, and Discover has a tab of popular public servers.
- **A mod fixer**: before each launch, Wisp checks an instance's mods for missing or mismatched dependencies, mods made for another loader or Minecraft version, duplicates and an outdated loader, and fixes what it safely can. If the game still crashes, it reads the log for the cause and fixes that too. Every fix shows in the journal with Undo.
- **Quick play** straight into worlds and servers.
- **A simpler sidebar**: six pages, with Skins, Cloud sync and Settings in the account menu.
- **Skins in a Minecraft-style world**: your skin stands on a floating island built block by block, with a Wisp statue, lanterns and the Wisp ghost, who lives there and plays with you. Block-built emotes (a throne, a ring of fire, a glass dance floor), and Free roam to see every side of a skin up close.
- **Layers in the skin editor**, with ready-made shading, texture, outlines and clothes in your colour.
