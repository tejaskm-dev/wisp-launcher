An early preview of Wisp, a fast Minecraft: Java Edition launcher.

> **This build can't start Minecraft yet.** Playing needs Microsoft sign-in, which is waiting on Mojang approving Wisp's Azure app. Until it's in, you can set up instances, install mods and modpacks, keep your server list and make skins, but Play won't open the game. Offline accounts are only in developer and tester builds.

### Download

| System | File |
|---|---|
| macOS, Apple Silicon (M1 and later) | `Wisp_0.1.9_aarch64.dmg` |
| macOS, Intel | `Wisp_0.1.9_x64.dmg` |
| Windows, x64 | `Wisp_0.1.9_x64-setup.exe`, or the `.msi` |
| Windows, ARM64 | `Wisp_0.1.9_arm64-setup.exe` |
| Linux, x64 | the `amd64.AppImage`, `.deb` (Debian, Ubuntu) or `x86_64.rpm` (Fedora, openSUSE) |
| Linux, ARM64 | the `aarch64.AppImage`, `arm64.deb` or `aarch64.rpm` |

These builds aren't code-signed yet, so your system checks with you the first time:

- **macOS**: open the disk image and drag Wisp into Applications. If macOS says it can't check Wisp, open System Settings > Privacy & Security and choose Open Anyway.
- **Windows**: if SmartScreen stops the installer, choose More info, then Run anyway.
- **Linux**: install the .deb or .rpm, or make the AppImage executable (`chmod +x Wisp_*.AppImage`) and run it.

### What's new

If you have 0.1.8, Wisp offers this update in its own window: Update now installs it and restarts where you were.

- **Quitting isn't a crash any more**: on some computers, Windows especially, leaving Minecraft with Quit Game, Alt+F4 or the window's close button said Wisp "closed unexpectedly", skipped the restore point Wisp makes after you play, and put a crash in the journal. A game that shuts down normally now counts as quit, whatever its exit code, and the game's last-run log ends with that code so a real crash is easy to look into.
- **Crouch spam crouches like the game**: the Skins page emote now uses Minecraft's own sneak, with the body tipped forward from the neck, hips back and the head dropping straight down. It snaps on game ticks in quick bursts, and the ghost bobs back.
- **Tester codes**: if one of Wisp's testers sends you a code, paste it into Settings, About, and Wisp switches itself to tester builds, which keep themselves up to date. Tester builds are only for people who own Minecraft.

---

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT. Wisp is made by its own developers, who answer for it. Contact: wisplaunchercontact@gmail.com
