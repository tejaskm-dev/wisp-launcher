An early preview of Wisp, a fast Minecraft: Java Edition launcher.

> **This build can't start Minecraft yet.** Playing needs Microsoft sign-in, which is waiting on Mojang approving Wisp's Azure app. Until it's in, you can set up instances, install mods and modpacks, keep your server list and make skins, but Play won't open the game. Offline accounts are only in developer and tester builds.

### Download

| System | File |
|---|---|
| macOS, Apple Silicon (M1 and later) | `Wisp_0.2.0_aarch64.dmg` |
| macOS, Intel | `Wisp_0.2.0_x64.dmg` |
| Windows, x64 | `Wisp_0.2.0_x64-setup.exe`, or the `.msi` |
| Windows, ARM64 | `Wisp_0.2.0_arm64-setup.exe` |
| Linux, x64 | the `amd64.AppImage`, `.deb` (Debian, Ubuntu) or `x86_64.rpm` (Fedora, openSUSE) |
| Linux, ARM64 | the `aarch64.AppImage`, `arm64.deb` or `aarch64.rpm` |

These builds aren't code-signed yet, so your system checks with you the first time:

- **macOS**: open the disk image and drag Wisp into Applications. If macOS says it can't check Wisp, open System Settings > Privacy & Security and choose Open Anyway.
- **Windows**: if SmartScreen stops the installer, choose More info, then Run anyway.
- **Linux**: install the .deb or .rpm, or make the AppImage executable (`chmod +x Wisp_*.AppImage`) and run it.

### What's new

If you have 0.1.9, Wisp offers this update in its own window: Update now installs it and restarts where you were.

- **Your game settings match in every instance**: FOV, keybinds, sound, video and the rest now start out the same everywhere. The first time you play after updating, every instance takes the settings of the one you played last, and from then on a change in any instance reaches the others when they next start. An instance's old settings are kept beside the new ones as `options.txt.before-sharing`, and any instance can keep its own.
- **Memory and settings beside Play**: every instance's page shows how much memory it gets and whether its settings are shared, right next to Play. Click either to change it.
- **Big modpacks get more memory by themselves**: an instance with lots of mods gets 6, 8 or 10 GB instead of the 4 GB vanilla needs, never more than 60% of your computer's memory. Give an instance its own memory and Wisp leaves it alone.

---

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT. Wisp is made by its own developers, who answer for it. Contact: wisplaunchercontact@gmail.com
