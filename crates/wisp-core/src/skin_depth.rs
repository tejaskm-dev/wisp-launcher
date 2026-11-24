//! Depth for flat skins. Every Minecraft skin has a second shell, the outer layer, that stands half
//! a pixel off the head and a quarter pixel off the body. A skin read from a picture comes out with
//! only the base layer, which looks flat in 3D; `add_depth` lifts the right pixels onto the outer
//! layer.
//!
//! What to lift comes from studying well-made skins: the most-used ones on NameMC and Mojang's own
//! defaults (November 2026). They lift edges, not whole areas: the fringe over the forehead, hair
//! strands with gaps between them, the ends of hair where it meets skin, cuffs and hems, the edges
//! of an open jacket, belts and shoes. Their outer layer covers a fifth to a half of a face and
//! almost never all of it, and the base under a lifted pixel is something else, such as skin under
//! a fringe or a darker shade under a cuff. Copying a whole area onto the outer layer only makes
//! the part a little bigger, which looks just as flat. So here a lifted pixel keeps its colour on
//! the outer layer and the base under it gets darker, or becomes skin under the fringe. The step
//! then shows from every angle but straight on.

use image::{Rgba, RgbaImage};

use crate::skins::SkinModel;
use crate::turnaround::{Face, OUTER, Part, parts};

pub(crate) type Rgb = [u8; 3];

pub(crate) const HEAD: usize = 0;
const BODY: usize = 1;
const ARMS: [usize; 2] = [2, 3];
const LEGS: [usize; 2] = [4, 5];
const SIDES: [Face; 4] = [Face::Front, Face::Right, Face::Back, Face::Left];
const ALL: [Face; 6] = [Face::Top, Face::Bottom, Face::Right, Face::Front, Face::Left, Face::Back];
/// Colours this close (the largest channel difference) count as the same.
const SAME: u8 = 36;
/// Further than this from the skin tone isn't skin.
const NOT_SKIN: u8 = 40;
/// Hair's own shading (highlights, undersides) stays within this of one of its colours.
const HAIR_SHADES: u8 = 60;

pub(crate) fn apart(a: Rgb, b: Rgb) -> u8 {
    (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

pub(crate) fn luma(c: Rgb) -> f32 {
    0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32
}

pub(crate) fn darker(c: Rgb) -> Rgb {
    c.map(|v| (v as f32 * 0.72) as u8)
}

/// A lifted colour, and the shade left on the base under it. Near-black can't get darker (and
/// lightening the lifted pixel greys it), so black stands out by its outline alone, as it does
/// on good skins.
fn shadow(c: Rgb) -> (Rgb, Rgb) {
    if luma(c) < 32.0 { (c, c) } else { (c, darker(c)) }
}

/// One face of a part, read off the skin: `w` by `h` colours, row by row.
pub(crate) struct Grid {
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) cells: Vec<Rgb>,
}

impl Grid {
    pub(crate) fn read(skin: &RgbaImage, part: Part, face: Face) -> Grid {
        let (x0, y0, w, h) = part.face(face);
        let cells = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let p = skin.get_pixel(x0 + x, y0 + y).0;
                [p[0], p[1], p[2]]
            })
            .collect();
        Grid { w, h, cells }
    }

    pub(crate) fn at(&self, x: u32, y: u32) -> Rgb {
        self.cells[(y * self.w + x) as usize]
    }

    fn row(&self, y: u32) -> Vec<Rgb> {
        (0..self.w).map(|x| self.at(x, y)).collect()
    }

    fn rows(&self, rows: std::ops::Range<u32>) -> Vec<Rgb> {
        rows.flat_map(|y| self.row(y)).collect()
    }

    /// The cells reachable from the seeds through cells that pass `keep` (4-connected).
    pub(crate) fn reach(&self, keep: &dyn Fn(u32, u32) -> bool, seeds: &[(u32, u32)]) -> Vec<bool> {
        let mut found = vec![false; self.cells.len()];
        let mut queue: Vec<(u32, u32)> = seeds.iter().copied().filter(|&(x, y)| keep(x, y)).collect();
        for &(x, y) in &queue {
            found[(y * self.w + x) as usize] = true;
        }
        while let Some((x, y)) = queue.pop() {
            let next = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
            for (nx, ny) in next {
                if nx < self.w && ny < self.h && !found[(ny * self.w + nx) as usize] && keep(nx, ny) {
                    found[(ny * self.w + nx) as usize] = true;
                    queue.push((nx, ny));
                }
            }
        }
        found
    }
}

/// The commonest colour among some (counting those within `SAME` of it), averaged, with its share.
pub(crate) fn commonest(colours: &[Rgb]) -> Option<(Rgb, f32)> {
    let (best, _) = colours
        .iter()
        .map(|c| (*c, colours.iter().filter(|o| apart(*c, **o) <= SAME).count()))
        .max_by_key(|(_, count)| *count)?;
    let group: Vec<&Rgb> = colours.iter().filter(|o| apart(best, **o) <= SAME).collect();
    let mut sum = [0u32; 3];
    for c in &group {
        for i in 0..3 {
            sum[i] += c[i] as u32;
        }
    }
    let n = group.len() as u32;
    Some(([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8], n as f32 / colours.len() as f32))
}

/// Colours grouped (each joins the first group within `SAME`), biggest group first.
fn palette(colours: &[Rgb]) -> Vec<Rgb> {
    // Each group: the colour it started from, the sum of its colours, and how many.
    let mut groups: Vec<(Rgb, [u32; 3], usize)> = Vec::new();
    for c in colours {
        let group = match groups.iter_mut().position(|(g, _, _)| apart(*g, *c) <= SAME) {
            Some(i) => i,
            None => {
                groups.push((*c, [0; 3], 0));
                groups.len() - 1
            }
        };
        for i in 0..3 {
            groups[group].1[i] += c[i] as u32;
        }
        groups[group].2 += 1;
    }
    groups.sort_by_key(|(_, _, count)| std::cmp::Reverse(*count));
    // Groups of one or two pixels are details, not a garment's colours. Each group stands for
    // all of it: its average, not whichever colour came first.
    let total = colours.len().max(1);
    groups
        .into_iter()
        .filter(|(_, _, count)| *count * 25 >= total || *count >= 3)
        .map(|(_, sum, count)| sum.map(|v| (v / count as u32) as u8))
        .collect()
}

fn near_any(c: Rgb, list: &[Rgb]) -> bool {
    list.iter().any(|o| apart(c, *o) <= SAME)
}

/// Whether a colour could be human skin (pale to dark, with some warmth).
pub(crate) fn skin_like(c: Rgb) -> bool {
    let (r, g, b) = (c[0] as f32, c[1] as f32, c[2] as f32);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max < 40.0 || max - min < 0.06 * max {
        return false;
    }
    let saturation = (max - min) / max;
    // Warm: red leads, blue trails, green between (hue roughly 0 to 50 degrees).
    r >= g && g >= b - 8.0 && saturation <= 0.75 && (r - b) > 12.0
}

/// The character's skin tone, and whether the face is covered (a mask, helmet, hood or wrap).
struct Tones {
    skin: Option<Rgb>,
    face_covered: bool,
}

fn read_tones(skin: &RgbaImage, parts: &[Part; 6]) -> Tones {
    let front = Grid::read(skin, parts[HEAD], Face::Front);
    let middle: Vec<Rgb> = (3..7).flat_map(|y| (1..7).map(move |x| (x, y))).map(|(x, y)| front.at(x, y)).collect();
    let face = commonest(&middle).filter(|(_, share)| *share >= 0.3).map(|(c, _)| c);
    let hands: Vec<Rgb> = ARMS
        .iter()
        .flat_map(|&arm| {
            let g = Grid::read(skin, parts[arm], Face::Front);
            g.rows(g.h - 2..g.h)
        })
        .collect();
    let hand = commonest(&hands).filter(|(_, share)| *share >= 0.4).map(|(c, _)| c);
    let back = commonest(&Grid::read(skin, parts[HEAD], Face::Back).cells).map(|(c, _)| c);
    // Skin showing over a quarter of the face's middle is a face, even with a long fringe and
    // big eyes taking more of it.
    let skinlike: Vec<Rgb> = middle.iter().copied().filter(|c| skin_like(*c)).collect();
    if let Some((tone, share)) = commonest(&skinlike)
        && share * skinlike.len() as f32 >= 0.25 * middle.len() as f32
    {
        return Tones { skin: Some(tone), face_covered: false };
    }
    match (face, hand) {
        (Some(face), _) if skin_like(face) => Tones { skin: Some(face), face_covered: false },
        // An unusual skin colour (a green orc, a grey robot) shows on the hands too, and not on
        // the back of the head. Black or a hood's colour all round is a mask and gloves.
        (Some(face), Some(hand)) if apart(face, hand) <= SAME && face.iter().any(|c| *c >= 70) && back.is_none_or(|back| apart(back, face) > SAME) => {
            Tones { skin: Some(face), face_covered: false }
        }
        (Some(_), Some(hand)) if skin_like(hand) => Tones { skin: Some(hand), face_covered: true },
        (Some(_), _) => Tones { skin: None, face_covered: true },
        (None, hand) => Tones { skin: hand.filter(|hand| skin_like(*hand)), face_covered: false },
    }
}

/// What the head shows: the skin tone, whether the face is covered, and the hair.
pub(crate) struct Head {
    pub(crate) skin: Option<Rgb>,
    /// A mask, helmet, hood or wrap over the face.
    pub(crate) covered: bool,
    /// The hair's colours, from the back of the head (the skin's own left out).
    pub(crate) colours: Vec<Rgb>,
    /// Hair the skin's colour (pale blond on pale skin), which colour alone can't tell apart, so
    /// where hair grows decides.
    pub(crate) by_place: bool,
    pub(crate) bald: bool,
}

impl Head {
    pub(crate) fn read(skin: &RgbaImage, parts: &[Part; 6]) -> Head {
        let tones = read_tones(skin, parts);
        let tone = tones.skin;
        // The back of the head shows the hair (or headwear).
        let back: Vec<Rgb> = Grid::read(skin, parts[HEAD], Face::Back).cells;
        let colours: Vec<Rgb> = palette(&back).into_iter().filter(|h| tone.is_none_or(|s| apart(*h, s) > 16)).collect();
        // How much of the top and back of the head is the skin's own colour: all of it on a bald
        // head (without the darker strands painted hair has), most of it when the hair is the
        // skin's colour.
        let skin_share = |face: Face| {
            let cells = Grid::read(skin, parts[HEAD], face).cells;
            let close = cells.iter().filter(|c| tone.is_some_and(|s| apart(**c, s) <= 16)).count();
            close as f32 / cells.len() as f32
        };
        let (top_skin, back_skin) = (skin_share(Face::Top), skin_share(Face::Back));
        let covered = tones.face_covered;
        Head {
            skin: tone,
            covered,
            by_place: !covered && (colours.is_empty() || back_skin >= 0.4),
            bald: !covered && top_skin >= 0.95 && back_skin >= 0.95,
            colours,
        }
    }

    /// Skin, telling hair of nearly the skin's colour apart by which it's nearer.
    pub(crate) fn is_skin(&self, c: Rgb) -> bool {
        self.skin.is_some_and(|s| {
            let to_skin = apart(c, s);
            to_skin <= 50 && self.colours.iter().all(|h| apart(c, *h) > to_skin)
        })
    }

    /// In the hair's colours, give or take its shading. Eyes and mouths aren't hair, and neither
    /// is what's painted on a mask.
    pub(crate) fn is_hair(&self, c: Rgb) -> bool {
        !self.is_skin(c) && (self.colours.is_empty() || self.colours.iter().any(|h| apart(c, *h) <= HAIR_SHADES))
    }

    /// Where the hair (or what covers the head) is on one of the head's faces.
    pub(crate) fn region(&self, grid: &Grid, face: Face) -> Vec<bool> {
        let (w, h) = (grid.w, grid.h);
        if self.bald || face == Face::Bottom {
            return vec![false; grid.cells.len()];
        }
        if self.covered {
            return grid.cells.iter().map(|c| self.is_hair(*c)).collect();
        }
        if self.by_place {
            // Where hair grows: the top and back, the upper sides and their back halves, the top
            // of the face and the locks beside it.
            return (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .map(|(x, y)| match face {
                    Face::Right => y < 3 || x < w / 2,
                    Face::Left => y < 3 || x >= w / 2,
                    Face::Front => y < 2 || ((x == 0 || x == w - 1) && y < 6),
                    _ => true,
                })
                .collect();
        }
        match face {
            Face::Right | Face::Left => {
                let seeds: Vec<(u32, u32)> = (0..w).map(|x| (x, 0)).collect();
                grid.reach(&|x, y| self.is_hair(grid.at(x, y)), &seeds)
            }
            Face::Front => {
                // Around the face only: the top rows, the outer columns, and the cheeks' edges
                // below the eyes. Under hair lighter than that, something near-black is an eye,
                // lashes or a shadow, not hair.
                let around = move |x: u32, y: u32| y < 3 || x == 0 || x == w - 1 || ((x == 1 || x == w - 2) && y >= 6);
                let light = self.colours.first().is_some_and(|c| luma(*c) >= 100.0);
                let hair = |c: Rgb| self.is_hair(c) && !(light && luma(c) < 60.0);
                let mut seeds: Vec<(u32, u32)> = (0..w).map(|x| (x, 0)).collect();
                seeds.extend((0..h).flat_map(|y| [(0, y), (w - 1, y)]));
                grid.reach(&|x, y| around(x, y) && hair(grid.at(x, y)), &seeds)
            }
            _ => grid.cells.iter().map(|c| self.is_hair(*c)).collect(),
        }
    }
}

/// What's left on the base under a lifted pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Under {
    /// A darker shade of it: the shadow under a strand, cuff, hem or shoe.
    Shadow,
    /// Skin: the forehead under a fringe.
    Skin(Rgb),
}

/// Which base pixels go up onto the outer layer, decided from the base as it is, then applied.
struct Plan<'a> {
    skin: &'a RgbaImage,
    parts: [Part; 6],
    /// By base pixel (`y * 64 + x`): the outer pixel it goes up to, and what's left under it.
    lifts: Vec<Option<(u32, u32, Under)>>,
    /// Base pixels darkened where they are: the gaps between strands of flat-painted hair.
    sunk: Vec<bool>,
}

impl Plan<'_> {
    fn grid(&self, part: usize, face: Face) -> Grid {
        Grid::read(self.skin, self.parts[part], face)
    }

    fn base_at(&self, part: usize, face: Face, x: u32, y: u32) -> usize {
        let (bx, by, _, _) = self.parts[part].face(face);
        ((by + y) * 64 + bx + x) as usize
    }

    fn lift(&mut self, part: usize, face: Face, x: u32, y: u32, under: Under) {
        let (ox, oy, _, _) = self.parts[part].outer(OUTER[part].0, OUTER[part].1).face(face);
        let at = self.base_at(part, face, x, y);
        if self.lifts[at].is_none() {
            self.lifts[at] = Some((ox + x, oy + y, under));
        }
    }

    /// Lifts the pixels of a face that `pick` picks, with a shadow under them.
    fn lift_where(&mut self, part: usize, face: Face, pick: &dyn Fn(&Grid, u32, u32) -> bool) {
        let grid = self.grid(part, face);
        for y in 0..grid.h {
            for x in 0..grid.w {
                if pick(&grid, x, y) {
                    self.lift(part, face, x, y, Under::Shadow);
                }
            }
        }
    }

    fn sink(&mut self, part: usize, face: Face, x: u32, y: u32) {
        let at = self.base_at(part, face, x, y);
        self.sunk[at] = true;
    }

    fn lifted(&self, part: usize, face: Face, x: u32, y: u32) -> bool {
        self.lifts[self.base_at(part, face, x, y)].is_some()
    }

    /// No lone floating pixels: one with nothing lifted beside it on its face stays down.
    fn tidy(&mut self) {
        for part in 0..self.parts.len() {
            for face in ALL {
                let (_, _, w, h) = self.parts[part].face(face);
                let mut lone = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        let beside = [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                            nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 && self.lifted(part, face, nx as u32, ny as u32)
                        });
                        if self.lifted(part, face, x, y) && !beside {
                            lone.push(self.base_at(part, face, x, y));
                        }
                    }
                }
                for at in lone {
                    self.lifts[at] = None;
                }
            }
        }
    }
}

/// Lifts the edges of a flat skin onto its outer layer (see the module notes). Outer pixels
/// already there stay as they are, and so does the base under them.
pub fn add_depth(skin: &mut RgbaImage, model: SkinModel) {
    if skin.dimensions() != (64, 64) {
        return;
    }
    let parts = parts(model);
    let (lifts, sunk) = {
        let head = Head::read(skin, &parts);
        let mut plan = Plan { skin: &*skin, parts, lifts: vec![None; 64 * 64], sunk: vec![false; 64 * 64] };
        hair(&mut plan, &head);
        let coat = clothes(&mut plan, head.skin);
        belt(&mut plan, head.skin);
        legs(&mut plan, head.skin, coat.as_deref());
        plan.tidy();
        (plan.lifts, plan.sunk)
    };
    apply(skin, &lifts, &sunk, &[]);
}

/// Depth from an outer layer the image tool painted (`painted`, by base pixel: the colour it put
/// on top there). The tool knows what sits on top, which rules can only guess: a hood, glasses,
/// headphones, horns, a jacket, boots. How much of it stands out follows good skins: hair (or a
/// wrap) by its edges, fringe and lighter strands, the rest of it staying on the base; anything
/// else whole. The base under a lifted pixel goes darker, or to skin under the fringe's tips.
/// With nothing painted, the outer layer is worked out from the base (`add_depth`).
pub fn add_painted_depth(skin: &mut RgbaImage, model: SkinModel, painted: &[Option<Rgb>]) {
    if skin.dimensions() != (64, 64) || painted.len() != 64 * 64 {
        return;
    }
    if painted.iter().all(Option::is_none) {
        add_depth(skin, model);
        return;
    }
    let parts = parts(model);
    let (lifts, sunk) = {
        let head = Head::read(skin, &parts);
        let mut plan = Plan { skin: &*skin, parts, lifts: vec![None; 64 * 64], sunk: vec![false; 64 * 64] };
        for (part, shape) in parts.iter().enumerate() {
            for face in ALL {
                let grid = plan.grid(part, face);
                let (bx, by, w, h) = shape.face(face);
                let marked: Vec<bool> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| painted[((by + y) * 64 + bx + x) as usize].is_some()).collect();
                if part == HEAD && face != Face::Top && face != Face::Bottom && !head.bald {
                    let hair: Vec<bool> = marked.iter().zip(&grid.cells).map(|(m, c)| *m && head.is_hair(*c)).collect();
                    let fringe = face == Face::Front && !head.covered;
                    let forehead = |x: u32, y: u32| y + 1 < grid.h && head.is_skin(grid.at(x, y + 1));
                    strands(&mut plan, HEAD, face, &grid, &hair, fringe, head.skin, &forehead);
                    for (i, (m, is_hair)) in marked.iter().zip(&hair).enumerate() {
                        if *m && !is_hair {
                            plan.lift(HEAD, face, i as u32 % w, i as u32 / w, Under::Shadow);
                        }
                    }
                } else {
                    for (i, m) in marked.iter().enumerate() {
                        if *m {
                            plan.lift(part, face, i as u32 % w, i as u32 / w, Under::Shadow);
                        }
                    }
                }
            }
        }
        plan.tidy();
        (plan.lifts, plan.sunk)
    };
    apply(skin, &lifts, &sunk, painted);
}

/// Writes a plan: each lifted pixel goes up (in the colour painted for it on top, when that's
/// clearly something else, like glasses drawn over eyes; otherwise its own), with what's left
/// under it; sunk pixels go darker. Outer pixels already there stay as they are, and so does the
/// base under them.
fn apply(skin: &mut RgbaImage, lifts: &[Option<(u32, u32, Under)>], sunk: &[bool], painted: &[Option<Rgb>]) {
    for (at, lift) in lifts.iter().enumerate() {
        let (bx, by) = (at as u32 % 64, at as u32 / 64);
        let base = skin.get_pixel(bx, by).0;
        let colour = [base[0], base[1], base[2]];
        if let Some((ox, oy, under)) = *lift {
            if skin.get_pixel(ox, oy).0[3] > 0 {
                continue;
            }
            let on_top = painted.get(at).copied().flatten().filter(|p| apart(*p, colour) > 60).unwrap_or(colour);
            let (up, down) = match under {
                Under::Shadow if on_top != colour => (on_top, colour),
                Under::Shadow => shadow(colour),
                Under::Skin(s) => (on_top, s),
            };
            skin.put_pixel(ox, oy, Rgba([up[0], up[1], up[2], 255]));
            skin.put_pixel(bx, by, Rgba([down[0], down[1], down[2], base[3]]));
        } else if sunk[at] {
            let d = darker(colour);
            skin.put_pixel(bx, by, Rgba([d[0], d[1], d[2], base[3]]));
        }
    }
}

/// Hair, or what covers the head (a hood, a wrap, a mask), lifted the way skin artists do it: the
/// fringe over the forehead with skin under its tips, the hair's edges where it meets skin, and
/// its lighter strands with gaps between them, on the front, sides and back. The top of the head
/// stays down. Long hair down the back or over the shoulders gets the same.
fn hair(plan: &mut Plan, head: &Head) {
    if head.bald {
        return;
    }
    for face in [Face::Front, Face::Right, Face::Left, Face::Back] {
        let grid = plan.grid(HEAD, face);
        let region = head.region(&grid, face);
        let fringe = face == Face::Front && !head.covered;
        let forehead = |x: u32, y: u32| y + 1 < grid.h && head.is_skin(grid.at(x, y + 1));
        strands(plan, HEAD, face, &grid, &region, fringe, head.skin, &forehead);
    }
    // Long hair down the back or over the shoulders, unless the body wears the hair's colour
    // (black hair over a black jacket can't be told from it).
    if head.colours.is_empty() || head.covered || head.by_place {
        return;
    }
    let body = plan.grid(BODY, Face::Front);
    if commonest(&body.cells).is_some_and(|(c, _)| head.is_hair(c)) {
        return;
    }
    for face in [Face::Back, Face::Front] {
        let grid = plan.grid(BODY, face);
        let w = grid.w;
        let lock = move |x: u32| face == Face::Back || x <= 1 || x + 2 >= w;
        let seeds: Vec<(u32, u32)> = (0..w).filter(|x| lock(*x)).map(|x| (x, 0)).collect();
        let region = grid.reach(&|x, y| lock(x) && head.is_hair(grid.at(x, y)), &seeds);
        strands(plan, BODY, face, &grid, &region, false, head.skin, &|_, _| false);
    }
}

/// Lifts one face's hair (`region`): its edges where it meets something else, a fringe's bottom
/// two pixels deep (with skin under its tips where `forehead` says the forehead is), its ends on
/// the bottom row, and its lighter strands. Hair painted flat has no strands to follow, so there
/// every third column stays down as a gap, darkened.
#[allow(clippy::too_many_arguments)]
fn strands(plan: &mut Plan, part: usize, face: Face, grid: &Grid, region: &[bool], fringe: bool, skin: Option<Rgb>, forehead: &dyn Fn(u32, u32) -> bool) {
    let (w, h) = (grid.w as i64, grid.h as i64);
    let inside = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h;
    let hairy = |x: i64, y: i64| inside(x, y) && region[(y * w + x) as usize];
    let mut lumas: Vec<f32> = (0..region.len()).filter(|i| region[*i]).map(|i| luma(grid.cells[i])).collect();
    if lumas.len() < 3 {
        return;
    }
    lumas.sort_by(f32::total_cmp);
    let quantile = |q: f32| lumas[((lumas.len() - 1) as f32 * q) as usize];
    let painted = quantile(0.9) - quantile(0.1) >= 10.0;
    // The lighter strands: what's lighter than most of the hair (hair shaded on a ramp is mostly
    // its middle shade), or else the lighter half (on hair painted in two shades, the lighter).
    let cut = lumas[lumas.len() / 2];
    let lighter = lumas.iter().filter(|l| **l > cut).count();
    let above = lighter * 100 >= lumas.len() * 15;
    for y in 0..h {
        for x in 0..w {
            if !hairy(x, y) {
                continue;
            }
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| inside(x + dx, y + dy) && !hairy(x + dx, y + dy));
            let ends = face != Face::Front && y == h - 1;
            let band = fringe && hairy(x, y + 1) && inside(x, y + 2) && !hairy(x, y + 2);
            let l = luma(grid.at(x as u32, y as u32));
            let strand = if painted { if above { l > cut } else { l >= cut } } else { x % 3 != 2 };
            let (x, y) = (x as u32, y as u32);
            if edge || ends || band || strand {
                let under = match skin {
                    Some(s) if fringe && forehead(x, y) => Under::Skin(s),
                    _ => Under::Shadow,
                };
                plan.lift(part, face, x, y, under);
            } else if !painted {
                plan.sink(part, face, x, y);
            }
        }
    }
}

/// Clothes, lifted at their edges: each sleeve's cuff (or hem) where it meets the hand or bare
/// arm, an open jacket's edges along the opening and its hem, and the hem of a long-sleeved top
/// over trousers of another colour. Returns the jacket's colours when there is one, since a long
/// coat reaches the legs.
fn clothes(plan: &mut Plan, skin: Option<Rgb>) -> Option<Vec<Rgb>> {
    let is_skin = move |c: Rgb| skin.is_some_and(|s| apart(c, s) <= NOT_SKIN);
    for arm in ARMS {
        for face in SIDES {
            let grid = plan.grid(arm, face);
            if let Some((top, bottom)) = cuff(&grid, &is_skin) {
                plan.lift_where(arm, face, &move |g, x, y| (top..=bottom).contains(&y) && !is_skin(g.at(x, y)));
            }
        }
    }
    // What the arms wear above the hands says whether the sleeves are long, and the outer
    // garment's colours (the back can show long hair or a print; a cuff is a detail).
    let mut sleeve = Vec::new();
    let mut arm_total = 0;
    for arm in ARMS {
        for face in SIDES {
            let grid = plan.grid(arm, face);
            for c in grid.rows(0..grid.h.saturating_sub(3)) {
                arm_total += 1;
                if !is_skin(c) {
                    sleeve.push(c);
                }
            }
        }
    }
    if sleeve.len() * 2 < arm_total {
        return None;
    }
    let garment: Vec<Rgb> = palette(&sleeve)
        .into_iter()
        .filter(|g| sleeve.iter().filter(|c| apart(**c, *g) <= SAME).count() * 100 >= sleeve.len() * 15)
        .collect();
    // The middle of the chest, in colours the sleeves don't wear: a shirt, vest or tie under a
    // jacket.
    let front = plan.grid(BODY, Face::Front);
    let centre: Vec<Rgb> = (1..10).flat_map(|y| (2..6).map(move |x| (x, y))).map(|(x, y)| front.at(x, y)).collect();
    let inner: Vec<Rgb> = centre.iter().copied().filter(|c| !is_skin(*c) && !near_any(*c, &garment)).collect();
    if inner.len() * 10 >= centre.len() {
        // An open jacket or coat: what shows through it (the shirt, or skin) is open, and the
        // jacket's edges beside it stand out, with its hem.
        let shirt = palette(&inner);
        let open = move |c: Rgb| is_skin(c) || near_any(c, &shirt);
        let opening = |g: &Grid, x: u32, y: u32| (x > 0 && open(g.at(x - 1, y))) || (x + 1 < g.w && open(g.at(x + 1, y)));
        plan.lift_where(BODY, Face::Front, &|g, x, y| !open(g.at(x, y)) && opening(g, x, y));
        for face in SIDES {
            plan.lift_where(BODY, face, &|g, x, y| y == g.h - 1 && !open(g.at(x, y)));
        }
        return Some(garment);
    }
    // A long-sleeved top over trousers of another colour: its hem.
    let legs: Vec<Rgb> = LEGS.iter().flat_map(|&leg| plan.grid(leg, Face::Front).row(0)).collect();
    let hem = commonest(&front.row(front.h - 1)).map(|(c, _)| c);
    if let (Some(hem), Some((trousers, _))) = (hem, commonest(&legs))
        && apart(hem, trousers) > SAME
        && !is_skin(hem)
    {
        for face in SIDES {
            plan.lift_where(BODY, face, &move |g, x, y| y == g.h - 1 && apart(g.at(x, y), hem) <= SAME);
        }
    }
    None
}

/// The rows of a sleeve's cuff on one face of an arm: the sleeve's last row above the hand or
/// bare arm (two rows when the cuff is painted in its own colour), or, with no hand showing, a
/// band of another colour at the bottom (a glove's edge or a cuff).
fn cuff(grid: &Grid, is_skin: &dyn Fn(Rgb) -> bool) -> Option<(u32, u32)> {
    let (w, h) = (grid.w as usize, grid.h);
    let mostly_skin = |y: u32| grid.row(y).iter().filter(|c| is_skin(**c)).count() * 2 > w;
    let row_colour = |y: u32| commonest(&grid.row(y)).map(|(c, _)| c);
    match (0..h).find(|&y| mostly_skin(y)) {
        // A bare arm.
        Some(0) => None,
        Some(end) => {
            let last = end - 1;
            let main = commonest(&grid.rows(0..last.max(1))).map(|(c, _)| c);
            let own = |y: u32| match (main, row_colour(y)) {
                (Some(m), Some(c)) => apart(m, c) > SAME,
                _ => false,
            };
            let deep = last >= 2 && own(last) && own(last - 1) && row_colour(last).zip(row_colour(last - 1)).is_some_and(|(a, b)| apart(a, b) <= SAME);
            Some((if deep { last - 1 } else { last }, last))
        }
        None => {
            let main = commonest(&grid.rows(0..h - 3)).map(|(c, _)| c)?;
            let band = (0..h).rev().take_while(|&y| row_colour(y).is_some_and(|c| apart(c, main) > SAME)).count() as u32;
            match band {
                1 | 2 => Some((h - band, h - 1)),
                3 => Some((h - 3, h - 3)),
                _ => None,
            }
        }
    }
}

/// A belt: a band of one or two rows low on the body, unlike what's above and below it.
fn belt(plan: &mut Plan, skin: Option<Rgb>) {
    let front = plan.grid(BODY, Face::Front);
    let row_colour = |y: u32| -> Option<Rgb> {
        let centre: Vec<Rgb> = (2..6).map(|x| front.at(x, y)).collect();
        commonest(&centre).filter(|(_, share)| *share >= 0.74).map(|(c, _)| c)
    };
    let differs = |a: Option<Rgb>, b: Option<Rgb>| match (a, b) {
        (Some(a), Some(b)) => apart(a, b) > 40,
        _ => true,
    };
    for top in 7..front.h {
        for rows in [1, 2] {
            let bottom = top + rows - 1;
            if bottom >= front.h {
                continue;
            }
            let Some(band) = row_colour(top) else { continue };
            if rows == 2 && differs(Some(band), row_colour(bottom)) {
                continue;
            }
            let below = if bottom + 1 < front.h { row_colour(bottom + 1) } else { None };
            let fits = differs(row_colour(top - 1), Some(band)) && (bottom + 1 == front.h || differs(below, Some(band)));
            if fits && !skin.is_some_and(|s| apart(band, s) <= NOT_SKIN) {
                for face in SIDES {
                    plan.lift_where(BODY, face, &move |g, x, y| (top..=bottom).contains(&y) && apart(g.at(x, y), band) <= SAME);
                }
                return;
            }
        }
    }
}

/// Shoes (a boot's top edge), and the hem of a long coat over the thighs.
fn legs(plan: &mut Plan, skin: Option<Rgb>, coat: Option<&[Rgb]>) {
    let is_skin = move |c: Rgb| skin.is_some_and(|s| apart(c, s) <= NOT_SKIN);
    for leg in LEGS {
        let front = plan.grid(leg, Face::Front);
        let row_colour = |y: u32| commonest(&front.row(y)).map(|(c, _)| c);
        let mut feet = 0;
        if let Some(foot) = row_colour(front.h - 1).filter(|c| !is_skin(*c)) {
            let mut rows = 1;
            while rows < 6 && row_colour(front.h - 1 - rows).is_some_and(|c| apart(c, foot) <= SAME) {
                rows += 1;
            }
            // Footwear ends somewhere (trousers reaching the ground don't count). Shoes stand out
            // whole; a tall boot by its top edge.
            if rows < 6 {
                feet = rows;
                let from = front.h - rows;
                let to = if rows <= 3 { front.h - 1 } else { from };
                for face in SIDES {
                    plan.lift_where(leg, face, &move |g, x, y| (from..=to).contains(&y) && apart(g.at(x, y), foot) <= SAME);
                }
            }
        }
        // A long coat over the thighs, if it ends on the leg (a suit's trousers match its jacket
        // all the way down, and aren't a coat): its hem.
        let ends = coat.is_some_and(|coat| {
            let main = |y: u32| commonest(&front.row(y)).filter(|(_, share)| *share >= 0.6).map(|(c, _)| c);
            let starts = (0..2).all(|y| main(y).is_some_and(|c| near_any(c, coat)));
            starts && (6..front.h - feet).any(|y| main(y).is_some_and(|c| !near_any(c, coat) && !is_skin(c)))
        });
        if let Some(coat) = coat.filter(|_| ends) {
            for face in SIDES {
                let grid = plan.grid(leg, face);
                let seeds: Vec<(u32, u32)> = (0..grid.w).map(|x| (x, 0)).collect();
                let tail = grid.reach(&|x, y| y < 8 && !is_skin(grid.at(x, y)) && near_any(grid.at(x, y), coat), &seeds);
                let w = grid.w;
                let hem = |x: u32, y: u32| tail[(y * w + x) as usize] && (y + 1 >= grid.h || !tail[((y + 1) * w + x) as usize]);
                plan.lift_where(leg, face, &|_, x, y| hem(x, y));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HAIR: Rgb = [70, 40, 20];
    const STRAND: Rgb = [120, 80, 45];
    const SKIN: Rgb = [235, 190, 160];
    const EYE: Rgb = [40, 120, 60];
    const JACKET: Rgb = [170, 30, 30];
    const SHIRT: Rgb = [240, 240, 240];
    const JEANS: Rgb = [70, 90, 150];
    const BOOT: Rgb = [30, 25, 25];
    const BELT: Rgb = [110, 70, 30];

    fn fill(skin: &mut RgbaImage, part: Part, face: Face, colour: &dyn Fn(u32, u32) -> Rgb) {
        let (x0, y0, w, h) = part.face(face);
        for y in 0..h {
            for x in 0..w {
                let c = colour(x, y);
                skin.put_pixel(x0 + x, y0 + y, Rgba([c[0], c[1], c[2], 255]));
            }
        }
    }

    /// Brown hair with lighter strands, a face with green eyes, a red jacket open over a white
    /// shirt, a belt, jeans and boots.
    fn dressed() -> RgbaImage {
        let parts = parts(SkinModel::Classic);
        let mut skin = RgbaImage::new(64, 64);
        let hair = |x: u32| if x.is_multiple_of(2) { STRAND } else { HAIR };
        for face in ALL {
            fill(&mut skin, parts[HEAD], face, &|x, y| if face == Face::Bottom || (face != Face::Top && face != Face::Back && y >= 4) { SKIN } else { hair(x) });
        }
        fill(&mut skin, parts[HEAD], Face::Front, &|x, y| match (x, y) {
            (_, 0..=1) | (0, _) | (7, _) => hair(x),
            (2 | 5, 4) => EYE,
            _ => SKIN,
        });
        for face in ALL {
            fill(&mut skin, parts[BODY], face, &|x, y| match (face, x, y) {
                (Face::Front, _, 10) => BELT,
                (Face::Front, 2..=5, _) => SHIRT,
                _ => JACKET,
            });
            for arm in ARMS {
                fill(&mut skin, parts[arm], face, &|_, y| if y >= 10 { SKIN } else { JACKET });
            }
            for leg in LEGS {
                fill(&mut skin, parts[leg], face, &|_, y| if y >= 9 { BOOT } else { JEANS });
            }
        }
        skin
    }

    fn outer_at(skin: &RgbaImage, part: usize, face: Face, x: u32, y: u32) -> Option<Rgb> {
        let base = parts(SkinModel::Classic)[part];
        let (ox, oy, _, _) = base.outer(OUTER[part].0, OUTER[part].1).face(face);
        let p = skin.get_pixel(ox + x, oy + y).0;
        (p[3] > 0).then_some([p[0], p[1], p[2]])
    }

    fn base_at(skin: &RgbaImage, part: usize, face: Face, x: u32, y: u32) -> Rgb {
        let (bx, by, _, _) = parts(SkinModel::Classic)[part].face(face);
        let p = skin.get_pixel(bx + x, by + y).0;
        [p[0], p[1], p[2]]
    }

    fn lifted_share(skin: &RgbaImage, part: usize, face: Face) -> f32 {
        let (_, _, w, h) = parts(SkinModel::Classic)[part].face(face);
        let lifted = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| outer_at(skin, part, face, x, y).is_some()).count();
        lifted as f32 / (w * h) as f32
    }

    #[test]
    fn the_fringe_stands_out_over_the_forehead_and_the_face_stays_flat() {
        let mut skin = dressed();
        add_depth(&mut skin, SkinModel::Classic);
        // The fringe's tips go up, with the forehead under them; the row above has a shadow.
        assert_eq!(outer_at(&skin, HEAD, Face::Front, 3, 1), Some(HAIR));
        assert_eq!(base_at(&skin, HEAD, Face::Front, 3, 1), SKIN);
        assert_eq!(outer_at(&skin, HEAD, Face::Front, 3, 0), Some(HAIR));
        assert_eq!(base_at(&skin, HEAD, Face::Front, 3, 0), darker(HAIR));
        // The locks beside the face, not the eyes or cheeks.
        assert!(outer_at(&skin, HEAD, Face::Front, 0, 3).is_some());
        assert_eq!(outer_at(&skin, HEAD, Face::Front, 2, 4), None, "eyes stay on the face");
        assert_eq!(outer_at(&skin, HEAD, Face::Front, 4, 5), None);
        assert_eq!(base_at(&skin, HEAD, Face::Front, 2, 4), EYE);
        // The top of the head stays down.
        assert_eq!(lifted_share(&skin, HEAD, Face::Top), 0.0);
    }

    #[test]
    fn hair_lifts_its_lighter_strands_and_leaves_gaps() {
        let mut skin = dressed();
        add_depth(&mut skin, SkinModel::Classic);
        // On the back: the light strands go up with a shadow under them, the dark ones stay down
        // between them (but for the ends on the bottom row).
        assert_eq!(outer_at(&skin, HEAD, Face::Back, 2, 3), Some(STRAND));
        assert_eq!(base_at(&skin, HEAD, Face::Back, 2, 3), darker(STRAND));
        assert_eq!(outer_at(&skin, HEAD, Face::Back, 3, 3), None);
        assert_eq!(base_at(&skin, HEAD, Face::Back, 3, 3), HAIR);
        assert!(outer_at(&skin, HEAD, Face::Back, 3, 7).is_some(), "the hair's ends");
        let share = lifted_share(&skin, HEAD, Face::Back);
        assert!((0.3..0.8).contains(&share), "partly covered, not all: {share}");
    }

    #[test]
    fn flat_painted_hair_gets_strands_with_darkened_gaps() {
        let parts = parts(SkinModel::Classic);
        let mut skin = dressed();
        fill(&mut skin, parts[HEAD], Face::Back, &|_, _| HAIR);
        add_depth(&mut skin, SkinModel::Classic);
        assert!(outer_at(&skin, HEAD, Face::Back, 0, 3).is_some());
        assert_eq!(outer_at(&skin, HEAD, Face::Back, 2, 3), None);
        assert_eq!(base_at(&skin, HEAD, Face::Back, 2, 3), darker(HAIR));
    }

    #[test]
    fn cuffs_and_an_open_jackets_edges_stand_out_while_the_rest_stays_flat() {
        let mut skin = dressed();
        add_depth(&mut skin, SkinModel::Classic);
        // The cuff above the hand, with a shadow under it; not the sleeve above or the hand.
        assert_eq!(outer_at(&skin, 2, Face::Front, 1, 9), Some(JACKET));
        assert_eq!(base_at(&skin, 2, Face::Front, 1, 9), darker(JACKET));
        assert_eq!(outer_at(&skin, 2, Face::Front, 1, 4), None);
        assert_eq!(outer_at(&skin, 2, Face::Front, 1, 11), None, "hands are skin");
        // The jacket's edges along the opening, and its hem; not its middle or the shirt.
        assert_eq!(outer_at(&skin, BODY, Face::Front, 1, 4), Some(JACKET));
        assert_eq!(outer_at(&skin, BODY, Face::Front, 0, 4), None);
        assert_eq!(outer_at(&skin, BODY, Face::Front, 3, 4), None, "the shirt stays under the jacket");
        assert_eq!(outer_at(&skin, BODY, Face::Back, 3, 11), Some(JACKET));
        assert_eq!(outer_at(&skin, BODY, Face::Back, 3, 4), None);
    }

    #[test]
    fn belts_and_shoes_stand_out_and_jeans_dont() {
        let mut skin = dressed();
        add_depth(&mut skin, SkinModel::Classic);
        assert_eq!(outer_at(&skin, BODY, Face::Front, 3, 10), Some(BELT));
        assert_eq!(base_at(&skin, BODY, Face::Front, 3, 10), darker(BELT));
        assert!(outer_at(&skin, 4, Face::Front, 1, 10).is_some());
        assert_eq!(outer_at(&skin, 4, Face::Front, 1, 4), None, "jeans stay flat");
    }

    #[test]
    fn a_wrapped_head_lifts_around_its_eyes_and_a_bald_one_not_at_all() {
        let parts = parts(SkinModel::Classic);
        let mut wrapped = dressed();
        let wrap = [200, 20, 30];
        let fold = [235, 60, 70];
        for face in ALL {
            fill(&mut wrapped, parts[HEAD], face, &|x, y| match (face, x, y) {
                (Face::Front, 2 | 5, 4) => [250, 250, 250],
                _ if y % 3 == 0 => fold,
                _ => wrap,
            });
        }
        add_depth(&mut wrapped, SkinModel::Classic);
        assert_eq!(outer_at(&wrapped, HEAD, Face::Front, 2, 4), None, "the eyes stay in the wrap");
        assert!(outer_at(&wrapped, HEAD, Face::Front, 1, 4).is_some(), "the wrap stands out around them");
        assert_eq!(outer_at(&wrapped, HEAD, Face::Back, 4, 3), Some(fold));

        let mut bald = dressed();
        for face in ALL {
            fill(&mut bald, parts[HEAD], face, &|_, _| SKIN);
        }
        add_depth(&mut bald, SkinModel::Classic);
        assert_eq!(lifted_share(&bald, HEAD, Face::Back), 0.0);
    }

    #[test]
    fn blond_hair_on_pale_skin_still_gets_a_fringe() {
        let parts = parts(SkinModel::Classic);
        let mut skin = dressed();
        let blond = [238, 200, 165];
        let strand = [200, 160, 120];
        for face in ALL {
            fill(&mut skin, parts[HEAD], face, &|x, y| match (face, x) {
                (Face::Back, 2) => strand,
                (Face::Top | Face::Back, _) => blond,
                _ if y < 2 => blond,
                _ => SKIN,
            });
        }
        fill(&mut skin, parts[HEAD], Face::Front, &|x, y| if (x, y) == (2, 4) || (x, y) == (5, 4) { EYE } else if y < 2 { blond } else { SKIN });
        add_depth(&mut skin, SkinModel::Classic);
        assert_eq!(outer_at(&skin, HEAD, Face::Front, 4, 1), Some(blond));
        assert_eq!(outer_at(&skin, HEAD, Face::Front, 2, 4), None, "eyes stay on the face");
        assert!(lifted_share(&skin, HEAD, Face::Back) > 0.2);
    }

    #[test]
    fn an_outer_layer_already_there_is_kept_with_its_base() {
        let mut skin = dressed();
        let parts = parts(SkinModel::Classic);
        let (ox, oy, _, _) = parts[HEAD].outer(OUTER[HEAD].0, OUTER[HEAD].1).face(Face::Front);
        skin.put_pixel(ox + 3, oy + 1, Rgba([1, 2, 3, 255]));
        add_depth(&mut skin, SkinModel::Classic);
        assert_eq!(outer_at(&skin, HEAD, Face::Front, 3, 1), Some([1, 2, 3]));
        assert_eq!(base_at(&skin, HEAD, Face::Front, 3, 1), HAIR);
    }

    #[test]
    fn short_sleeves_get_a_hem() {
        let parts = parts(SkinModel::Classic);
        let mut skin = dressed();
        for arm in ARMS {
            for face in ALL {
                fill(&mut skin, parts[arm], face, &|_, y| if y < 4 { SHIRT } else { SKIN });
            }
        }
        for face in ALL {
            fill(&mut skin, parts[BODY], face, &|_, _| SHIRT);
        }
        add_depth(&mut skin, SkinModel::Classic);
        assert_eq!(outer_at(&skin, 2, Face::Front, 1, 3), Some(SHIRT));
        assert_eq!(outer_at(&skin, 2, Face::Front, 1, 1), None);
        assert_eq!(outer_at(&skin, 2, Face::Front, 1, 6), None);
        assert!(skin_like(SKIN) && !skin_like([250, 250, 250]) && !skin_like([200, 20, 30]) && !skin_like(JEANS));
    }

    #[test]
    fn a_lone_speck_doesnt_float() {
        let parts = parts(SkinModel::Classic);
        let mut skin = dressed();
        for face in ALL {
            fill(&mut skin, parts[HEAD], face, &|x, y| if face == Face::Back && (x, y) == (4, 4) { HAIR } else { SKIN });
        }
        add_depth(&mut skin, SkinModel::Classic);
        assert_eq!(outer_at(&skin, HEAD, Face::Back, 4, 4), None);
    }
}
