//! Skins from a picture an image tool painted. Wisp hands the player a template (a blank Minecraft
//! character seen from the front, right, back and left, each grid square one skin pixel) and a
//! prompt for an image tool such as ChatGPT or Gemini; the tool paints the template as the
//! player's character; Wisp reads the painting back into a skin, square by square. The tool does
//! the artistry, the template keeps the layout exact, and nothing runs or is paid for on Wisp's
//! side.

use image::{Rgba, RgbaImage};

use crate::skins::SkinModel;

/// Template pixels per skin pixel.
const SCALE: u32 = 20;
/// The background: magenta, a colour characters almost never wear (white would swallow a white
/// suit's edges).
const BACKGROUND: [u8; 3] = [255, 0, 255];
const BODY: [u8; 3] = [178, 178, 178];
const GRID: [u8; 3] = [158, 158, 158];
const OUTLINE: [u8; 3] = [96, 96, 96];
const LABEL: [u8; 3] = [60, 0, 60];
/// The template's size in skin pixels: 3:2, the shape ChatGPT paints wide pictures in and Gemini
/// keeps, so neither has to rearrange the views to fit its canvas.
const WIDE: u32 = 66;
const HIGH: u32 = 44;
/// Space above the views and between them, in skin pixels (the labels go below).
const TOP: u32 = 5;
const GAP: u32 = 4;

/// What the player gives the image tool along with the template and their character. The layout
/// rules follow what makes these tools keep a layout (OpenAI's and Google's prompting guides, and
/// the BLOCK paper's character-to-skin prompts, 2026): the images named by number, "change only"
/// with everything to keep listed, the grid's real size in numbers, the exclusions said plainly,
/// and a check before finishing. How much detail to paint stays the original's: match the
/// character closely with careful shading (asking for few flat colours and simple clothes made
/// skins too plain, November 2026).
pub const PROMPT: &str = "You are a professional Minecraft skin artist. Image 1 is my character. Image 2 is a blank Minecraft skin template: the same blocky figure four times, seen from the front, from its right side, from the back and from its left side, drawn as grey blocks on a grid over a flat magenta background.

Repaint Image 2 as my character, like a professional, hand-made Minecraft skin. Change only the grey blocks, and keep everything else in Image 2 exactly as it is:
- The same canvas: the same size and 3:2 shape, and the same flat magenta background (#FF00FF) everywhere outside the blocks. No backdrop, gradient, glow, shadow, floor or scenery.
- The same four figures, in the same places and sizes, with the same blocky outlines. Paint nothing outside the grey blocks: hair, ears, horns, hats, hoods, capes, tails and weapons all stay inside them.
- The labels \"FRONT\", \"RIGHT\", \"BACK\" and \"LEFT\", unchanged.

Every small square of the grid is one pixel of the skin: the head is 8 by 8 squares, the body 8 wide and 12 tall, each arm 4 wide (3 on a slim figure) and 12 tall, each leg 4 wide and 12 tall. Paint each square one flat colour: crisp pixel art, no smooth gradients, no lines between squares.

Match my character's colours, hair, face, clothes and accessories as closely as you can, with careful shading: lighter tops, darker undersides, folds, hair strands and highlights. Keep the details that make my character theirs: patterns, trims, buttons, belts, jewellery, tears and patches.

FRONT shows the face, the chest, and the fronts of the arms and legs. RIGHT shows my character's right side, facing right: the front of the head is at that view's right edge. BACK shows the back of the head and body. LEFT shows the left side, facing left. All four are the same character, and anything that reaches the edge of one view carries on into the next.

- The face is only 8 by 8 squares, so paint it the way skin artists do, not as a shrunken drawing: eyes in my character's own style (dots, tall eyes, lashes or a white highlight), each made of whole squares and the same shape on both sides, with skin around them; a mouth or blush only if my character has them. Leave out anything smaller than a square; glasses are one row of frame squares.
- Where hair meets the face, and where a jacket, coat, belt or boots meet what is under them, use a clearly darker shade along the edge, so each reads as its own layer.

Before you finish, check each of these and fix any that fails: the background is still flat magenta; the four figures are in the same places and sizes, with nothing outside the blocks; every square is one flat colour, with no grid lines.

Reply with one image: the repainted template, with no text but the four labels.";

/// The follow-up, in the same chat, for the outer layer: the painting again with only what sits
/// on top kept and the rest painted out in the background's magenta (an edit of its own
/// picture, which these tools keep to the square). Wisp reads it with the first painting's views.
pub const OUTER_PROMPT: &str = "Now the outer layer. A Minecraft skin has a second layer that sits slightly above the first, for the parts that stand out. Edit the skin you just painted: keep only the parts of my character that sit on top as their own layer, exactly where and as you painted them, and paint every other square flat magenta (#FF00FF), the same as the background.

Keep: the hair and fringe, a hood, hat or helmet, ears, horns, glasses, headphones, a jacket or coat worn over something, a collar, cuffs, a belt, gloves, boots or shoes, armour, and accessories such as a scarf, a strap or jewellery.
Paint magenta: the face and eyes, bare skin, the shirt or top under a jacket, trousers, and anything else flat.

Keep the canvas, the four figures' places and sizes, and the labels exactly the same, and keep every square one flat colour. Reply with one image.";

/// A part's box on the skin: where its unfolded faces start, and its width, height and depth.
#[derive(Clone, Copy)]
pub(crate) struct Part {
    pub(crate) u: u32,
    pub(crate) v: u32,
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) d: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Face {
    Top,
    Bottom,
    Right,
    Front,
    Left,
    Back,
}

impl Part {
    /// x, y, width, height of a face on the skin.
    pub(crate) fn face(self, face: Face) -> (u32, u32, u32, u32) {
        let Part { u, v, w, h, d } = self;
        match face {
            Face::Top => (u + d, v, w, d),
            Face::Bottom => (u + d + w, v, w, d),
            Face::Right => (u, v + d, d, h),
            Face::Front => (u + d, v + d, w, h),
            Face::Left => (u + d + w, v + d, d, h),
            Face::Back => (u + 2 * d + w, v + d, w, h),
        }
    }

    pub(crate) fn outer(self, u: u32, v: u32) -> Part {
        Part { u, v, ..self }
    }
}

/// The six parts (head, body, right arm, left arm, right leg, left leg) for a build.
pub(crate) fn parts(model: SkinModel) -> [Part; 6] {
    let a = arm(model);
    let at = |u, v, w, h, d| Part { u, v, w, h, d };
    [at(0, 0, 8, 8, 8), at(16, 16, 8, 12, 4), at(40, 16, a, 12, 4), at(32, 48, a, 12, 4), at(0, 16, 4, 12, 4), at(16, 48, 4, 12, 4)]
}

/// Where each part's outer layer starts.
pub(crate) const OUTER: [(u32, u32); 6] = [(32, 0), (16, 32), (40, 32), (48, 48), (0, 32), (0, 48)];

fn arm(model: SkinModel) -> u32 {
    match model {
        SkinModel::Classic => 4,
        SkinModel::Slim => 3,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Front,
    Right,
    Back,
    Left,
}

const VIEWS: [View; 4] = [View::Front, View::Right, View::Back, View::Left];

/// A view's width in skin pixels.
fn view_width(view: View, model: SkinModel) -> u32 {
    match view {
        View::Front | View::Back => 2 * arm(model) + 8,
        View::Right | View::Left => 8,
    }
}

/// Which skin face each square of a view shows: (part, face, the face's pixel), or nothing.
/// Columns run left to right as the view is seen; rows from the top of the head.
fn square(view: View, model: SkinModel, column: u32, row: u32) -> Option<(usize, Face, u32, u32)> {
    let a = arm(model);
    match view {
        View::Front | View::Back => {
            let back = view == View::Back;
            // (part, column range start) by region; the back view swaps left and right.
            let (part, x0) = match row {
                0..8 if (a..a + 8).contains(&column) => (0, a),
                8..20 if (a..a + 8).contains(&column) => (1, a),
                8..20 if column < a => (if back { 3 } else { 2 }, 0),
                8..20 if column >= a + 8 && column < 2 * a + 8 => (if back { 2 } else { 3 }, a + 8),
                20..32 if (a..a + 4).contains(&column) => (if back { 5 } else { 4 }, a),
                20..32 if (a + 4..a + 8).contains(&column) => (if back { 4 } else { 5 }, a + 4),
                _ => return None,
            };
            let y0 = match part {
                0 => 0,
                1..=3 => 8,
                _ => 20,
            };
            Some((part, if back { Face::Back } else { Face::Front }, column - x0, row - y0))
        }
        View::Right | View::Left => {
            let right = view == View::Right;
            match row {
                0..8 if column < 8 => Some((0, if right { Face::Right } else { Face::Left }, column, row)),
                8..20 if (2..6).contains(&column) => Some((if right { 2 } else { 3 }, if right { Face::Right } else { Face::Left }, column - 2, row - 8)),
                20..32 if (2..6).contains(&column) => Some((if right { 4 } else { 5 }, if right { Face::Right } else { Face::Left }, column - 2, row - 20)),
                _ => None,
            }
        }
    }
}

/// Where each view starts across the template (centred), and the template's width and height,
/// in skin pixels.
fn layout(model: SkinModel) -> ([u32; 4], u32, u32) {
    let views = VIEWS.iter().map(|view| view_width(*view, model)).sum::<u32>() + 3 * GAP;
    let mut x = (WIDE - views) / 2;
    let mut starts = [0; 4];
    for (index, view) in VIEWS.iter().enumerate() {
        starts[index] = x;
        x += view_width(*view, model) + GAP;
    }
    (starts, WIDE, HIGH)
}

fn put(image: &mut RgbaImage, x: u32, y: u32, rgb: [u8; 3]) {
    if x < image.width() && y < image.height() {
        image.put_pixel(x, y, Rgba([rgb[0], rgb[1], rgb[2], 255]));
    }
}

/// Draws the four views of `paint`'s squares (or the blank grey figure) on the magenta ground.
fn draw(model: SkinModel, scale: u32, paint: &dyn Fn(usize, Face, u32, u32) -> Option<[u8; 3]>, labels: bool) -> RgbaImage {
    let (starts, width, height) = layout(model);
    let mut image = RgbaImage::from_pixel(width * scale, height * scale, Rgba([BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 255]));
    for (index, view) in VIEWS.iter().enumerate() {
        let x0 = starts[index];
        for row in 0..32 {
            for column in 0..view_width(*view, model) {
                let Some((part, face, fx, fy)) = square(*view, model, column, row) else { continue };
                let blank = paint(part, face, fx, fy).is_none();
                let colour = paint(part, face, fx, fy).unwrap_or(BODY);
                for dy in 0..scale {
                    for dx in 0..scale {
                        // The blank template shows its grid: a line along each square's top and left.
                        let line = blank && scale >= 4 && (dx == 0 || dy == 0);
                        put(&mut image, (x0 + column) * scale + dx, (TOP + row) * scale + dy, if line { GRID } else { colour });
                    }
                }
            }
        }
        if labels {
            // Each part outlined, so the blocks read as blocks.
            for row in 0..32 {
                for column in 0..view_width(*view, model) {
                    let here = square(*view, model, column, row).map(|(part, ..)| part);
                    if here.is_none() {
                        continue;
                    }
                    let edges = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)];
                    for (dx, dy) in edges {
                        let (nc, nr) = (column as i32 + dx, row as i32 + dy);
                        let other = if nc < 0 || nr < 0 { None } else { square(*view, model, nc as u32, nr as u32).map(|(part, ..)| part) };
                        if other == here {
                            continue;
                        }
                        let (px, py) = ((x0 + column) * scale, (TOP + row) * scale);
                        for t in 0..scale {
                            for thick in 0..2.min(scale) {
                                let (x, y) = match (dx, dy) {
                                    (-1, 0) => (px + thick, py + t),
                                    (1, 0) => (px + scale - 1 - thick, py + t),
                                    (0, -1) => (px + t, py + thick),
                                    _ => (px + t, py + scale - 1 - thick),
                                };
                                put(&mut image, x, y, OUTLINE);
                            }
                        }
                    }
                }
            }
            let name = match view {
                View::Front => "FRONT",
                View::Right => "RIGHT",
                View::Back => "BACK",
                View::Left => "LEFT",
            };
            let centre = (x0 * 2 + view_width(*view, model)) * scale / 2;
            text(&mut image, name, centre, (TOP + 33) * scale + scale / 2, (scale / 5).max(1));
        }
    }
    image
}

/// The template for a build, as a PNG-ready picture.
pub fn template(model: SkinModel) -> RgbaImage {
    draw(model, SCALE, &|_, _, _, _| None, true)
}

/// A skin drawn as the four views (as a tool would paint the template), for tests and checks.
pub fn render(skin: &RgbaImage, model: SkinModel, scale: u32) -> RgbaImage {
    let parts = parts(model);
    draw(
        model,
        scale,
        &|part, face, x, y| {
            let (fx, fy, _, _) = parts[part].face(face);
            let base = skin.get_pixel(fx + x, fy + y).0;
            let (ou, ov) = OUTER[part];
            let (ox, oy, _, _) = parts[part].outer(ou, ov).face(face);
            let over = skin.get_pixel(ox + x, oy + y).0;
            Some(if over[3] >= 128 { [over[0], over[1], over[2]] } else { [base[0], base[1], base[2]] })
        },
        false,
    )
}

/// A 5×7 letter, row by row.
fn glyph(letter: char) -> [u8; 7] {
    match letter {
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        _ => [0; 7],
    }
}

fn text(image: &mut RgbaImage, words: &str, centre: u32, top: u32, size: u32) {
    let advance = 6 * size;
    let width = words.len() as u32 * advance;
    let left = centre.saturating_sub(width / 2);
    for (index, letter) in words.chars().enumerate() {
        let rows = glyph(letter);
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    for dy in 0..size {
                        for dx in 0..size {
                            put(image, left + index as u32 * advance + column * size + dx, top + row as u32 * size + dy, LABEL);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TurnaroundError {
    #[error("Wisp couldn't find the character on a plain background. Ask the image tool to keep the template's magenta background.")]
    NoBackground,
    #[error("Wisp found {0} figures, not the template's four views (front, right, back, left). Ask the image tool to keep all four views, side by side.")]
    Views(usize),
}

/// How far apart two colours are (the largest channel difference).
fn apart(a: [u8; 3], b: [u8; 3]) -> u8 {
    (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

/// Reads a painted template back into a 64×64 skin (RGBA): each face from the view that shows
/// it, the hidden faces from their neighbours, the face and hair tidied the way skins draw them
/// (`skin_tidy`), and depth on the outer layer (`skin_depth`). The
/// views are found on a plain background when there is one; when the tool painted its own
/// background, from the template's layout (tools keep it, scaled to their canvas), each edge
/// moved to the strongest outline near it.
pub fn read(picture: &RgbaImage, model: SkinModel) -> Result<RgbaImage, TurnaroundError> {
    let mut skin = read_flat(picture, model)?;
    crate::skin_tidy::tidy(&mut skin, model);
    crate::skin_depth::add_depth(&mut skin, model);
    Ok(skin)
}

/// Reads the painting and its follow-up for the outer layer (the same picture with only what
/// sits on top kept, the rest painted out in magenta; see `OUTER_PROMPT`) into a skin: the base
/// from the painting, tidied, and the outer layer where the follow-up has it (`skin_depth`
/// decides how much of the hair stands out). The follow-up is read with the painting's views.
pub fn read_layers(picture: &RgbaImage, outer: &RgbaImage, model: SkinModel) -> Result<RgbaImage, TurnaroundError> {
    let located = locate(picture, model)?;
    let (mut skin, shifts) = base(&located, model);
    let painted = outer_layer(&located, &shifts, outer, model);
    crate::skin_tidy::tidy(&mut skin, model);
    crate::skin_depth::add_painted_depth(&mut skin, model, &painted);
    Ok(skin)
}

/// The painting read onto the base layer only, as painted (no tidying, no depth).
pub fn read_flat(picture: &RgbaImage, model: SkinModel) -> Result<RgbaImage, TurnaroundError> {
    let located = locate(picture, model)?;
    Ok(base(&located, model).0)
}

/// A painting's views, found: the picture at the size it's read at, each view's box, and which
/// of its pixels are figure rather than background.
struct Located {
    picture: RgbaImage,
    views: Vec<Placed>,
    mask: Vec<bool>,
}

fn locate(picture: &RgbaImage, model: SkinModel) -> Result<Located, TurnaroundError> {
    let picture = if picture.width() > 1600 {
        image::imageops::resize(picture, 1600, picture.height() * 1600 / picture.width(), image::imageops::FilterType::Triangle)
    } else {
        picture.clone()
    };
    let (width, height) = picture.dimensions();
    if width < 64 || height < 32 {
        return Err(TurnaroundError::Views(0));
    }
    let found = foreground(&picture).map(|figure| {
        on_background(&figure, width, height).map(|(mut views, mask)| {
            tighten(&mut views, &mask, (width, height), model);
            (views, mask)
        })
    });
    let (views, mask) = match found {
        // Views found on a background count only if they're shaped and placed like the
        // template's (a busy backdrop can pass for a plain one and split the figure wrongly).
        Some(Ok(found)) if fits_layout(&found.0, model) => found,
        Some(Ok(_)) if looks_like_a_painting(&picture, model) => (by_layout(&picture, model), vec![true; (width * height) as usize]),
        Some(Ok(found)) => found,
        // A painted background, or the views run together: the template's layout.
        Some(Err(TurnaroundError::Views(count))) if count > 4 => (by_layout(&picture, model), vec![true; (width * height) as usize]),
        Some(Err(error)) if !looks_like_a_painting(&picture, model) => return Err(error),
        None if !looks_like_a_painting(&picture, model) => return Err(TurnaroundError::NoBackground),
        _ => (by_layout(&picture, model), vec![true; (width * height) as usize]),
    };
    Ok(Located { picture, views, mask })
}

/// The base layer from the views' squares, and how far each view's head grid moved to keep the
/// face (`dx`, `dy` in squares, by view).
fn base(located: &Located, model: SkinModel) -> (RgbaImage, Vec<(f32, f32)>) {
    let Located { picture, views, mask } = located;
    let parts = parts(model);
    let mut skin = RgbaImage::new(64, 64);
    let mut seen = vec![false; 64 * 64];
    let mut shifts = vec![(0.0, 0.0); views.len()];
    for (index, (view, (left, top, right, bottom))) in views.iter().enumerate() {
        let columns = view_width(*view, model);
        let (cell_w, cell_h) = ((right - left) as f32 / columns as f32, (bottom - top) as f32 / 32.0);
        // Outlines up to a third of a square thick are strokes, not pixels.
        let stroke = (cell_w.min(cell_h) / 3.0).round() as u32;
        // The squares each part shows in this view: column, row, the face and its pixel.
        let mut regions: Vec<Vec<Square>> = vec![Vec::new(); 6];
        for row in 0..32 {
            for column in 0..columns {
                if let Some((part, face, fx, fy)) = square(*view, model, column, row) {
                    regions[part].push((column, row, face, fx, fy));
                }
            }
        }
        for (part, squares) in regions.iter().enumerate() {
            // Each square's colour with the grid moved by (dx, dy) squares.
            let sample = |dx: f32, dy: f32| -> Vec<Option<[u8; 3]>> {
                squares
                    .iter()
                    .map(|&(column, row, ..)| {
                        let x0 = (*left as f32 + (column as f32 + 0.15 + dx) * cell_w).max(0.0);
                        let y0 = (*top as f32 + (row as f32 + 0.15 + dy) * cell_h).max(0.0);
                        middle(picture, mask, (x0 as u32, y0 as u32, ((cell_w * 0.7) as u32).max(1), ((cell_h * 0.7) as u32).max(1)), stroke)
                    })
                    .collect()
            };
            // The head is where tools paint finer than the grid (a face's eyes as thin lines
            // between two rows). Its grid moves, by up to a third of a square, to where the face
            // read keeps the most contrast: an eye caught in one row rather than lost in two.
            let colours = if part == 0 {
                let mut best: Option<HeadRead> = None;
                for dy in -3..=3 {
                    for dx in -3..=3 {
                        let (dx, dy) = (dx as f32 / 10.0, dy as f32 / 10.0);
                        let colours = sample(dx, dy);
                        let score = contrast(squares, &colours) - 1000.0 * (dx.abs() + dy.abs());
                        if best.as_ref().is_none_or(|(most, ..)| score > *most) {
                            best = Some((score, dx, dy, colours));
                        }
                    }
                }
                let (_, dx, dy, mut colours) = best.unwrap_or((0.0, 0.0, 0.0, Vec::new()));
                shifts[index] = (dx, dy);
                if *view == View::Front {
                    let square_at = |column: u32, row: u32| {
                        let x0 = (*left as f32 + (column as f32 + 0.08 + dx) * cell_w).max(0.0);
                        let y0 = (*top as f32 + (row as f32 + 0.08 + dy) * cell_h).max(0.0);
                        (x0 as u32, y0 as u32, ((cell_w * 0.84) as u32).max(1), ((cell_h * 0.84) as u32).max(1))
                    };
                    keep_features(picture, mask, squares, &mut colours, &square_at);
                }
                colours
            } else {
                sample(0.0, 0.0)
            };
            for (&(_, _, face, fx, fy), colour) in squares.iter().zip(colours) {
                if let Some(colour) = colour {
                    let (sx, sy, _, _) = parts[part].face(face);
                    skin.put_pixel(sx + fx, sy + fy, Rgba([colour[0], colour[1], colour[2], 255]));
                    seen[((sy + fy) * 64 + sx + fx) as usize] = true;
                }
            }
        }
    }

    fill_hidden(&mut skin, &mut seen, &parts);
    (skin, shifts)
}

/// The outer layer a follow-up painting shows: by base pixel (`y * 64 + x`), the colour painted
/// on top there. Its views are where the painting's are (it's the same picture, edited; brought
/// to the same size when the tool returned another), and the sides no view shows take what's
/// beside them on the front and back, so a cuff or a jacket goes all the way round.
fn outer_layer(located: &Located, shifts: &[(f32, f32)], outer: &RgbaImage, model: SkinModel) -> Vec<Option<[u8; 3]>> {
    let (width, height) = located.picture.dimensions();
    let outer = if outer.dimensions() == (width, height) { outer.clone() } else { image::imageops::resize(outer, width, height, image::imageops::FilterType::Nearest) };
    // Painted is anything that isn't the magenta it was painted out in.
    let mask: Vec<bool> = outer.pixels().map(|p| apart([p.0[0], p.0[1], p.0[2]], BACKGROUND) > 60).collect();
    let parts = parts(model);
    let mut painted: Vec<Option<[u8; 3]>> = vec![None; 64 * 64];
    for (index, (view, (left, top, right, bottom))) in located.views.iter().enumerate() {
        let columns = view_width(*view, model);
        let (cell_w, cell_h) = ((right - left) as f32 / columns as f32, (bottom - top) as f32 / 32.0);
        let stroke = (cell_w.min(cell_h) / 3.0).round() as u32;
        for row in 0..32 {
            for column in 0..columns {
                let Some((part, face, fx, fy)) = square(*view, model, column, row) else { continue };
                let (dx, dy) = if part == 0 { shifts.get(index).copied().unwrap_or((0.0, 0.0)) } else { (0.0, 0.0) };
                let x0 = (*left as f32 + (column as f32 + 0.15 + dx) * cell_w).max(0.0);
                let y0 = (*top as f32 + (row as f32 + 0.15 + dy) * cell_h).max(0.0);
                if let Some(colour) = middle(&outer, &mask, (x0 as u32, y0 as u32, ((cell_w * 0.7) as u32).max(1), ((cell_h * 0.7) as u32).max(1)), stroke) {
                    let (sx, sy, _, _) = parts[part].face(face);
                    painted[((sy + fy) * 64 + sx + fx) as usize] = Some(colour);
                }
            }
        }
    }
    // The sides the views hide: the body's (under the arms) and each limb's inner side.
    for (part, face) in [(1, Face::Right), (1, Face::Left), (2, Face::Left), (3, Face::Right), (4, Face::Left), (5, Face::Right)] {
        let (fx, fy, fw, _) = parts[part].face(Face::Front);
        let (bx, by, bw, _) = parts[part].face(Face::Back);
        let (sx, sy, sw, sh) = parts[part].face(face);
        // A right side meets the front's first column and the back's last; a left side the
        // front's last and the back's first.
        let (front_x, back_x) = if face == Face::Right { (fx, bx + bw - 1) } else { (fx + fw - 1, bx) };
        for y in 0..sh {
            let front = painted[((fy + y) * 64 + front_x) as usize];
            let back = painted[((by + y) * 64 + back_x) as usize];
            for x in 0..sw {
                // The half nearer the front takes the front's, the other half the back's.
                let near_front = if face == Face::Right { x >= sw / 2 } else { x < sw / 2 };
                let colour = if near_front { front.or(back) } else { back.or(front) };
                if colour.is_some() {
                    painted[((sy + y) * 64 + sx + x) as usize] = colour;
                }
            }
        }
    }
    painted
}

/// Eyes and mouths that tools paint finer than the grid: a face square read as skin (its main
/// colour) gets the strongest colour unlike the skin inside it instead, when that covers enough
/// of it, so an eye a third of a square tall isn't lost. Where one feature spans two rows, the
/// row holding more of it keeps it: Minecraft eyes are one pixel tall.
fn keep_features(picture: &RgbaImage, mask: &[bool], squares: &[Square], colours: &mut [Option<[u8; 3]>], square_at: &dyn Fn(u32, u32) -> (u32, u32, u32, u32)) {
    let face: Vec<[u8; 3]> = squares
        .iter()
        .zip(colours.iter())
        .filter(|((.., fx, fy), _)| (4..=6).contains(fy) && (1..=6).contains(fx))
        .filter_map(|(_, colour)| *colour)
        .collect();
    let Some(skin) = crate::skin_depth::commonest(&face).filter(|(c, share)| *share >= 0.4 && crate::skin_depth::skin_like(*c)).map(|(c, _)| c) else {
        return;
    };
    let mut found: Vec<(usize, [u8; 3], f32)> = Vec::new();
    for (index, &(column, row, _, _, fy)) in squares.iter().enumerate() {
        let least = match fy {
            3..=5 => 0.10,
            2 | 6 => 0.16,
            _ => continue,
        };
        if colours[index].is_none_or(|c| apart(c, skin) > 40) {
            continue;
        }
        if let Some((colour, share)) = standout(picture, mask, square_at(column, row), skin)
            && share >= least
        {
            found.push((index, colour, share));
        }
    }
    for &(index, colour, share) in &found {
        let (_, _, _, fx, fy) = squares[index];
        // The same feature in the row above or below, holding more of it, keeps it instead.
        let stronger = found.iter().any(|&(other, other_colour, other_share)| {
            let (_, _, _, ox, oy) = squares[other];
            ox == fx && oy.abs_diff(fy) == 1 && apart(other_colour, colour) <= 48 && other_share > share
        });
        if !stronger {
            colours[index] = Some(colour);
        }
    }
}

/// The biggest group of colours far from the skin inside a square, and its share of the square.
fn standout(picture: &RgbaImage, mask: &[bool], (x0, y0, w, h): (u32, u32, u32, u32), skin: [u8; 3]) -> Option<([u8; 3], f32)> {
    let (width, height) = picture.dimensions();
    let mut total = 0u32;
    let mut bins: std::collections::HashMap<[u8; 3], (u32, [u32; 3])> = std::collections::HashMap::new();
    for y in y0..(y0 + h).min(height) {
        for x in x0..(x0 + w).min(width) {
            let p = picture.get_pixel(x, y).0;
            let c = [p[0], p[1], p[2]];
            if !mask[(y * width + x) as usize] || apart(c, BACKGROUND) <= 45 {
                continue;
            }
            total += 1;
            if apart(c, skin) <= 70 {
                continue;
            }
            // Coarse bins, so one feature's shades count together.
            let bin = bins.entry([c[0] >> 5, c[1] >> 5, c[2] >> 5]).or_insert((0, [0; 3]));
            bin.0 += 1;
            for i in 0..3 {
                bin.1[i] += c[i] as u32;
            }
        }
    }
    let (count, sum) = bins.values().max_by_key(|(count, _)| *count)?;
    (total > 0).then(|| ([(sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8], *count as f32 / total as f32))
}

/// The head as read with its grid moved: the read's contrast, the move (in squares), and each
/// square's colour.
type HeadRead = (f32, f32, f32, Vec<Option<[u8; 3]>>);

/// A square of a view: its column and row, and the face and the face's pixel it shows.
type Square = (u32, u32, Face, u32, u32);

/// How much a face read from squares changes from pixel to pixel: the sum of the differences
/// between neighbours.
fn contrast(squares: &[Square], colours: &[Option<[u8; 3]>]) -> f32 {
    let at: std::collections::HashMap<(u32, u32), [u8; 3]> = squares.iter().zip(colours).filter_map(|(&(_, _, _, fx, fy), colour)| colour.map(|colour| ((fx, fy), colour))).collect();
    let mut total = 0.0;
    for (&(x, y), colour) in &at {
        for next in [(x + 1, y), (x, y + 1)] {
            if let Some(other) = at.get(&next) {
                total += apart(*colour, *other) as f32;
            }
        }
    }
    total
}

/// A view and its figure's box: left, top, right, bottom.
type Placed = (View, (u32, u32, u32, u32));

/// The views on a plain background: runs of columns holding the figure (apart from small specks,
/// like the labels), each as tall as its own big pieces.
fn on_background(figure: &Figure, width: u32, height: u32) -> Result<(Vec<Placed>, Vec<bool>), TurnaroundError> {
    let tallest = figure.boxes.iter().map(|(_, _, _, h)| *h).max().unwrap_or(0);
    let mut columns = vec![false; width as usize];
    for &(x, _, w, h) in &figure.boxes {
        // Big pieces only: not the labels, nor a line (a view is at least a quarter as wide as tall).
        if h * 3 >= tallest && w * 16 >= h {
            for column in x..x + w {
                columns[column as usize] = true;
            }
        }
    }
    let least_gap = (tallest / 32).max(2) as usize;
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut empty = 0usize;
    for (x, &filled) in columns.iter().enumerate() {
        match (filled, start) {
            (true, None) => {
                start = Some(x);
                empty = 0;
            }
            (true, Some(_)) => empty = 0,
            (false, Some(from)) => {
                empty += 1;
                if empty >= least_gap {
                    runs.push((from as u32, (x - empty + 1) as u32));
                    start = None;
                }
            }
            (false, None) => {}
        }
    }
    if let Some(from) = start {
        runs.push((from as u32, (width as usize - empty) as u32));
    }
    let views: Vec<View> = match runs.len() {
        4 => VIEWS.to_vec(),
        2 => vec![View::Front, View::Back],
        count => return Err(TurnaroundError::Views(count)),
    };
    let mut placed = Vec::with_capacity(views.len());
    for (view, (left, right)) in views.into_iter().zip(runs) {
        let (mut top, mut bottom) = (height, 0);
        for &(x, y, w, h) in &figure.boxes {
            if h * 3 >= tallest && w * 16 >= h && x < right && x + w > left {
                top = top.min(y);
                bottom = bottom.max(y + h);
            }
        }
        if bottom > top {
            placed.push((view, (left, top, right, bottom)));
        }
    }
    Ok((placed, figure.mask.clone()))
}

/// Counts of the figure's pixels, for counting any rectangle of it at once.
struct Sums {
    width: u32,
    height: u32,
    sums: Vec<u32>,
}

impl Sums {
    fn new(mask: &[bool], width: u32, height: u32) -> Sums {
        let stride = (width + 1) as usize;
        let mut sums = vec![0u32; stride * (height + 1) as usize];
        for y in 0..height as usize {
            let mut row = 0;
            for x in 0..width as usize {
                row += mask[y * width as usize + x] as u32;
                sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
            }
        }
        Sums { width, height, sums }
    }

    /// How much of the rectangle x0..x1, y0..y1 is figure, 0 to 1.
    fn filled(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> f32 {
        let (x1, y1) = (x1.min(self.width), y1.min(self.height));
        if x1 <= x0 || y1 <= y0 {
            return 0.0;
        }
        let stride = (self.width + 1) as usize;
        let at = |x: u32, y: u32| self.sums[y as usize * stride + x as usize] as i64;
        let count = at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0);
        count as f32 / ((x1 - x0) * (y1 - y0)) as f32
    }
}

/// Each view's box brought in to the template's blocks. Tools sometimes paint hair, horns or a
/// cape past the blocks, and the figure's outline takes them in, which would stretch the grid.
/// Each edge moves in from the outline to the first line the figure all but fills along the part
/// there (the arms for the sides, or the head from the side; the head for the top; the legs for
/// the bottom), past anything only partly there.
fn tighten(views: &mut [Placed], mask: &[bool], (width, height): (u32, u32), model: SkinModel) {
    let sums = Sums::new(mask, width, height);
    let a = arm(model);
    for (view, edges) in views.iter_mut() {
        let columns = view_width(*view, model) as f32;
        let (head, legs, side_rows) = match view {
            View::Front | View::Back => ((a, a + 8), (a, a + 8), (8, 20)),
            View::Right | View::Left => ((0, 8), (2, 6), (0, 8)),
        };
        let (left, top, right, bottom) = *edges;
        // As far in as three squares (a cape can flare that far past a narrow side view).
        let reach = (bottom - top) * 3 / 32;
        let (reach_x, reach_y) = (reach.min((right - left) / 3), reach);
        // Twice: the second time with the parts placed by the first.
        for _ in 0..2 {
            let (l, t, r, b) = (edges.0 as f32, edges.1 as f32, edges.2 as f32, edges.3 as f32);
            // A run of squares, its ends trimmed (they may round off, or meet a neighbour).
            let band = |(from, to): (u32, u32), start: f32, unit: f32| {
                let (s, e) = (start + from as f32 * unit, start + to as f32 * unit);
                let pad = (e - s) * 0.15;
                ((s + pad) as u32, ((e - pad) as u32).max((s + pad) as u32 + 1))
            };
            let rows = band(side_rows, t, (b - t) / 32.0);
            let head = band(head, l, (r - l) / columns);
            let legs = band(legs, l, (r - l) / columns);
            let full = |share: f32| share >= 0.85;
            let new_left = (left..left + reach_x).find(|&x| full(sums.filled(x, rows.0, x + 1, rows.1)));
            let new_right = (right - reach_x..right).rev().find(|&x| full(sums.filled(x, rows.0, x + 1, rows.1))).map(|x| x + 1);
            let new_top = (top..top + reach_y).find(|&y| full(sums.filled(head.0, y, head.1, y + 1)));
            let new_bottom = (bottom - reach_y..bottom).rev().find(|&y| full(sums.filled(legs.0, y, legs.1, y + 1))).map(|y| y + 1);
            *edges = (new_left.unwrap_or(left), new_top.unwrap_or(top), new_right.unwrap_or(right), new_bottom.unwrap_or(bottom));
        }
    }
    // The views stand level, as on the template: each takes the views' middle top and bottom, so
    // hair heaped on one of them doesn't stretch it.
    let middle = |mut values: Vec<u32>| {
        values.sort_unstable();
        values[values.len() / 2]
    };
    if views.len() >= 3 {
        let top = middle(views.iter().map(|(_, edges)| edges.1).collect());
        let bottom = middle(views.iter().map(|(_, edges)| edges.3).collect());
        for (_, edges) in views.iter_mut() {
            (edges.1, edges.3) = (top, bottom);
        }
    }
}

/// Whether views found on a background are the template's: four (or front and back) whose
/// shapes match the figure's (a front twice as tall as wide, a side four times) and, for four,
/// standing level and spaced as on the template, wherever the tool put them on its canvas.
fn fits_layout(views: &[Placed], model: SkinModel) -> bool {
    let shaped = views.iter().all(|(view, (left, top, right, bottom))| {
        let expected = view_width(*view, model) as f32 / 32.0;
        let actual = (right - left) as f32 / (bottom - top).max(1) as f32;
        (actual / expected - 1.0).abs() < 0.15
    });
    if !shaped {
        return false;
    }
    if views.len() != 4 {
        return views.len() == 2;
    }
    let (starts, _, _) = layout(model);
    let (first, last) = (views[0].1, views[3].1);
    let span = (starts[3] + view_width(View::Left, model) - starts[0]) as f32;
    let unit_x = last.2.saturating_sub(first.0) as f32 / span;
    let unit_y = first.3.saturating_sub(first.1) as f32 / 32.0;
    views.iter().enumerate().all(|(index, (_, (left, top, _, bottom)))| {
        let expected = first.0 as f32 + (starts[index] - starts[0]) as f32 * unit_x;
        (*left as f32 - expected).abs() <= 1.5 * unit_x && top.abs_diff(first.1) as f32 <= 1.5 * unit_y && bottom.abs_diff(first.3) as f32 <= 1.5 * unit_y
    })
}

/// Whether a picture is shaped like the template (about as wide for its height), so a painted
/// background is worth reading by the layout.
fn looks_like_a_painting(picture: &RgbaImage, model: SkinModel) -> bool {
    let (_, width, height) = layout(model);
    let expected = width as f32 / height as f32;
    let actual = picture.width() as f32 / picture.height() as f32;
    (actual / expected - 1.0).abs() < 0.2
}

/// Brightness, for finding outlines.
fn luma(picture: &RgbaImage, x: u32, y: u32) -> f32 {
    let p = picture.get_pixel(x, y).0;
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// The views where the template put them, stretched to the picture, each edge then moved to the
/// strongest outline within a few per cent (tools keep the layout but not to the pixel).
fn by_layout(picture: &RgbaImage, model: SkinModel) -> Vec<Placed> {
    let (width, height) = picture.dimensions();
    let (starts, units_wide, units_high) = layout(model);
    let (sx, sy) = (width as f32 / (units_wide * SCALE) as f32, height as f32 / (units_high * SCALE) as f32);
    let reach_x = (width as f32 * 0.025) as i64;
    let reach_y = (height as f32 * 0.025) as i64;
    let a = arm(model);
    let mut placed = Vec::with_capacity(4);
    for (index, view) in VIEWS.iter().enumerate() {
        let columns = view_width(*view, model);
        let unit_x = SCALE as f32 * sx;
        let unit_y = SCALE as f32 * sy;
        let left = starts[index] as f32 * unit_x;
        let right = (starts[index] + columns) as f32 * unit_x;
        let top = TOP as f32 * unit_y;
        let bottom = (TOP + 32) as f32 * unit_y;
        // Where to look along each edge: the head's columns for the top, and the widest rows (the
        // arms, or the head from the side) for the sides.
        let (head, side_rows) = match view {
            View::Front | View::Back => ((a, a + 8), (8, 20)),
            View::Right | View::Left => ((0, 8), (0, 8)),
        };
        let across = |range: (u32, u32), shrink: f32| {
            let (from, to) = (left + range.0 as f32 * unit_x, left + range.1 as f32 * unit_x);
            let pad = (to - from) * shrink;
            ((from + pad) as u32, (to - pad).max(from + pad + 1.0) as u32)
        };
        let down = |range: (u32, u32)| {
            let (from, to) = (top + range.0 as f32 * unit_y, top + range.1 as f32 * unit_y);
            let pad = (to - from) * 0.15;
            ((from + pad) as u32, (to - pad).max(from + pad + 1.0) as u32)
        };
        let horizontal = |y: i64, (x0, x1): (u32, u32)| -> f32 {
            if y < 2 || y + 1 >= height as i64 {
                return 0.0;
            }
            let (y, total) = (y as u32, (x0..x1.min(width)).count().max(1) as f32);
            (x0..x1.min(width)).map(|x| (luma(picture, x, y) - luma(picture, x, y - 2)).abs()).sum::<f32>() / total
        };
        let vertical = |x: i64, (y0, y1): (u32, u32)| -> f32 {
            if x < 2 || x + 1 >= width as i64 {
                return 0.0;
            }
            let (x, total) = (x as u32, (y0..y1.min(height)).count().max(1) as f32);
            (y0..y1.min(height)).map(|y| (luma(picture, x, y) - luma(picture, x - 2, y)).abs()).sum::<f32>() / total
        };
        let best = |around: f32, reach: i64, score: &dyn Fn(i64) -> f32| -> f32 {
            let centre = around as i64;
            let (mut at, mut strongest) = (centre, 0.0);
            for candidate in centre - reach..=centre + reach {
                let strength = score(candidate);
                if strength > strongest * 1.05 {
                    (at, strongest) = (candidate, strength);
                }
            }
            // A faint outline isn't worth moving for.
            if strongest < 12.0 { around } else { at as f32 }
        };
        let new_top = best(top, reach_y, &|y| horizontal(y, across(head, 0.1)));
        // The feet keep the layout's line: dark shoes on a dark backdrop outline more weakly than
        // trousers meeting shoes, so snapping there would cut the legs short.
        let new_bottom = bottom;
        let new_left = best(left, reach_x, &|x| vertical(x, down(side_rows)));
        let new_right = best(right, reach_x, &|x| vertical(x + 1, down(side_rows)));
        placed.push((*view, (new_left.max(0.0) as u32, new_top.max(0.0) as u32, (new_right as u32).min(width), (new_bottom as u32).min(height))));
    }
    placed
}

struct Figure {
    mask: Vec<bool>,
    /// Each connected piece's bounding box: x, y, width, height.
    boxes: Vec<(u32, u32, u32, u32)>,
}

/// What isn't background: the background is the edges' commonest colour, flooded in from the
/// edges (so a colour like it inside the figure stays figure).
fn foreground(picture: &RgbaImage) -> Option<Figure> {
    let (width, height) = picture.dimensions();
    if width < 16 || height < 16 {
        return None;
    }
    let rgb = |x: u32, y: u32| {
        let p = picture.get_pixel(x, y).0;
        [p[0], p[1], p[2]]
    };
    // A band around the edge, a few pixels deep (a line a tool drew along the very edge isn't the
    // background).
    let band = (width.min(height) / 100).max(4);
    let edge: Vec<(u32, u32)> = (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).filter(|&(x, y)| x < band || y < band || x >= width - band || y >= height - band).collect();
    let mut buckets: std::collections::HashMap<[u8; 3], (u32, [u32; 3])> = std::collections::HashMap::new();
    for &(x, y) in &edge {
        let c = rgb(x, y);
        let entry = buckets.entry([c[0] >> 4, c[1] >> 4, c[2] >> 4]).or_insert((0, [0; 3]));
        entry.0 += 1;
        for i in 0..3 {
            entry.1[i] += c[i] as u32;
        }
    }
    let (count, sum) = buckets.values().max_by_key(|(count, _)| *count)?;
    let reference = [(sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8];
    let plainly = |x: u32, y: u32| picture.get_pixel(x, y).0[3] < 128 || apart(rgb(x, y), reference) <= 48;
    // Lines a tool drew across the background (its own grid) are background too: thin, with
    // background close on both sides.
    let reach = (width.max(height) / 400).max(2);
    // Past the picture's edge counts as background (a tool's grid can run along the edge).
    let beside = |x: i64, y: i64| x < 0 || y < 0 || x >= width as i64 || y >= height as i64 || plainly(x as u32, y as u32);
    let background = |x: u32, y: u32| {
        let (x, y, r) = (x as i64, y as i64, reach as i64);
        plainly(x as u32, y as u32) || (beside(x - r, y) && beside(x + r, y)) || (beside(x, y - r) && beside(x, y + r))
    };
    // A plain background is most of the border close to one colour (a painted backdrop isn't).
    let plain = edge.iter().filter(|&&(x, y)| background(x, y)).count();
    if plain * 10 < edge.len() * 8 {
        return None;
    }
    let mut mask = vec![true; (width * height) as usize];
    let mut stack: Vec<(u32, u32)> = edge.into_iter().filter(|&(x, y)| background(x, y)).collect();
    for &(x, y) in &stack {
        mask[(y * width + x) as usize] = false;
    }
    while let Some((x, y)) = stack.pop() {
        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
            if nx < width && ny < height && mask[(ny * width + nx) as usize] && background(nx, ny) {
                mask[(ny * width + nx) as usize] = false;
                stack.push((nx, ny));
            }
        }
    }
    // Connected pieces of figure.
    let mut label = vec![0u32; (width * height) as usize];
    let mut boxes = Vec::new();
    for start in 0..(width * height) {
        if !mask[start as usize] || label[start as usize] != 0 {
            continue;
        }
        let id = boxes.len() as u32 + 1;
        let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
        let mut stack = vec![start];
        label[start as usize] = id;
        while let Some(at) = stack.pop() {
            let (x, y) = (at % width, at / width);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < width && ny < height {
                    let next = ny * width + nx;
                    if mask[next as usize] && label[next as usize] == 0 {
                        label[next as usize] = id;
                        stack.push(next);
                    }
                }
            }
        }
        boxes.push((x0, y0, x1 - x0 + 1, y1 - y0 + 1));
    }
    Some(Figure { mask, boxes })
}

/// A square's colour: the commonest colour (within a little) among the figure's pixels in its
/// centre, as pixel-art downscaling picks it, so detail painted finer than a square gives that
/// square its main colour rather than a muddy mix. Seams thinner than `stroke` (the outlines
/// tools draw between pieces of clothing, which no skin pixel is) don't count; fine detail drawn
/// on one colour (a buckle, a chain) still does. None when it's mostly background.
fn middle(picture: &RgbaImage, mask: &[bool], (x0, y0, w, h): (u32, u32, u32, u32), stroke: u32) -> Option<[u8; 3]> {
    let (width, height) = picture.dimensions();
    let rgb = |x: u32, y: u32| {
        let p = picture.get_pixel(x, y).0;
        [p[0], p[1], p[2]]
    };
    // What's at a point: None off the picture, Some(None) off the figure, else its colour.
    let look = |x: i64, y: i64| -> Option<Option<[u8; 3]>> {
        (x >= 0 && y >= 0 && x < width as i64 && y < height as i64).then(|| mask[(y as u32 * width + x as u32) as usize].then(|| rgb(x as u32, y as u32)))
    };
    let unlike = |seen: Option<Option<[u8; 3]>>, colour: [u8; 3]| match seen {
        None => false,
        Some(None) => true,
        Some(Some(other)) => apart(other, colour) > 40,
    };
    // Two sides unlike each other: a seam between two pieces (or a piece and the background).
    let differ = |a: Option<Option<[u8; 3]>>, b: Option<Option<[u8; 3]>>| match (a, b) {
        (Some(Some(a)), Some(Some(b))) => apart(a, b) > 40,
        (Some(Some(_)), Some(None)) | (Some(None), Some(Some(_))) => true,
        _ => false,
    };
    let reach = stroke as i64;
    let mut pixels: Vec<([u8; 3], bool)> = Vec::with_capacity((w * h) as usize);
    let total = (y0..(y0 + h).min(height)).count() * (x0..(x0 + w).min(width)).count();
    for y in y0..(y0 + h).min(height) {
        for x in x0..(x0 + w).min(width) {
            // The template's magenta (and its blend into the figure's edge) is never a colour
            // the character wears.
            if mask[(y * width + x) as usize] && apart(rgb(x, y), BACKGROUND) > 45 {
                let colour = rgb(x, y);
                let (x, y) = (x as i64, y as i64);
                let (up, down, left, right) = (look(x, y - reach), look(x, y + reach), look(x - reach, y), look(x + reach, y));
                // A seam is thin one way between two different sides, and runs on the other way
                // (a small blob, like an eye, is kept).
                let across = unlike(up, colour) && unlike(down, colour) && differ(up, down) && !unlike(left, colour) && !unlike(right, colour);
                let down_it = unlike(left, colour) && unlike(right, colour) && differ(left, right) && !unlike(up, colour) && !unlike(down, colour);
                pixels.push((colour, reach > 0 && (across || down_it)));
            }
        }
    }
    if total == 0 || pixels.len() * 10 < total * 4 {
        return None;
    }
    // Without the strokes, unless that leaves too little (a square that's all fine lines).
    let solid = pixels.iter().filter(|(_, thin)| !thin).count();
    let keep_strokes = solid * 4 < pixels.len();
    // Colours in bins of 16 levels a channel, each bin keeping its pixels' sum.
    let mut bins: std::collections::HashMap<[u8; 3], (u32, [u32; 3])> = std::collections::HashMap::new();
    for (p, thin) in pixels {
        if thin && !keep_strokes {
            continue;
        }
        let bin = bins.entry([p[0] >> 4, p[1] >> 4, p[2] >> 4]).or_insert((0, [0; 3]));
        bin.0 += 1;
        for c in 0..3 {
            bin.1[c] += p[c] as u32;
        }
    }
    // The commonest bin, counting its neighbours too (a colour on a bin's edge splits between two).
    let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|c| a[c].abs_diff(b[c]) <= 1);
    let (best, _) = bins.keys().map(|key| (*key, bins.iter().filter(|(other, _)| near(*key, **other)).map(|(_, (count, _))| count).sum::<u32>())).max_by_key(|(_, weight)| *weight)?;
    let (count, sum) = bins.iter().filter(|(other, _)| near(best, **other)).fold((0u32, [0u32; 3]), |(count, sum), (_, (n, s))| (count + n, [sum[0] + s[0], sum[1] + s[1], sum[2] + s[2]]));
    Some([(sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8])
}

/// The faces no view shows (tops, bottoms, the sides that face each other) from their
/// neighbours, and any square a view missed from beside it, so the base layer is solid.
fn fill_hidden(skin: &mut RgbaImage, seen: &mut [bool], parts: &[Part; 6]) {
    let get = |skin: &RgbaImage, x: u32, y: u32| skin.get_pixel(x, y).0;
    for (index, part) in parts.iter().enumerate() {
        let (front_x, front_y, w, h) = part.face(Face::Front);
        let (back_x, back_y, _, _) = part.face(Face::Back);
        let d = part.d;
        // A side no view showed: its front half like the front's edge, its back half like the back's.
        for (face, front_column, back_column) in [(Face::Right, 0, w - 1), (Face::Left, w - 1, 0)] {
            let (sx, sy, _, _) = part.face(face);
            for y in 0..h {
                for x in 0..d {
                    let at = ((sy + y) * 64 + sx + x) as usize;
                    if seen[at] {
                        continue;
                    }
                    let toward_front = if face == Face::Right { x >= d / 2 } else { x < d / 2 };
                    let colour = if toward_front { get(skin, front_x + front_column, front_y + y) } else { get(skin, back_x + back_column, back_y + y) };
                    skin.put_pixel(sx + x, sy + y, Rgba(colour));
                    seen[at] = true;
                }
            }
        }
        // Top and bottom: the front's and back's first (or last) rows, meeting in the middle. The
        // game lays both with their last rows at the front, so under the head the chin is the
        // last rows, not the first. The colours stay as they are: the game shades undersides
        // itself, and real skins' soles, hands and chins match their edges, not a darker shade.
        for (face, row) in [(Face::Top, 0), (Face::Bottom, h - 1)] {
            // The head's top is its own (below).
            if index == 0 && face == Face::Top {
                continue;
            }
            let (sx, sy, _, _) = part.face(face);
            for y in 0..d {
                for x in 0..w {
                    let at = ((sy + y) * 64 + sx + x) as usize;
                    if seen[at] {
                        continue;
                    }
                    let colour = if y >= d / 2 { get(skin, front_x + x, front_y + row) } else { get(skin, back_x + w - 1 - x, back_y + row) };
                    skin.put_pixel(sx + x, sy + y, Rgba(colour));
                    seen[at] = true;
                }
            }
        }
    }
    // The top of the head, as good skins paint it: more of the hair (or the hood, the wrap, the
    // scalp), with the back of the head's texture carried over the crown and mirrored so the crown
    // sits in the middle; its front edge meets the front's top row (the hairline) and its sides
    // the sides' top rows, so a headband or a streak carries on round. Copying the front's top
    // row backwards would drag a fringe, ear tips or a hat brim across it in stripes.
    let head = parts[0];
    let (tx, ty, w, d) = head.face(Face::Top);
    if !seen[(ty * 64 + tx) as usize] {
        let (bx, by, _, _) = head.face(Face::Back);
        let (fx, fy, _, _) = head.face(Face::Front);
        let (rx, ry, _, _) = head.face(Face::Right);
        let (lx, ly, _, _) = head.face(Face::Left);
        for y in 0..d {
            for x in 0..w {
                let colour = if y == d - 1 {
                    get(skin, fx + x, fy)
                } else if x == 0 {
                    get(skin, rx + y, ry)
                } else if x == w - 1 {
                    get(skin, lx + d - 1 - y, ly)
                } else {
                    get(skin, bx + w - 1 - x, by + y.min(d - 1 - y))
                };
                skin.put_pixel(tx + x, ty + y, Rgba(colour));
                seen[((ty + y) * 64 + tx + x) as usize] = true;
            }
        }
    }
    // Anything still empty on the base layer takes the nearest filled pixel in its row.
    for part in parts {
        for face in [Face::Top, Face::Bottom, Face::Right, Face::Front, Face::Left, Face::Back] {
            let (fx, fy, fw, fh) = part.face(face);
            for y in fy..fy + fh {
                for x in fx..fx + fw {
                    if seen[(y * 64 + x) as usize] {
                        continue;
                    }
                    let near = (1..fw)
                        .flat_map(|step| [x.checked_sub(step), Some(x + step)])
                        .flatten()
                        .find(|&nx| nx >= fx && nx < fx + fw && seen[(y * 64 + nx) as usize]);
                    let colour = near.map(|nx| get(skin, nx, y)).unwrap_or([128, 128, 128, 255]);
                    skin.put_pixel(x, y, Rgba(colour));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A skin with a different colour for every face of every part, and a face on the front.
    fn sample_skin(model: SkinModel) -> RgbaImage {
        let mut skin = RgbaImage::new(64, 64);
        for (index, part) in parts(model).iter().enumerate() {
            for (n, face) in [Face::Top, Face::Bottom, Face::Right, Face::Front, Face::Left, Face::Back].into_iter().enumerate() {
                let (x0, y0, w, h) = part.face(face);
                for y in y0..y0 + h {
                    for x in x0..x0 + w {
                        // Distinct per part and face, varying across the face (to catch flips).
                        let r = 40 + index as u8 * 35;
                        let g = 30 + n as u8 * 35;
                        let b = 20 + ((x - x0) * 23 + (y - y0) * 7) as u8 % 200;
                        skin.put_pixel(x, y, Rgba([r, g, b, 255]));
                    }
                }
            }
        }
        skin
    }

    /// Whether one of the four views shows a part's face (the rest are filled in from neighbours).
    fn shown(part: usize, face: Face) -> bool {
        match face {
            Face::Front | Face::Back => true,
            Face::Right => matches!(part, 0 | 2 | 4),
            Face::Left => matches!(part, 0 | 3 | 5),
            _ => false,
        }
    }

    fn compare(made: &RgbaImage, original: &RgbaImage, model: SkinModel, tolerance: u8) -> (usize, usize) {
        let (mut matched, mut total) = (0, 0);
        for (index, part) in parts(model).into_iter().enumerate() {
            for face in [Face::Right, Face::Front, Face::Left, Face::Back] {
                if !shown(index, face) {
                    continue;
                }
                let (x0, y0, w, h) = part.face(face);
                for y in y0..y0 + h {
                    for x in x0..x0 + w {
                        let (a, b) = (made.get_pixel(x, y).0, original.get_pixel(x, y).0);
                        total += 1;
                        if apart([a[0], a[1], a[2]], [b[0], b[1], b[2]]) <= tolerance {
                            matched += 1;
                        }
                    }
                }
            }
        }
        (matched, total)
    }

    #[test]
    fn a_painted_template_reads_back_exactly() {
        for model in [SkinModel::Classic, SkinModel::Slim] {
            let skin = sample_skin(model);
            let painted = render(&skin, model, 16);
            let made = read_flat(&painted, model).unwrap();
            let (matched, total) = compare(&made, &skin, model, 0);
            assert_eq!(matched, total, "{model:?}: every shown face reads back exactly");
            // Every front and back pixel is exact.
            for part in parts(model) {
                for face in [Face::Front, Face::Back] {
                    let (x0, y0, w, h) = part.face(face);
                    for y in y0..y0 + h {
                        for x in x0..x0 + w {
                            assert_eq!(made.get_pixel(x, y), skin.get_pixel(x, y), "{model:?} {x},{y}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_resized_blurred_painting_still_reads_back() {
        let model = SkinModel::Classic;
        let skin = sample_skin(model);
        let painted = render(&skin, model, 24);
        // As a tool might hand it back: a different size, smoothed, offset by a border.
        let (w, h) = painted.dimensions();
        let resized = image::imageops::resize(&painted, w * 7 / 10, h * 7 / 10, image::imageops::FilterType::Triangle);
        let mut framed = RgbaImage::from_pixel(resized.width() + 60, resized.height() + 90, Rgba([250, 4, 250, 255]));
        image::imageops::overlay(&mut framed, &resized, 30, 50);
        let made = read_flat(&framed, model).unwrap();
        let (matched, total) = compare(&made, &skin, model, 24);
        assert!(matched as f32 >= total as f32 * 0.85, "{matched} of {total}");
    }

    #[test]
    fn a_painting_on_its_own_background_reads_by_the_layout() {
        let model = SkinModel::Classic;
        let skin = sample_skin(model);
        let painted = render(&skin, model, 20);
        // A tool that painted its own background (a dark gradient) and stretched the layout a
        // little to its canvas.
        let mut backdrop = painted.clone();
        for (x, y, pixel) in backdrop.enumerate_pixels_mut() {
            if pixel.0 == [255, 0, 255, 255] {
                let shade = (40 + (x / 9 + y / 7) % 60) as u8;
                *pixel = Rgba([shade, shade / 3, shade / 3, 255]);
            }
        }
        let (w, h) = backdrop.dimensions();
        let stretched = image::imageops::resize(&backdrop, w * 106 / 100, h * 104 / 100, image::imageops::FilterType::Nearest);
        let made = read_flat(&stretched, model).unwrap();
        let (matched, total) = compare(&made, &skin, model, 16);
        assert!(matched as f32 >= total as f32 * 0.9, "{matched} of {total}");
    }

    #[test]
    fn hair_horns_and_capes_past_the_blocks_are_left_out() {
        let model = SkinModel::Classic;
        let skin = sample_skin(model);
        let scale = 20;
        let mut painted = render(&skin, model, scale);
        let mut paint = |x0: u32, y0: u32, x1: u32, y1: u32, colour: [u8; 3]| {
            for y in y0..y1 {
                for x in x0..x1 {
                    painted.put_pixel(x, y, Rgba([colour[0], colour[1], colour[2], 255]));
                }
            }
        };
        let (starts, _, _) = layout(model);
        let a = arm(model);
        for (index, view) in VIEWS.iter().enumerate() {
            let (left, right) = (starts[index] * scale, (starts[index] + view_width(*view, model)) * scale);
            let (head_left, head_right) = match view {
                View::Front | View::Back => (left + a * scale, left + (a + 8) * scale),
                View::Right | View::Left => (left, right),
            };
            let top = TOP * scale;
            // Spiky hair over the head, and a horn beside it.
            for (n, x) in (head_left..head_right).step_by(scale as usize / 2).enumerate() {
                let spike = if n % 2 == 0 { scale * 2 / 3 } else { scale / 4 };
                paint(x, top - spike, x + scale / 2, top, [90, 60, 30]);
            }
            paint(head_left - scale / 2, top - scale, head_left, top + 2 * scale, [200, 200, 180]);
            // A ragged cape flaring out past the sides.
            for row in 12..28 {
                let (flare, y) = (scale * (row - 11) / 12, (TOP + row) * scale);
                paint(left - flare, y, left, y + scale, [120, 10, 20]);
                paint(right, y, right + flare, y + scale, [120, 10, 20]);
            }
        }
        // The hair's solid fringe moves the head's grid a quarter of a square; nothing else does.
        let made = read_flat(&painted, model).unwrap();
        let (matched, total) = compare(&made, &skin, model, 12);
        assert!(matched as f32 >= total as f32 * 0.97, "{matched} of {total}");
    }

    #[test]
    fn a_grid_drawn_over_the_background_is_still_background() {
        let model = SkinModel::Slim;
        let skin = sample_skin(model);
        let painted = render(&skin, model, 20);
        // Moved off the template's layout, so only finding the views on the background works.
        let mut framed = RgbaImage::from_pixel(painted.width() + 140, painted.height() + 40, Rgba([255, 0, 255, 255]));
        image::imageops::overlay(&mut framed, &painted, 120, 10);
        for (x, y, pixel) in framed.enumerate_pixels_mut() {
            if pixel.0 == [255, 0, 255, 255] && ((x + 13) % 27 < 3 || (y + 13) % 27 < 3) {
                *pixel = Rgba([150, 0, 150, 255]);
            }
        }
        let made = read_flat(&framed, model).unwrap();
        let (matched, total) = compare(&made, &skin, model, 0);
        assert_eq!(matched, total, "every shown face reads back exactly");
    }

    /// A character for the outer layer: brown hair with lighter strands, a face with eyes behind
    /// glasses, a red jacket open over a white shirt, jeans and black boots.
    fn layered() -> (RgbaImage, Vec<bool>) {
        let model = SkinModel::Classic;
        let parts = parts(model);
        let mut skin = RgbaImage::new(64, 64);
        let mut on_top = vec![false; 64 * 64];
        let (hair, strand, tone, eye, glasses) = ([90, 55, 30], [130, 85, 45], [235, 190, 160], [40, 110, 60], [60, 60, 70]);
        let (jacket, shirt, jeans, boot) = ([180, 30, 35], [240, 240, 240], [60, 80, 140], [25, 22, 22]);
        let faces = [Face::Top, Face::Bottom, Face::Right, Face::Front, Face::Left, Face::Back];
        for (index, part) in parts.iter().enumerate() {
            for face in faces {
                let (x0, y0, w, h) = part.face(face);
                for y in 0..h {
                    for x in 0..w {
                        let (colour, top) = match index {
                            0 => {
                                let hair_here = match face {
                                    Face::Front => y < 2 || x == 0 || x == w - 1,
                                    Face::Bottom => false,
                                    Face::Right | Face::Left => y < 4 || (face == Face::Right && x < 4) || (face == Face::Left && x >= 4),
                                    _ => true,
                                };
                                if hair_here {
                                    (if x % 2 == 0 { strand } else { hair }, true)
                                } else if face == Face::Front && y == 4 && (1..7).contains(&x) {
                                    (if x == 2 || x == 5 { eye } else { glasses }, x != 2 && x != 5)
                                } else {
                                    (tone, false)
                                }
                            }
                            1 if face == Face::Front && (2..6).contains(&x) => (shirt, false),
                            1 => (jacket, true),
                            2 | 3 if y >= 10 => (tone, false),
                            2 | 3 => (jacket, y == 9),
                            _ if y >= 10 => (boot, true),
                            _ => (jeans, false),
                        };
                        skin.put_pixel(x0 + x, y0 + y, Rgba([colour[0], colour[1], colour[2], 255]));
                        on_top[((y0 + y) * 64 + x0 + x) as usize] = top;
                    }
                }
            }
        }
        (skin, on_top)
    }

    fn outer_at(skin: &RgbaImage, part: usize, face: Face, x: u32, y: u32) -> Option<[u8; 3]> {
        let base = parts(SkinModel::Classic)[part];
        let (ox, oy, _, _) = base.outer(OUTER[part].0, OUTER[part].1).face(face);
        let p = skin.get_pixel(ox + x, oy + y).0;
        (p[3] > 0).then_some([p[0], p[1], p[2]])
    }

    #[test]
    fn a_follow_up_painting_of_what_sits_on_top_becomes_the_outer_layer() {
        let model = SkinModel::Classic;
        let (skin, on_top) = layered();
        let painting = render(&skin, model, 20);
        // The follow-up: the same painting with everything that doesn't sit on top painted out.
        let mut kept = skin.clone();
        for (index, top) in on_top.iter().enumerate() {
            let (x, y) = (index as u32 % 64, index as u32 / 64);
            if !top && !crate::skins::is_outer(x, y) {
                kept.put_pixel(x, y, Rgba([255, 0, 255, 255]));
            }
        }
        let follow_up = render(&kept, model, 20);
        let made = read_layers(&painting, &follow_up, model).unwrap();
        // Glasses stand out whole; the eyes behind them and the face don't.
        assert!(outer_at(&made, 0, Face::Front, 1, 4).is_some(), "glasses");
        assert!(outer_at(&made, 0, Face::Front, 3, 4).is_some(), "glasses");
        assert_eq!(outer_at(&made, 0, Face::Front, 2, 4), None, "an eye stays on the face");
        assert_eq!(outer_at(&made, 0, Face::Front, 4, 6), None, "the face stays flat");
        // The jacket stands out, the shirt and jeans don't; boots do.
        assert!(outer_at(&made, 1, Face::Front, 0, 5).is_some(), "the jacket");
        assert_eq!(outer_at(&made, 1, Face::Front, 3, 5), None, "the shirt under it");
        assert!(outer_at(&made, 4, Face::Front, 1, 11).is_some(), "boots");
        assert_eq!(outer_at(&made, 4, Face::Front, 1, 5), None, "jeans");
        // The cuff goes round the arm, the side no view shows too.
        assert!(outer_at(&made, 2, Face::Front, 1, 9).is_some(), "the cuff");
        assert!(outer_at(&made, 2, Face::Left, 1, 9).is_some(), "the cuff on the arm's hidden side");
        // Hair stands out by its strands, not all of it.
        let lifted = (0..8).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|&(x, y)| outer_at(&made, 0, Face::Back, x, y).is_some()).count();
        assert!((12..56).contains(&lifted), "hair on the back of the head partly lifted: {lifted} of 64");
        // Without the follow-up, the glasses aren't known to sit on top.
        let guessed = read(&painting, model).unwrap();
        assert_eq!(outer_at(&guessed, 0, Face::Front, 1, 4), None);
    }

    #[test]
    fn the_top_of_the_head_carries_the_back_over_the_crown() {
        let model = SkinModel::Classic;
        let parts = parts(model);
        let mut skin = RgbaImage::new(64, 64);
        let mut seen = vec![false; 64 * 64];
        let mut paint = |face: Face, colour: &dyn Fn(u32, u32) -> [u8; 3]| {
            let (x0, y0, w, h) = parts[0].face(face);
            for y in 0..h {
                for x in 0..w {
                    let c = colour(x, y);
                    skin.put_pixel(x0 + x, y0 + y, Rgba([c[0], c[1], c[2], 255]));
                    seen[((y0 + y) * 64 + x0 + x) as usize] = true;
                }
            }
        };
        // Hair on the back in rows of its own shade; a fringe with ear tips on the front's top row.
        paint(Face::Back, &|_, y| [20 * y as u8, 40, 60]);
        paint(Face::Front, &|x, y| if y == 0 { if x == 0 || x == 7 { [250, 150, 60] } else { [200, 180, 150] } } else { [240, 200, 170] });
        paint(Face::Right, &|_, _| [20, 40, 60]);
        paint(Face::Left, &|_, _| [20, 40, 60]);
        fill_hidden(&mut skin, &mut seen, &parts);
        let (tx, ty, _, _) = parts[0].face(Face::Top);
        let top = |x: u32, y: u32| {
            let p = skin.get_pixel(tx + x, ty + y).0;
            [p[0], p[1], p[2]]
        };
        assert_eq!(top(3, 0), [0, 40, 60], "the back edge meets the back's top row");
        assert_eq!(top(3, 3), [60, 40, 60], "towards the crown, the back's lower rows");
        assert_eq!(top(3, 4), [60, 40, 60], "mirrored past the crown");
        assert_eq!(top(3, 7), [200, 180, 150], "the front edge meets the hairline");
        assert_eq!(top(3, 5), [40, 40, 60], "an ear tip on the front's top row doesn't streak back over the crown");
    }

    #[test]
    fn undersides_meet_the_front_with_their_last_rows() {
        let model = SkinModel::Classic;
        let parts = parts(model);
        let mut skin = RgbaImage::new(64, 64);
        let mut seen = vec![false; 64 * 64];
        let mut paint = |part: usize, face: Face, colour: [u8; 3]| {
            let (x0, y0, w, h) = parts[part].face(face);
            for y in 0..h {
                for x in 0..w {
                    skin.put_pixel(x0 + x, y0 + y, Rgba([colour[0], colour[1], colour[2], 255]));
                    seen[((y0 + y) * 64 + x0 + x) as usize] = true;
                }
            }
        };
        // A face over a back of hair; a boot with a toe cap in front and a heel behind.
        let (skin_tone, hair, toe, heel) = ([240, 200, 170], [60, 40, 30], [200, 40, 40], [30, 30, 90]);
        paint(0, Face::Front, skin_tone);
        paint(0, Face::Back, hair);
        paint(4, Face::Front, toe);
        paint(4, Face::Back, heel);
        fill_hidden(&mut skin, &mut seen, &parts);
        let at = |part: usize, x: u32, y: u32| {
            let (x0, y0, _, _) = parts[part].face(Face::Bottom);
            let p = skin.get_pixel(x0 + x, y0 + y).0;
            [p[0], p[1], p[2]]
        };
        assert_eq!(at(0, 3, 7), skin_tone, "under the chin is skin, as it is, not darkened");
        assert_eq!(at(0, 3, 0), hair, "under the back of the head is hair");
        assert_eq!(at(4, 1, 3), toe, "the sole's front is the toe");
        assert_eq!(at(4, 1, 0), heel, "the sole's back is the heel");
    }

    #[test]
    fn the_template_is_three_by_two() {
        for model in [SkinModel::Classic, SkinModel::Slim] {
            let template = template(model);
            assert_eq!(template.width() * 2, template.height() * 3, "{model:?}");
        }
    }

    #[test]
    fn the_template_has_four_views_on_magenta() {
        let template = template(SkinModel::Classic);
        assert_eq!(template.get_pixel(1, 1).0, [255, 0, 255, 255]);
        let figure = foreground(&template).unwrap();
        // The four figures, and the label letters.
        assert!(figure.boxes.iter().filter(|(_, _, _, h)| *h > SCALE * 30).count() == 4);
        // A blank template reads as a plain grey figure, every face filled.
        let read_back = read(&template, SkinModel::Classic).unwrap();
        assert_eq!(read_back.get_pixel(8, 8).0[3], 255);
    }

    #[test]
    fn pictures_without_the_views_are_explained() {
        let plain = RgbaImage::from_pixel(200, 200, Rgba([255, 0, 255, 255]));
        assert_eq!(read(&plain, SkinModel::Classic).unwrap_err(), TurnaroundError::Views(0));
    }
}
