An early preview of Wisp, a fast Minecraft: Java Edition launcher.

> **This build can't start Minecraft yet.** Playing needs Microsoft sign-in, which is waiting on Mojang approving Wisp's Azure app. Until it's in, you can set up instances, install mods and modpacks, keep your server list and make skins, but Play won't open the game. Offline accounts are for developer builds only.

### Download

| System | File |
|---|---|
| macOS, Apple Silicon (M1 and later) | `Wisp_0.1.8_aarch64.dmg` |
| macOS, Intel | `Wisp_0.1.8_x64.dmg` |
| Windows, x64 | `Wisp_0.1.8_x64-setup.exe`, or the `.msi` |
| Windows, ARM64 | `Wisp_0.1.8_arm64-setup.exe` |
| Linux, x64 | the `amd64.AppImage`, `.deb` (Debian, Ubuntu) or `x86_64.rpm` (Fedora, openSUSE) |
| Linux, ARM64 | the `aarch64.AppImage`, `arm64.deb` or `aarch64.rpm` |

These builds aren't code-signed yet, so your system checks with you the first time:

- **macOS**: open the disk image and drag Wisp into Applications. If macOS says it can't check Wisp, open System Settings > Privacy & Security and choose Open Anyway.
- **Windows**: if SmartScreen stops the installer, choose More info, then Run anyway.
- **Linux**: install the .deb or .rpm, or make the AppImage executable (`chmod +x Wisp_*.AppImage`) and run it.

### What's new

If you have 0.1.7, Wisp offers this update by itself: it installs and restarts where you were. From this version on, a new Wisp is announced in its own window, with what's new, Update now and Remind me later.

- **A page for every instance**: Play (or straight into one of its worlds), when it was last played and for how long, and tabs. **Content** lists every mod, resource pack, shader and data pack, the modpack's own files included, named and pictured through Modrinth: turn any of them off and on, pick another version, update one or all, or remove one (it can be put back). Libraries other mods need sit in their own group, saying what needs them. A modpack instance can update to the pack's newer versions and keep the mods you added. Worlds, screenshots, the journal, servers and the game's logs (with problems picked out) are each instance's own. **Settings** moves an instance to another Minecraft version or loader (after showing which mods have a build), sets its memory and Java and, on this computer, its own Java arguments, window and launch commands, makes a copy, and exports it as a Modrinth modpack (.mrpack).
- **Settings follow you between instances**: FOV, keybinds, sound, video and the rest reach your other instances on the same computer the next time they start. Any instance can keep its own.
- **Sync does more by itself**: a world changed on two computers is settled for you (the copy played last wins, and the other is kept as a restore point), Play waits for files your cloud app is still bringing instead of asking, big worlds come down while there's room, and Home offers to join a cloud folder another computer already syncs through, in one click.
- **The mod fixer is smarter, and careful**: no more "Fixing 2 problems" on every launch (mods made for several loaders were mistaken for missing NeoForge and Forge), it checks a mod before installing it and never deletes anything, it puts back mods whose files went missing, and what's left after a crash is grouped into a few lines instead of one per mod.
- **Fixed**: modpack icons lost their edges; a stopped download left a partial file behind; a modpack listing one file twice wouldn't install.
---

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT. Wisp is made by its own developers, who answer for it. Contact: wisplaunchercontact@gmail.com
