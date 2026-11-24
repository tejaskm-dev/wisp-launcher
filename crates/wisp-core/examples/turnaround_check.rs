//! Reads image-tool paintings of the skin template into skins, to check the reader by hand:
//!
//!   turnaround_check <out folder> [--slim] <painting>...
//!
//!   turnaround_check depth <out folder> [--slim] <skin>...
//!
//!   turnaround_check redo <out folder> [--slim] <skin>...
//!
//!   turnaround_check flat <out folder> [--slim] <painting>...
//!
//!   turnaround_check layers <out folder> [--slim] <painting> <outer-layer painting>
//!
//! Each skin is saved as <out folder>/<painting name>.png. `depth` instead clears each skin's
//! outer layer and fills it again with `skin_depth::add_depth`; `redo` tidies the face and hair
//! first (`skin_tidy::tidy`), as reading a painting does; `flat` reads paintings onto the base
//! layer only, as painted; `layers` reads a painting with its follow-up for the outer layer.

use std::path::Path;

use wisp_core::skins::SkinModel;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().cloned().filter(|first| ["depth", "redo", "flat", "layers"].contains(&first.as_str()));
    if mode.is_some() {
        args.remove(0);
    }
    let out = args.remove(0);
    let model = if args.first().map(String::as_str) == Some("--slim") {
        args.remove(0);
        SkinModel::Slim
    } else {
        SkinModel::Classic
    };
    std::fs::create_dir_all(&out).unwrap();
    if mode.as_deref() == Some("layers") {
        let (painting, outer) = (image::open(&args[0]).unwrap().to_rgba8(), image::open(&args[1]).unwrap().to_rgba8());
        let name = Path::new(&args[0]).file_stem().unwrap().to_string_lossy().into_owned();
        match wisp_core::turnaround::read_layers(&painting, &outer, model) {
            Ok(skin) => {
                skin.save(Path::new(&out).join(format!("{name}.png"))).unwrap();
                println!("{name}: read with its outer layer");
            }
            Err(error) => println!("{name}: {error}"),
        }
        return;
    }
    for path in &args {
        let name = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
        let painting = image::open(path).unwrap().to_rgba8();
        if mode.as_deref() == Some("flat") {
            match wisp_core::turnaround::read_flat(&painting, model) {
                Ok(skin) => skin.save(Path::new(&out).join(format!("{name}.png"))).unwrap(),
                Err(error) => println!("{name}: {error}"),
            }
            continue;
        }
        if let Some(mode) = &mode {
            let mut skin = painting;
            for y in 0..64 {
                for x in 0..64 {
                    if wisp_core::skins::is_outer(x, y) {
                        skin.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
                    }
                }
            }
            if mode == "redo" {
                wisp_core::skin_tidy::tidy(&mut skin, model);
            }
            wisp_core::skin_depth::add_depth(&mut skin, model);
            skin.save(Path::new(&out).join(format!("{name}.png"))).unwrap();
            println!("{name}: {mode} done");
            continue;
        }
        let started = std::time::Instant::now();
        match wisp_core::turnaround::read(&painting, model) {
            Ok(skin) => {
                skin.save(Path::new(&out).join(format!("{name}.png"))).unwrap();
                println!("{name}: read in {:?}", started.elapsed());
            }
            Err(error) => println!("{name}: {error}"),
        }
    }
}
