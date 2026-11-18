An early preview of Wisp, a fast Minecraft: Java Edition launcher.

> **This build can't start Minecraft yet.** Playing needs Microsoft sign-in, which is waiting on Mojang approving Wisp's Azure app. Until it's in, you can set up instances, install mods and modpacks, keep your server list and make skins, but Play won't open the game. Offline accounts are for developer builds only.

### Download

| System | File |
|---|---|
| macOS, Apple Silicon (M1 and later) | `Wisp_0.1.7_aarch64.dmg` |
| macOS, Intel | `Wisp_0.1.7_x64.dmg` |
| Windows, x64 | `Wisp_0.1.7_x64-setup.exe`, or the `.msi` |
| Windows, ARM64 | `Wisp_0.1.7_arm64-setup.exe` |
| Linux, x64 | the `amd64.AppImage`, `.deb` (Debian, Ubuntu) or `x86_64.rpm` (Fedora, openSUSE) |
| Linux, ARM64 | the `aarch64.AppImage`, `arm64.deb` or `aarch64.rpm` |

These builds aren't code-signed yet, so your system checks with you the first time:

- **macOS**: open the disk image and drag Wisp into Applications. If macOS says it can't check Wisp, open System Settings > Privacy & Security and choose Open Anyway.
- **Windows**: if SmartScreen stops the installer, choose More info, then Run anyway.
- **Linux**: install the .deb or .rpm, or make the AppImage executable (`chmod +x Wisp_*.AppImage`) and run it.

### What's new

If you have 0.1.6, Wisp offers this update by itself: it installs and restarts where you were.

- **Cloud sync**: your worlds, instances, keybinds and server lists follow you between computers, through a cloud folder you already have (iCloud Drive, OneDrive, Google Drive, Dropbox, Syncthing, or any folder you pick). Nothing signs in and nothing goes to Wisp's servers. The newest copy comes down before you play and changes go up when the game closes. A world changed on two computers is never mixed: you keep one copy, the other, or both. Mods download fresh from Modrinth on each computer, so only jars you added yourself take space in your cloud. Screen and graphics settings stay on each computer. Find it under your account, Cloud sync.
- **Switches shaped like Wisp's ghost**: the ghost dozes when a setting's off, and hops across and wakes up when you switch it on.
- **CurseForge only shows up when there's a key for it**: without one, Discover browses Modrinth and nothing else mentions CurseForge. CurseForge modpack zips still open, and say what they need.
- **Fixed**: games wouldn't start on offline accounts in developer builds.

---

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT. Wisp is made by its own developers, who answer for it. Contact: wisplaunchercontact@gmail.com
