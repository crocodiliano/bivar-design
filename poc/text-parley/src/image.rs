//! RGBA8 images: PNG files and pixel comparison.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

/// Straight (not premultiplied) RGBA8 pixels, row by row, no padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(pixels.len(), width as usize * height as usize * 4);
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn save_png(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&self.pixels)
            .map_err(|e| e.to_string())
    }

    /// Reads an 8-bit RGBA PNG, as written by [`Rgba::save_png`].
    pub fn load_png(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut reader = png::Decoder::new(BufReader::new(file))
            .read_info()
            .map_err(|e| e.to_string())?;
        let mut pixels = vec![0; reader.output_buffer_size().ok_or("image too large")?];
        let info = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
        if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
            return Err(format!("{}: not an 8-bit RGBA PNG", path.display()));
        }
        pixels.truncate(info.buffer_size());
        Ok(Self::new(info.width, info.height, pixels))
    }

    /// Copies the `width` × `height` block whose top-left corner is (`x`, `y`).
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Self {
        assert!(x + width <= self.width && y + height <= self.height);
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for row in y..y + height {
            let start = (row as usize * self.width as usize + x as usize) * 4;
            pixels.extend_from_slice(&self.pixels[start..start + width as usize * 4]);
        }
        Self::new(width, height, pixels)
    }

    /// The pixels as `[r, g, b, a]` groups.
    pub fn texels(&self) -> &[[u8; 4]] {
        self.pixels.as_chunks().0
    }
}

/// How much two images of the same size differ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diff {
    pub total_pixels: usize,
    pub differing_pixels: usize,
    /// Largest difference in any channel, 0–255.
    pub max_channel_delta: u8,
    /// Average difference per channel over all pixels, 0–255.
    pub mean_channel_delta: f64,
}

/// Vello on the GPU is not bit-for-bit repeatable on the reference machine:
/// measured over 99 repeated renders of the same scene, up to 3 pixels in
/// 1,024,000 changed by 1 level (of 255) on DirectX 12, and up to 1 pixel on
/// Vulkan. This is the allowed difference between two GPU renders of the
/// same view.
pub const GPU_REPEAT_TOLERANCE: Tolerance = Tolerance {
    max_channel_delta: 1,
    max_differing_pixels: 3,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tolerance {
    pub max_channel_delta: u8,
    pub max_differing_pixels: usize,
}

impl Diff {
    pub fn identical(&self) -> bool {
        self.differing_pixels == 0
    }

    pub fn within(&self, tolerance: Tolerance) -> bool {
        self.max_channel_delta <= tolerance.max_channel_delta
            && self.differing_pixels <= tolerance.max_differing_pixels
    }

    pub fn differing_percent(&self) -> f64 {
        100.0 * self.differing_pixels as f64 / self.total_pixels.max(1) as f64
    }
}

pub fn compare(a: &Rgba, b: &Rgba) -> Result<Diff, String> {
    if (a.width, a.height) != (b.width, b.height) {
        return Err(format!(
            "sizes differ: {}x{} vs {}x{}",
            a.width, a.height, b.width, b.height
        ));
    }
    let mut differing_pixels = 0;
    let mut max_channel_delta = 0;
    let mut sum = 0u64;
    for (pa, pb) in a.texels().iter().zip(b.texels()) {
        let mut differs = false;
        for (ca, cb) in pa.iter().zip(pb) {
            let d = ca.abs_diff(*cb);
            differs |= d != 0;
            max_channel_delta = max_channel_delta.max(d);
            sum += u64::from(d);
        }
        differing_pixels += usize::from(differs);
    }
    let total_pixels = a.pixels.len() / 4;
    Ok(Diff {
        total_pixels,
        differing_pixels,
        max_channel_delta,
        mean_channel_delta: sum as f64 / (total_pixels.max(1) * 4) as f64,
    })
}

/// Black where the images match, white where they differ, to see where.
pub fn diff_mask(a: &Rgba, b: &Rgba) -> Rgba {
    let pixels = a
        .texels()
        .iter()
        .zip(b.texels())
        .flat_map(|(pa, pb)| {
            let v = if pa == pb { 0 } else { 255 };
            [v, v, v, 255]
        })
        .collect();
    Rgba::new(a.width, a.height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(width: u32, height: u32) -> Rgba {
        let pixels = (0..width * height)
            .flat_map(|i| {
                let v = if (i % width + i / width).is_multiple_of(2) {
                    30
                } else {
                    220
                };
                [v, (i % 256) as u8, 255 - v, 255]
            })
            .collect();
        Rgba::new(width, height, pixels)
    }

    #[test]
    fn png_round_trip_keeps_every_pixel() {
        let dir = std::env::temp_dir().join("bivar-poc-text-parley-png-test");
        let path = dir.join(format!("{}.png", std::process::id()));
        let image = checker(37, 19);
        image.save_png(&path).unwrap();
        let back = Rgba::load_png(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(back, image);
    }

    #[test]
    fn loading_a_missing_file_is_an_error() {
        assert!(Rgba::load_png(Path::new("does/not/exist.png")).is_err());
    }

    #[test]
    fn identical_images_have_no_difference() {
        let a = checker(8, 8);
        let diff = compare(&a, &a).unwrap();
        assert!(diff.identical());
        assert_eq!(diff.max_channel_delta, 0);
        assert_eq!(diff.total_pixels, 64);
    }

    #[test]
    fn one_changed_pixel_is_counted_once() {
        let a = checker(8, 8);
        let mut b = a.clone();
        b.pixels[4 * 10] += 15;
        b.pixels[4 * 10 + 1] += 1;
        let diff = compare(&a, &b).unwrap();
        assert_eq!(diff.differing_pixels, 1);
        assert_eq!(diff.max_channel_delta, 15);
        let mask = diff_mask(&a, &b);
        assert_eq!(&mask.pixels[4 * 10..4 * 11], &[255, 255, 255, 255]);
        assert_eq!(&mask.pixels[0..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn tolerance_limits_both_count_and_size() {
        let a = checker(8, 8);
        let mut b = a.clone();
        b.pixels[0] += 1;
        let tolerance = Tolerance {
            max_channel_delta: 1,
            max_differing_pixels: 1,
        };
        assert!(compare(&a, &b).unwrap().within(tolerance));
        b.pixels[4] += 1;
        assert!(!compare(&a, &b).unwrap().within(tolerance));
        let mut c = a.clone();
        c.pixels[0] += 2;
        assert!(!compare(&a, &c).unwrap().within(tolerance));
    }

    #[test]
    fn different_sizes_cannot_be_compared() {
        assert!(compare(&checker(4, 4), &checker(4, 5)).is_err());
    }

    #[test]
    fn crop_takes_the_right_block() {
        let image = checker(10, 6);
        let block = image.crop(3, 2, 4, 3);
        assert_eq!((block.width, block.height), (4, 3));
        for row in 0..3 {
            for col in 0..4 {
                let src = (((2 + row) * 10 + 3 + col) * 4) as usize;
                let dst = ((row * 4 + col) * 4) as usize;
                assert_eq!(block.pixels[dst..dst + 4], image.pixels[src..src + 4]);
            }
        }
    }
}
