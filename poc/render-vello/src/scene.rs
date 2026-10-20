//! Backend-neutral description of the test scene: 10,000 rounded rectangles
//! with a stroke and a linear gradient, plus one line of text per size.
//!
//! The same description feeds the GPU renderer (`vello`) and the CPU
//! renderer (`vello_cpu`), so both draw exactly the same content.

use vello::kurbo::{Point, Rect, RoundedRect};
use vello::peniko::Color;

pub const RECT_COUNT: usize = 10_000;
const COLUMNS: usize = 100;
const CELL: f64 = 48.0;
const TEXT_TOP: f64 = -760.0;
const TEXT_GAP: f64 = 12.0;

/// Text sizes in world units (pixels at 100% zoom).
pub const TEXT_SIZES: [f32; 10] = [8.0, 10.0, 12.0, 14.0, 18.0, 24.0, 32.0, 48.0, 64.0, 96.0];

pub const BACKGROUND: Color = Color::from_rgb8(0x1b, 0x1b, 0x22);
pub const TEXT_COLOR: Color = Color::from_rgb8(0xf2, 0xf0, 0xfa);

#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub shape: RoundedRect,
    pub gradient_start: Point,
    pub gradient_end: Point,
    pub colors: [Color; 2],
    pub stroke: Color,
    pub stroke_width: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub size: f32,
    /// Left end of the baseline.
    pub origin: Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneDesc {
    pub tiles: Vec<Tile>,
    pub lines: Vec<TextLine>,
}

impl SceneDesc {
    /// Builds the fixed test scene. The same call always returns the same scene.
    pub fn test_scene() -> Self {
        let mut rng = Lcg(0x5EED_B1FA);
        let tiles = (0..RECT_COUNT).map(|i| tile(i, &mut rng)).collect();

        let mut y = TEXT_TOP;
        let lines = TEXT_SIZES
            .iter()
            .map(|&size| {
                y += f64::from(size) + TEXT_GAP;
                TextLine {
                    text: format!("Bivar {size} px — Source Sans 3 OTF: AaBbGgQq 0123 ção"),
                    size,
                    origin: Point::new(0.0, y),
                }
            })
            .collect();
        Self { tiles, lines }
    }

    /// Makes the tiles share `distinct` gradients, cycling through the colors of
    /// the first `distinct` tiles. Every tile still has its own gradient line.
    ///
    /// Vello 0.11 keeps only 64 gradients between frames, so this measures
    /// how speed depends on the number of different gradients.
    pub fn with_distinct_gradients(mut self, distinct: usize) -> Self {
        let distinct = distinct.clamp(1, self.tiles.len());
        let palette: Vec<[Color; 2]> = self.tiles[..distinct].iter().map(|t| t.colors).collect();
        for (i, tile) in self.tiles.iter_mut().enumerate() {
            tile.colors = palette[i % distinct];
        }
        self
    }

    /// Area covered by the scene, in world units.
    pub fn bounds() -> Rect {
        let side = COLUMNS as f64 * CELL;
        let rows = RECT_COUNT.div_ceil(COLUMNS) as f64;
        Rect::new(0.0, TEXT_TOP, side, rows * CELL)
    }
}

fn tile(i: usize, rng: &mut Lcg) -> Tile {
    let origin = Point::new((i % COLUMNS) as f64 * CELL, (i / COLUMNS) as f64 * CELL);
    let width = 24.0 + rng.unit() * 20.0;
    let height = 24.0 + rng.unit() * 20.0;
    let x0 = origin.x + (CELL - width) / 2.0;
    let y0 = origin.y + (CELL - height) / 2.0;
    let radius = 2.0 + rng.unit() * 10.0;
    let shape = RoundedRect::new(x0, y0, x0 + width, y0 + height, radius);
    Tile {
        shape,
        gradient_start: Point::new(x0, y0),
        gradient_end: Point::new(x0 + width, y0 + height),
        colors: [rng.color(), rng.color()],
        stroke: rng.color(),
        stroke_width: 1.0 + rng.unit() * 2.0,
    }
}

/// Small deterministic pseudo-random generator, so the scene never changes.
struct Lcg(u64);

impl Lcg {
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }

    /// A number in `0.0..1.0`.
    fn unit(&mut self) -> f64 {
        f64::from(self.next_u32()) / f64::from(1u32 << 31)
    }

    fn color(&mut self) -> Color {
        let [r, g, b, _] = self.next_u32().to_le_bytes();
        Color::from_rgb8(r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_has_ten_thousand_rects() {
        assert_eq!(SceneDesc::test_scene().tiles.len(), 10_000);
    }

    #[test]
    fn scene_is_deterministic() {
        assert_eq!(SceneDesc::test_scene(), SceneDesc::test_scene());
    }

    #[test]
    fn every_rect_is_rounded_stroked_and_has_a_gradient() {
        for tile in SceneDesc::test_scene().tiles {
            assert!(tile.shape.radii().top_left >= 2.0);
            assert!(tile.stroke_width >= 1.0);
            assert_ne!(tile.gradient_start, tile.gradient_end);
        }
    }

    #[test]
    fn everything_lies_inside_the_bounds() {
        let scene = SceneDesc::test_scene();
        let bounds = SceneDesc::bounds();
        for tile in &scene.tiles {
            assert!(bounds.contains_rect(tile.shape.rect()));
        }
        for line in &scene.lines {
            assert!(bounds.contains(line.origin));
        }
    }

    #[test]
    fn one_text_line_per_size() {
        let scene = SceneDesc::test_scene();
        let sizes: Vec<f32> = scene.lines.iter().map(|l| l.size).collect();
        assert_eq!(sizes, TEXT_SIZES);
        // Lines go down the page without overlapping.
        for pair in scene.lines.windows(2) {
            assert!(pair[1].origin.y - pair[0].origin.y >= f64::from(pair[1].size));
        }
    }

    #[test]
    fn distinct_gradients_can_be_limited() {
        let count = |desc: &SceneDesc| {
            let mut pairs: Vec<_> = desc
                .tiles
                .iter()
                .map(|t| t.colors.map(|c| c.to_rgba8().to_u32()))
                .collect();
            pairs.sort_unstable();
            pairs.dedup();
            pairs.len()
        };
        assert_eq!(count(&SceneDesc::test_scene()), RECT_COUNT);
        let limited = SceneDesc::test_scene().with_distinct_gradients(64);
        assert_eq!(count(&limited), 64);
        assert_eq!(limited.tiles.len(), RECT_COUNT);
        // Shapes and gradient lines do not change.
        for (a, b) in limited.tiles.iter().zip(SceneDesc::test_scene().tiles) {
            assert_eq!((a.shape, a.gradient_start), (b.shape, b.gradient_start));
        }
        assert_eq!(
            count(&SceneDesc::test_scene().with_distinct_gradients(0)),
            1
        );
    }

    #[test]
    fn unit_numbers_stay_in_range() {
        let mut rng = Lcg(1);
        for _ in 0..10_000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u));
        }
    }
}
