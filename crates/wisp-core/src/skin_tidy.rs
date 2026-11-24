//! Tidying a skin read from a painting, in the painting's own style. Image tools paint more than
//! 64 by 64 pixels can carry: a face in many skin tones with eyes half lost in the fringe, hair
//! in a dozen unrelated shades, clothes speckled with shading. Good skins (the most-used on NameMC,
//! Mojang's defaults, and the ones in the player's own library, November 2026) draw faces in every
//! style, from dots to big lashed eyes, but keep them readable: one skin tone, eyes on skin, both
//! alike. Their hair is one colour with its own highlights and shadows, and their clothes are
//! flat areas with few details. So `tidy` keeps each face as painted (its eyes' look, its blush,
//! the hair framing it) and only cleans it: one skin tone, eyes cleared of the hair around them
//! and lined up, a lost eye mirrored onto bare skin. The hair becomes its main colour on a gentle
//! ramp (two colours for streaks or tips), kept apart from the skin, and the clothes lose their
//! speckle. Masked and faceless characters keep their heads as painted. The outer layer comes
//! after (`skin_depth`).

use image::{Rgba, RgbaImage};

use crate::skin_depth::{Grid, HEAD, Head, Rgb, apart, luma, skin_like};
use crate::skins::SkinModel;
use crate::turnaround::{Face, Part, parts};

/// Tidies a flat skin's face, hair and clothes (see the module notes).
pub fn tidy(skin: &mut RgbaImage, model: SkinModel) {
    if skin.dimensions() != (64, 64) {
        return;
    }
    let parts = parts(model);
    let head = Head::read(skin, &parts);
    if !head.covered {
        hair(skin, &parts, &head);
        let head = Head::read(skin, &parts);
        face(skin, &parts, &head);
    }
    clothes(skin, &parts);
}

fn put(skin: &mut RgbaImage, part: Part, face: Face, x: u32, y: u32, c: Rgb) {
    let (x0, y0, _, _) = part.face(face);
    skin.put_pixel(x0 + x, y0 + y, Rgba([c[0], c[1], c[2], 255]));
}

fn saturation(c: Rgb) -> f32 {
    let (max, min) = (c.iter().copied().max().unwrap_or(0) as f32, c.iter().copied().min().unwrap_or(0) as f32);
    if max <= 0.0 { 0.0 } else { (max - min) / max }
}

fn distance(a: Rgb, b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] as f32 - b[i]).powi(2)).sum()
}

fn nearest(centres: &[[f32; 3]], c: Rgb) -> usize {
    (0..centres.len()).min_by(|&a, &b| distance(c, centres[a]).total_cmp(&distance(c, centres[b]))).unwrap_or(0)
}

/// Up to `k` colours standing in for many (k-means from starts spread over brightness), with
/// near ones merged.
fn few_shades(colours: &[Rgb], k: usize) -> Vec<Rgb> {
    let mut sorted = colours.to_vec();
    sorted.sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
    let mut centres: Vec<[f32; 3]> = (0..k).map(|i| sorted[(2 * i + 1) * sorted.len() / (2 * k)].map(f32::from)).collect();
    for _ in 0..12 {
        let mut sums = vec![([0f32; 3], 0u32); k];
        for c in colours {
            let i = nearest(&centres, *c);
            for (sum, value) in sums[i].0.iter_mut().zip(c) {
                *sum += *value as f32;
            }
            sums[i].1 += 1;
        }
        for (centre, (sum, n)) in centres.iter_mut().zip(&sums) {
            if *n > 0 {
                *centre = sum.map(|v| v / *n as f32);
            }
        }
    }
    let mut shades: Vec<Rgb> = Vec::new();
    for centre in centres {
        let c = centre.map(|v| v.round() as u8);
        if !shades.iter().any(|s| apart(*s, c) <= 14) {
            shades.push(c);
        }
    }
    shades
}

/// A colour made lighter (`delta` above zero) or darker by about `delta` in brightness, keeping
/// its hue.
fn shade_by(c: Rgb, delta: f32) -> Rgb {
    let l = luma(c).max(1.0);
    if delta >= 0.0 {
        let t = (delta / (255.0 - l).max(1.0)).min(1.0);
        c.map(|v| (v as f32 + (255.0 - v as f32) * t).round() as u8)
    } else {
        let t = (-delta / l).min(1.0);
        c.map(|v| (v as f32 * (1.0 - t)).round() as u8)
    }
}

/// Hair of nearly the skin's colour reads as a bald head, so artists keep the two apart: the
/// hair goes paler and yellower (blond, cream) or darker, whichever parts them sooner.
fn away_from_skin(hair: Rgb, skin: Rgb) -> Rgb {
    let pale = [250.0, 238.0, 200.0];
    let mut c = hair;
    for _ in 0..6 {
        if apart(c, skin) >= 40 {
            break;
        }
        let paler = [0, 1, 2].map(|i| (c[i] as f32 + (pale[i] - c[i] as f32) * 0.35).round() as u8);
        let deeper = shade_by(c, -14.0);
        c = if apart(paler, skin) >= apart(deeper, skin) { paler } else { deeper };
    }
    c
}

/// Hair the way good skins shade it: its main colour with a highlight and a shadow of that same
/// colour, following where the painting is lighter and darker, and a second colour only for
/// streaks or tips that cover a fair part of it (specks of another colour join the main one). A
/// pixel that breaks a strand (the shade above and below it agreeing) joins it.
fn hair(skin: &mut RgbaImage, parts: &[Part; 6], head: &Head) {
    if head.bald {
        return;
    }
    let hair: Vec<(Face, Grid, Vec<bool>)> = [Face::Top, Face::Front, Face::Right, Face::Left, Face::Back]
        .into_iter()
        .map(|face| {
            let grid = Grid::read(skin, parts[HEAD], face);
            let region = head.region(&grid, face);
            (face, grid, region)
        })
        .collect();
    let colours: Vec<Rgb> = hair.iter().flat_map(|(_, grid, region)| grid.cells.iter().zip(region).filter(|(_, r)| **r).map(|(c, _)| *c)).collect();
    if colours.len() < 16 {
        return;
    }
    let halves = few_shades(&colours, 2);
    let centres: Vec<[f32; 3]> = halves.iter().map(|s| s.map(f32::from)).collect();
    let counts: Vec<usize> = (0..centres.len()).map(|i| colours.iter().filter(|c| nearest(&centres, **c) == i).count()).collect();
    // A second colour is a streak or tips when it covers a fair part of the hair, or a smaller
    // part in runs (most of its pixels beside another of it); scattered specks are shading.
    let minor = if counts.len() == 2 && counts[1] < counts[0] { 1 } else { 0 };
    let (mut together, mut alone) = (0usize, 0usize);
    for (_, grid, region) in &hair {
        let (w, h) = (grid.w as i64, grid.h as i64);
        let of_minor = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h && region[(y * w + x) as usize] && nearest(&centres, grid.cells[(y * w + x) as usize]) == minor;
        for y in 0..h {
            for x in 0..w {
                if of_minor(x, y) {
                    if [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| of_minor(x + dx, y + dy)) {
                        together += 1;
                    } else {
                        alone += 1;
                    }
                }
            }
        }
    }
    let share = counts.get(minor).copied().unwrap_or(0) * 100 / colours.len().max(1);
    let runs = together * 10 >= (together + alone) * 6;
    let two = halves.len() == 2 && apart(halves[0], halves[1]) > 60 && (share >= 15 || (share >= 5 && runs));
    let mut families: Vec<[f32; 3]> = if two {
        let mut order: Vec<usize> = vec![0, 1];
        order.sort_by_key(|&i| std::cmp::Reverse(counts[i]));
        order.into_iter().map(|i| centres[i]).collect()
    } else {
        vec![[0, 1, 2].map(|i| colours.iter().map(|c| c[i] as f32).sum::<f32>() / colours.len() as f32)]
    };
    // Each colour's ramp, and where its shadow and highlight begin.
    let ramps: Vec<([Rgb; 3], f32, f32)> = (0..families.len())
        .map(|f| {
            let mut members: Vec<Rgb> = colours.iter().copied().filter(|c| nearest(&families, *c) == f).collect();
            members.sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
            let quantile = |q: f32| luma(members[((members.len() - 1) as f32 * q) as usize]);
            // Its own colour: the painted pixel nearest its centre (the middle by brightness can be
            // a stray, such as a pink of the face where pale hair meets pale skin).
            let mut mid = members.iter().copied().min_by(|a, b| distance(*a, families[f]).total_cmp(&distance(*b, families[f]))).unwrap_or(members[members.len() / 2]);
            if f == 0
                && let Some(tone) = head.skin
            {
                mid = away_from_skin(mid, tone);
            }
            ([shade_by(mid, -18.0), mid, shade_by(mid, 16.0)], quantile(0.25), quantile(0.8))
        })
        .collect();
    // Which colour each pixel belongs to is settled before the main one moves off the skin.
    let shade_of = |c: Rgb| -> (usize, usize) {
        let f = nearest(&families, c);
        let (_, shadow, light) = ramps[f];
        let l = luma(c);
        (f, if l < shadow { 0 } else if l > light { 2 } else { 1 })
    };
    for (face, grid, region) in hair {
        let (w, h) = (grid.w, grid.h);
        let at = |x: u32, y: u32| (y * w + x) as usize;
        let mut shade: Vec<Option<(usize, usize)>> = grid.cells.iter().zip(&region).map(|(c, r)| r.then(|| shade_of(*c))).collect();
        for _ in 0..2 {
            let before = shade.clone();
            for y in 1..h.saturating_sub(1) {
                for x in 0..w {
                    if let (Some(here), Some(up), Some(down)) = (before[at(x, y)], before[at(x, y - 1)], before[at(x, y + 1)])
                        && up == down
                        && here != up
                    {
                        shade[at(x, y)] = Some(up);
                    }
                }
            }
        }
        for y in 0..h {
            for x in 0..w {
                if let Some((f, i)) = shade[at(x, y)] {
                    put(skin, parts[HEAD], face, x, y, ramps[f].0[i]);
                }
            }
        }
    }
    families.clear();
}

/// In the hair's colours (a strand over the face isn't an eye).
fn hair_like(head: &Head, c: Rgb) -> bool {
    head.colours.iter().any(|h| apart(c, *h) <= 30)
}

/// The face, cleaned in its own style. One skin tone where the painting has many (blush, being
/// redder than the skin, stays) and alike colours in its features merged. Then the eyes, in the
/// look they were painted with: the hair right beside them and between them cleared to skin so
/// they show, both lined up alike when the reading set them a column apart, and an eye lost on
/// one side mirrored from the other onto bare skin (an eye under a strand of hair stays under it).
fn face(skin: &mut RgbaImage, parts: &[Part; 6], head: &Head) {
    let Some(near) = head.skin else { return };
    let part = parts[HEAD];
    let grid = Grid::read(skin, part, Face::Front);
    let (w, h) = (grid.w, grid.h);
    let at = |x: u32, y: u32| (y * w + x) as usize;
    // The face's own tone, as painted: the skin colour most of its skin is closest to (strands
    // of hair over the face, however many, aren't skin; and skin is warm, unless this character's
    // isn't).
    let warm = |c: Rgb| skin_like(c) || !skin_like(near);
    let skins: Vec<Rgb> = grid.cells.iter().copied().filter(|c| apart(*c, near) <= 40 && head.is_skin(*c) && warm(*c)).collect();
    let Some(tone) = skins.iter().copied().max_by_key(|c| skins.iter().filter(|o| apart(*c, **o) <= 10).count()) else { return };
    let redder = |c: Rgb| (c[0] as i32 - c[1] as i32) - (tone[0] as i32 - tone[1] as i32);
    let plain = |c: Rgb| head.is_skin(c) && warm(c) && redder(c) <= 30;
    // A mouth in the middle of the lower face stays, faint as it may be: redder than the skin
    // (lips), or clearly darker (a line), not just a shade of it.
    let mouth = |x: u32, y: u32, c: Rgb| (3..=4).contains(&x) && y >= 5 && !hair_like(head, c) && (luma(tone) - luma(c) >= 35.0 || redder(c) >= 8);
    let hair = head.region(&grid, Face::Front);
    let mut cells = grid.cells.clone();
    for (i, c) in cells.iter_mut().enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        if !hair[i] && plain(*c) && !mouth(x, y, *c) {
            *c = tone;
        }
    }
    // The features (eyes, lashes, brows, mouth, blush): alike colours merged into the commonest.
    let features: Vec<usize> = (0..cells.len()).filter(|&i| !hair[i] && cells[i] != tone).collect();
    let colours: Vec<Rgb> = features.iter().map(|&i| cells[i]).collect();
    let mut ordered = colours.clone();
    ordered.sort_by_key(|c| std::cmp::Reverse(colours.iter().filter(|o| apart(*c, **o) <= 24).count()));
    let mut kept: Vec<Rgb> = Vec::new();
    for c in ordered {
        if !kept.iter().any(|k| apart(*k, c) <= 24) {
            kept.push(c);
        }
    }
    for &i in &features {
        if let Some(k) = kept.iter().find(|k| apart(**k, cells[i]) <= 24) {
            cells[i] = *k;
        }
    }
    eyes(&mut cells, &hair, w, tone, head);
    for y in 0..h {
        for x in 0..w {
            if cells[at(x, y)] != grid.at(x, y) {
                put(skin, part, Face::Front, x, y, cells[at(x, y)]);
            }
        }
    }
    // The sides of the head: plain skin where it's skin (an ear's many tones).
    for face in [Face::Right, Face::Left] {
        let grid = Grid::read(skin, part, face);
        let hair = head.region(&grid, face);
        for y in 0..grid.h {
            for x in 0..grid.w {
                if !hair[(y * grid.w + x) as usize] && plain(grid.at(x, y)) {
                    put(skin, part, face, x, y, tone);
                }
            }
        }
    }
}

/// The eyes on a face (`cells`, `w` wide; `hair` is where the hair is): see `face`.
fn eyes(cells: &mut [Rgb], hair: &[bool], w: u32, tone: Rgb, head: &Head) {
    let at = |x: u32, y: u32| (y * w + x) as usize;
    // Where the hair is, as the eyes move: an eye moved or mirrored over hair is an eye there.
    let mut hair = hair.to_vec();
    let eyelike = |c: Rgb| {
        c != tone && !hair_like(head, c) && (luma(c) < 80.0 || (saturation(c) > 0.3 && !skin_like(c)) || (luma(c) > 200.0 && saturation(c) < 0.2))
    };
    // The eye on one side: the topmost shape of eye-like pixels there (a dark collar or mouth
    // lower on the face is something else).
    let find = |cells: &[Rgb], hair: &[bool], xs: std::ops::RangeInclusive<u32>| -> Vec<(u32, u32)> {
        let marks: Vec<(u32, u32)> = (2..=6u32).flat_map(|y| xs.clone().map(move |x| (x, y))).filter(|&(x, y)| !hair[at(x, y)] && eyelike(cells[at(x, y)])).collect();
        let Some(&first) = marks.first() else { return Vec::new() };
        let mut shape = vec![first];
        let mut grown = true;
        while grown {
            grown = false;
            for &m in &marks {
                if !shape.contains(&m) && shape.iter().any(|s| s.0.abs_diff(m.0) <= 1 && s.1.abs_diff(m.1) <= 1) {
                    shape.push(m);
                    grown = true;
                }
            }
        }
        shape
    };
    let span = |eye: &[(u32, u32)]| -> Option<(u32, u32, u32, u32)> {
        Some((eye.iter().map(|p| p.0).min()?, eye.iter().map(|p| p.0).max()?, eye.iter().map(|p| p.1).min()?, eye.iter().map(|p| p.1).max()?))
    };
    let (mut left, mut right) = (find(cells, &hair, 1..=3), find(cells, &hair, 4..=6));
    // Lined up alike: the same shape a column off its mirror moves inward to match it.
    if let (Some(l), Some(r)) = (span(&left), span(&right))
        && l.1 - l.0 == r.1 - r.0
        && (l.2, l.3) == (r.2, r.3)
        && l.0 != 7 - r.1
    {
        let (moving, by): (&mut Vec<(u32, u32)>, i64) = if l.0 < 7 - r.1 { (&mut left, (7 - r.1 - l.0) as i64) } else { (&mut right, -((r.0 - (7 - l.1)) as i64)) };
        if by.abs() == 1 {
            let colours: Vec<((u32, u32), Rgb)> = moving.iter().map(|&(x, y)| ((x, y), cells[at(x, y)])).collect();
            for &((x, y), _) in &colours {
                cells[at(x, y)] = tone;
            }
            for ((x, y), c) in colours {
                let moved = at((x as i64 + by) as u32, y);
                cells[moved] = c;
                hair[moved] = false;
            }
            *moving = moving.iter().map(|&(x, y)| ((x as i64 + by) as u32, y)).collect();
        }
    }
    // An eye lost on one side, where the other side is bare skin: mirrored over.
    for (from, to_side) in [(left.clone(), 4..=6u32), (right.clone(), 1..=3u32)] {
        if from.is_empty() || !find(cells, &hair, to_side).is_empty() {
            continue;
        }
        let bare = from.iter().all(|&(x, y)| cells[at(7 - x, y)] == tone && !hair[at(7 - x, y)]);
        if bare {
            for &(x, y) in &from {
                cells[at(7 - x, y)] = cells[at(x, y)];
            }
        }
    }
    // Skin around the eyes: hair beside them, and between them on their rows, cleared.
    let (left, right) = (find(cells, &hair, 1..=3), find(cells, &hair, 4..=6));
    let eye: Vec<(u32, u32)> = left.iter().chain(&right).copied().collect();
    let mut clear: Vec<(u32, u32)> = Vec::new();
    for &(x, y) in &eye {
        for nx in [x.wrapping_sub(1), x + 1] {
            if (1..=6).contains(&nx) {
                clear.push((nx, y));
            }
        }
    }
    if let (Some(l), Some(r)) = (span(&left), span(&right)) {
        for y in l.2.max(r.2)..=l.3.min(r.3) {
            for x in l.1 + 1..r.0 {
                clear.push((x, y));
            }
        }
    }
    for (x, y) in clear {
        let c = cells[at(x, y)];
        if !eye.contains(&(x, y)) && (hair[at(x, y)] || hair_like(head, c)) {
            cells[at(x, y)] = tone;
        }
    }
}

/// Clothes without the painting's speckle: a pixel takes the colour three of its four neighbours
/// agree on when it's only a shade off it, twice over. Lines and stripes a pixel wide (two
/// neighbours each way) stay, and so does a button, stud or stitch in another colour.
fn clothes(skin: &mut RgbaImage, parts: &[Part; 6]) {
    for part in parts.iter().skip(1) {
        for face in [Face::Front, Face::Right, Face::Back, Face::Left] {
            for _ in 0..2 {
                let grid = Grid::read(skin, *part, face);
                let (w, h) = (grid.w as i64, grid.h as i64);
                for y in 0..h {
                    for x in 0..w {
                        let around: Vec<Rgb> = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                            .iter()
                            .map(|(dx, dy)| (x + dx, y + dy))
                            .filter(|&(nx, ny)| nx >= 0 && ny >= 0 && nx < w && ny < h)
                            .map(|(nx, ny)| grid.at(nx as u32, ny as u32))
                            .collect();
                        let Some(&most) = around.iter().find(|n| around.iter().filter(|o| apart(**n, **o) <= 16).count() >= 3) else { continue };
                        let c = grid.at(x as u32, y as u32);
                        let off = apart(c, most);
                        if off > 10 && off <= 48 {
                            put(skin, *part, face, x as u32, y as u32, most);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HAIR: Rgb = [90, 55, 30];
    const SKIN: Rgb = [235, 190, 160];
    const GREEN: Rgb = [40, 150, 70];
    const LASH: Rgb = [25, 20, 20];
    const COAT: Rgb = [60, 60, 140];

    fn fill(skin: &mut RgbaImage, part: Part, face: Face, colour: &dyn Fn(u32, u32) -> Rgb) {
        let (x0, y0, w, h) = part.face(face);
        for y in 0..h {
            for x in 0..w {
                let c = colour(x, y);
                skin.put_pixel(x0 + x, y0 + y, Rgba([c[0], c[1], c[2], 255]));
            }
        }
    }

    fn at_part(skin: &RgbaImage, part: usize, face: Face, x: u32, y: u32) -> Rgb {
        let (x0, y0, _, _) = parts(SkinModel::Classic)[part].face(face);
        let p = skin.get_pixel(x0 + x, y0 + y).0;
        [p[0], p[1], p[2]]
    }

    fn at(skin: &RgbaImage, face: Face, x: u32, y: u32) -> Rgb {
        at_part(skin, HEAD, face, x, y)
    }

    /// Brown hair, skin-coloured hands, a blue coat, and a face painted by `front`.
    fn character(front: &dyn Fn(u32, u32) -> Rgb) -> RgbaImage {
        let parts = parts(SkinModel::Classic);
        let mut skin = RgbaImage::new(64, 64);
        for face in [Face::Top, Face::Bottom, Face::Right, Face::Left, Face::Back] {
            fill(&mut skin, parts[HEAD], face, &|x, y| if face == Face::Bottom || ((face == Face::Right || face == Face::Left) && y >= 4 && x >= 4) { SKIN } else { HAIR });
        }
        fill(&mut skin, parts[HEAD], Face::Front, front);
        for (index, part) in parts.iter().enumerate().skip(1) {
            for face in [Face::Top, Face::Bottom, Face::Right, Face::Front, Face::Left, Face::Back] {
                fill(&mut skin, *part, face, &|_, y| if (index == 2 || index == 3) && y >= 10 { SKIN } else { COAT });
            }
        }
        skin
    }

    fn row(skin: &RgbaImage, y: u32) -> Vec<Rgb> {
        (0..8).map(|x| at(skin, Face::Front, x, y)).collect()
    }

    #[test]
    fn a_face_keeps_its_painted_eyes_and_gets_one_skin_tone() {
        // Tall eyes, a lash over each, in shades the reading picked up; freckles and a second
        // skin tone; blush on the cheeks.
        let pink = [240, 140, 140];
        let mut skin = character(&|x, y| match (x, y) {
            (_, 0..=1) | (0 | 7, _) => HAIR,
            (2, 4) => LASH,
            (5, 4) => [40, 30, 30],
            (2 | 5, 5) => GREEN,
            (1 | 6, 6) => pink,
            (3, 2) | (4, 6) => [215, 165, 135],
            (6, 3) => [225, 175, 150],
            _ => SKIN,
        });
        tidy(&mut skin, SkinModel::Classic);
        for (x, y) in [(3, 2), (4, 6), (6, 3), (3, 4), (1, 4)] {
            assert_eq!(at(&skin, Face::Front, x, y), SKIN, "one skin tone at {x},{y}");
        }
        assert_eq!(at(&skin, Face::Front, 2, 5), GREEN, "the eyes stay where and as they were painted");
        assert_eq!(at(&skin, Face::Front, 5, 5), GREEN);
        assert_eq!(at(&skin, Face::Front, 2, 4), at(&skin, Face::Front, 5, 4), "alike lash shades merge");
        assert_ne!(at(&skin, Face::Front, 2, 4), SKIN);
        assert_eq!(at(&skin, Face::Front, 1, 6), pink, "blush stays");

        let lips = [215, 160, 150];
        let mut mouthed = character(&|x, y| match (x, y) {
            (_, 0..=1) | (0 | 7, _) => HAIR,
            (2 | 5, 4) => LASH,
            (3 | 4, 6) => lips,
            _ => SKIN,
        });
        tidy(&mut mouthed, SkinModel::Classic);
        assert_eq!(at(&mouthed, Face::Front, 3, 6), lips, "a faint mouth stays");
    }

    #[test]
    fn eyes_in_the_fringe_get_skin_around_them_and_line_up() {
        // Silver hair down to the eyes' row, and eyes a column apart from mirroring each other.
        let silver = [190, 195, 205];
        let dark = [15, 10, 25];
        let mut skin = character(&|x, y| match (x, y) {
            (0 | 7, _) | (_, 0..=1) => silver,
            (1 | 5, 2) => dark,
            (_, 2) => silver,
            _ => SKIN,
        });
        let parts = parts(SkinModel::Classic);
        for face in [Face::Top, Face::Right, Face::Left, Face::Back] {
            fill(&mut skin, parts[HEAD], face, &|x, y| if face != Face::Top && face != Face::Back && y >= 4 && x >= 4 { SKIN } else { silver });
        }
        tidy(&mut skin, SkinModel::Classic);
        let eyes = row(&skin, 2);
        assert_eq!(eyes[2], dark, "the outer eye moves in a column to mirror the other: {eyes:?}");
        assert_eq!(eyes[5], dark);
        for x in [1, 3, 4, 6] {
            assert_eq!(eyes[x], SKIN, "skin beside and between the eyes at {x}: {eyes:?}");
        }
    }

    #[test]
    fn an_eye_lost_on_bare_skin_is_mirrored_and_one_under_hair_stays_hidden() {
        let mut lost = character(&|x, y| match (x, y) {
            (_, 0..=1) | (0 | 7, _) => HAIR,
            (2, 4) => LASH,
            (2, 5) => GREEN,
            _ => SKIN,
        });
        tidy(&mut lost, SkinModel::Classic);
        assert_eq!(at(&lost, Face::Front, 5, 4), LASH);
        assert_eq!(at(&lost, Face::Front, 5, 5), GREEN);

        let gold = [195, 160, 55];
        let mut hidden = character(&|x, y| match (x, y) {
            (_, 0..=1) | (0 | 7, _) => HAIR,
            (4..=6, 2..=5) => HAIR,
            (2, 4) => gold,
            _ => SKIN,
        });
        tidy(&mut hidden, SkinModel::Classic);
        assert_eq!(at(&hidden, Face::Front, 2, 4), gold);
        assert_ne!(at(&hidden, Face::Front, 5, 4), gold, "the other eye stays under the hair");
        assert_ne!(at(&hidden, Face::Front, 5, 4), SKIN);
    }

    #[test]
    fn a_masked_head_stays_as_painted() {
        let wrap = |x: u32, y: u32| if (x == 2 || x == 5) && y == 4 { [250, 250, 250] } else if y.is_multiple_of(3) { [235, 60, 70] } else { [200, 20, 30] };
        let mut masked = character(&wrap);
        let parts = parts(SkinModel::Classic);
        for face in [Face::Top, Face::Right, Face::Left, Face::Back] {
            fill(&mut masked, parts[HEAD], face, &wrap);
        }
        let before = masked.clone();
        tidy(&mut masked, SkinModel::Classic);
        for y in 0..16 {
            for x in 0..64 {
                assert_eq!(masked.get_pixel(x, y), before.get_pixel(x, y), "{x},{y}");
            }
        }
    }

    #[test]
    fn hair_is_one_colour_with_its_own_highlights_and_shadows() {
        // Seven shades of brown and a lighter speck, in no order, and a strand broken by a stray
        // pixel.
        let shades: [Rgb; 7] = [[60, 35, 20], [75, 45, 25], [90, 55, 30], [105, 65, 35], [120, 75, 40], [140, 90, 50], [160, 110, 65]];
        let mut skin = character(&|x, y| if y < 2 || x == 0 || x == 7 { HAIR } else { SKIN });
        let parts = parts(SkinModel::Classic);
        fill(&mut skin, parts[HEAD], Face::Back, &|x, y| match (x, y) {
            (3, 4) => [160, 110, 65],
            (3, _) => [60, 35, 20],
            (6, 6) => [185, 135, 85],
            _ => shades[((x * 3 + y * 5) % 7) as usize],
        });
        tidy(&mut skin, SkinModel::Classic);
        let mut seen: Vec<Rgb> = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                let c = at(&skin, Face::Back, x, y);
                if !seen.contains(&c) {
                    seen.push(c);
                }
            }
        }
        assert!(seen.len() <= 3, "a colour, its highlight and its shadow: {seen:?}");
        // All of one hue: each channel in the same order.
        for c in &seen {
            assert!(c[0] > c[1] && c[1] > c[2], "{c:?}");
        }
        assert_eq!(at(&skin, Face::Back, 3, 4), at(&skin, Face::Back, 3, 3), "the strand runs on through the stray pixel");
    }

    #[test]
    fn hair_the_skins_colour_is_kept_apart_from_it() {
        let cream = [240, 208, 172];
        let mut skin = character(&|x, y| match (x, y) {
            (_, 0..=1) | (0 | 7, _) => cream,
            (2 | 5, 4) => LASH,
            _ => SKIN,
        });
        let parts = parts(SkinModel::Classic);
        for face in [Face::Top, Face::Right, Face::Left, Face::Back] {
            fill(&mut skin, parts[HEAD], face, &|x, y| if (face == Face::Right || face == Face::Left) && y >= 4 && x >= 4 { SKIN } else if (x + y) % 3 == 0 { [215, 172, 142] } else { cream });
        }
        tidy(&mut skin, SkinModel::Classic);
        let back = at(&skin, Face::Back, 4, 4);
        assert!(apart(back, SKIN) >= 30, "hair {back:?} against skin {SKIN:?}");
    }

    #[test]
    fn clothes_lose_their_speckle_but_keep_their_buttons() {
        let gold = [230, 190, 40];
        let mut skin = character(&|x, y| if y < 2 || x == 0 || x == 7 { HAIR } else { SKIN });
        let parts = parts(SkinModel::Classic);
        fill(&mut skin, parts[1], Face::Front, &|x, y| match (x, y) {
            (2, 3) => [80, 80, 165],
            (5, 6) => gold,
            _ => COAT,
        });
        tidy(&mut skin, SkinModel::Classic);
        assert_eq!(at_part(&skin, 1, Face::Front, 2, 3), at_part(&skin, 1, Face::Front, 1, 3), "a speck of shading goes");
        assert_eq!(at_part(&skin, 1, Face::Front, 5, 6), gold, "a button stays");
    }
}
