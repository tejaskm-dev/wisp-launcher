//! Minecraft's options.txt for new instances: copied from another instance (settings and
//! keybinds) and seeded with the defaults picked in Wisp's settings.

use std::path::Path;

use crate::settings::NewInstanceOptions;

const FILE: &str = "options.txt";

/// The options.txt lines Wisp's settings ask new instances to start with.
pub fn chosen_defaults(options: &NewInstanceOptions) -> Vec<(&'static str, &'static str)> {
    let mut set = Vec::new();
    if options.skip_welcome {
        set.extend([
            // The accessibility screen on first launch (1.19.4 and later).
            ("onboardAccessibility", "false"),
            // The warning before playing online, and the social interactions hint after.
            ("skipMultiplayerWarning", "true"),
            ("joinedFirstServer", "true"),
        ]);
    }
    if options.skip_tutorial {
        set.push(("tutorialStep", "none"));
    }
    if options.auto_jump_off {
        set.push(("autoJump", "false"));
    }
    set
}

/// Writes a new instance's options.txt, unless it already has one (a modpack's, say). Returns
/// whether it wrote anything.
pub fn seed(
    game_dir: &Path,
    copy_from: Option<&Path>,
    set: &[(&str, &str)],
) -> std::io::Result<bool> {
    let target = game_dir.join(FILE);
    if target.exists() {
        return Ok(false);
    }
    let mut lines: Vec<String> = copy_from
        .and_then(|dir| std::fs::read_to_string(dir.join(FILE)).ok())
        .map(|text| text.lines().map(String::from).collect())
        .unwrap_or_default();
    for (key, value) in set {
        let line = format!("{key}:{value}");
        match lines
            .iter_mut()
            .find(|existing| existing.split(':').next() == Some(key))
        {
            Some(existing) => *existing = line,
            None => lines.push(line),
        }
    }
    if lines.is_empty() {
        return Ok(false);
    }
    std::fs::create_dir_all(game_dir)?;
    std::fs::write(target, lines.join("\n") + "\n")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_then_applies_the_chosen_defaults() {
        let root = std::env::temp_dir().join(format!("wisp-options-{}", std::process::id()));
        let source = root.join("old");
        let target = root.join("new");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join(FILE),
            "version:3955\nautoJump:true\nkey_key.jump:key.keyboard.space\n",
        )
        .unwrap();
        let set = chosen_defaults(&NewInstanceOptions {
            auto_jump_off: true,
            ..NewInstanceOptions::default()
        });
        assert!(seed(&target, Some(&source), &set).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join(FILE)).unwrap(),
            "version:3955\nautoJump:false\nkey_key.jump:key.keyboard.space\n"
        );
        // An instance that already has options keeps them.
        assert!(!seed(&target, None, &[("autoJump", "true")]).unwrap());
        // Nothing to copy and nothing chosen: no file at all.
        assert!(!seed(&root.join("empty"), None, &[]).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
}
