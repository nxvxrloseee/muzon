use serde::{Deserialize, Serialize};
use specta::Type;

/// The four swatches the Now Playing background gradient is built from.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrackPalette {
    pub vibrant: String,
    pub dark_vibrant: String,
    pub muted: String,
    pub dark_muted: String,
}

impl Default for TrackPalette {
    /// Same fallbacks the frontend used when extraction failed, so a cover that
    /// yields nothing usable still produces the familiar purple gradient.
    fn default() -> Self {
        Self {
            vibrant: "#7c5cff".into(),
            dark_vibrant: "#2a1f4d".into(),
            muted: "#4a4560".into(),
            dark_muted: "#15121f".into(),
        }
    }
}

/// Downscale target before counting colors. The exact pixels don't matter for
/// picking dominant colors, and this bounds the work regardless of cover size.
const SAMPLE_MAX_DIM: u32 = 50;
/// 5 bits per channel, i.e. colors within the same 8-value cube are one bucket.
const QUANT_SHIFT: u8 = 3;

const WEIGHT_SATURATION: f64 = 3.0;
const WEIGHT_LUMA: f64 = 6.5;
const WEIGHT_POPULATION: f64 = 0.5;

struct Target {
    luma: f64,
    luma_min: f64,
    luma_max: f64,
    saturation: f64,
    saturation_min: f64,
    saturation_max: f64,
}

const VIBRANT: Target = Target {
    luma: 0.5,
    luma_min: 0.3,
    luma_max: 0.7,
    saturation: 1.0,
    saturation_min: 0.35,
    saturation_max: 1.0,
};
const DARK_VIBRANT: Target = Target {
    luma: 0.26,
    luma_min: 0.0,
    luma_max: 0.45,
    saturation: 1.0,
    saturation_min: 0.35,
    saturation_max: 1.0,
};
const MUTED: Target = Target {
    luma: 0.5,
    luma_min: 0.3,
    luma_max: 0.7,
    saturation: 0.3,
    saturation_min: 0.0,
    saturation_max: 0.4,
};
const DARK_MUTED: Target = Target {
    luma: 0.26,
    luma_min: 0.0,
    luma_max: 0.45,
    saturation: 0.3,
    saturation_min: 0.0,
    saturation_max: 0.4,
};

struct Swatch {
    rgb: [u8; 3],
    population: u32,
    hsl: (f64, f64, f64),
}

/// Extracts a Vibrant-style palette from encoded image bytes.
///
/// This used to run in the webview via `node-vibrant`, which decoded the cover
/// and quantized it on the main thread on every track change. Here the bytes
/// are already at hand from the thumbnail cache and the work is off the UI
/// thread entirely.
pub fn extract(image_bytes: &[u8]) -> Option<TrackPalette> {
    let img = image::load_from_memory(image_bytes).ok()?;
    let sample = img.thumbnail(SAMPLE_MAX_DIM, SAMPLE_MAX_DIM).to_rgb8();

    let mut buckets: std::collections::HashMap<u32, (u64, u64, u64, u32)> =
        std::collections::HashMap::new();
    for pixel in sample.pixels() {
        let [r, g, b] = pixel.0;
        let key = (u32::from(r >> QUANT_SHIFT) << 10)
            | (u32::from(g >> QUANT_SHIFT) << 5)
            | u32::from(b >> QUANT_SHIFT);
        let entry = buckets.entry(key).or_insert((0, 0, 0, 0));
        entry.0 += u64::from(r);
        entry.1 += u64::from(g);
        entry.2 += u64::from(b);
        entry.3 += 1;
    }

    let mut swatches: Vec<Swatch> = buckets
        .into_values()
        .map(|(r_sum, g_sum, b_sum, count)| {
            let n = u64::from(count);
            let rgb = [
                (r_sum / n) as u8,
                (g_sum / n) as u8,
                (b_sum / n) as u8,
            ];
            Swatch {
                rgb,
                population: count,
                hsl: rgb_to_hsl(rgb),
            }
        })
        .collect();

    if swatches.is_empty() {
        return None;
    }
    // Near-black and near-white pixels are almost always background or paper,
    // never the color a listener would call "the color of this cover".
    swatches.retain(|s| s.hsl.2 > 0.05 && s.hsl.2 < 0.95);
    if swatches.is_empty() {
        return None;
    }

    let max_population = swatches.iter().map(|s| s.population).max().unwrap_or(1);

    let mut used: Vec<[u8; 3]> = Vec::new();
    let defaults = TrackPalette::default();
    let mut pick = |target: &Target, fallback: &str| -> String {
        match best_swatch(&swatches, target, max_population, &used) {
            Some(rgb) => {
                used.push(rgb);
                to_hex(rgb)
            }
            None => fallback.to_string(),
        }
    };

    Some(TrackPalette {
        vibrant: pick(&VIBRANT, &defaults.vibrant),
        dark_vibrant: pick(&DARK_VIBRANT, &defaults.dark_vibrant),
        muted: pick(&MUTED, &defaults.muted),
        dark_muted: pick(&DARK_MUTED, &defaults.dark_muted),
    })
}

fn best_swatch(
    swatches: &[Swatch],
    target: &Target,
    max_population: u32,
    used: &[[u8; 3]],
) -> Option<[u8; 3]> {
    let mut best: Option<([u8; 3], f64)> = None;

    for swatch in swatches {
        let (_, saturation, luma) = swatch.hsl;
        if saturation < target.saturation_min || saturation > target.saturation_max {
            continue;
        }
        if luma < target.luma_min || luma > target.luma_max {
            continue;
        }
        // One color must not stand in for two swatches, or the gradient
        // collapses to a single flat tone.
        if used.contains(&swatch.rgb) {
            continue;
        }

        let score = invert_diff(saturation, target.saturation) * WEIGHT_SATURATION
            + invert_diff(luma, target.luma) * WEIGHT_LUMA
            + (f64::from(swatch.population) / f64::from(max_population)) * WEIGHT_POPULATION;

        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((swatch.rgb, score));
        }
    }

    best.map(|(rgb, _)| rgb)
}

fn invert_diff(value: f64, target: f64) -> f64 {
    1.0 - (value - target).abs()
}

fn rgb_to_hsl(rgb: [u8; 3]) -> (f64, f64, f64) {
    let r = f64::from(rgb[0]) / 255.0;
    let g = f64::from(rgb[1]) / 255.0;
    let b = f64::from(rgb[2]) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let luma = (max + min) / 2.0;
    let delta = max - min;

    if delta.abs() < f64::EPSILON {
        return (0.0, 0.0, luma);
    }

    let saturation = if luma > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };

    let hue = if (max - r).abs() < f64::EPSILON {
        ((g - b) / delta) % 6.0
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };

    ((hue * 60.0).rem_euclid(360.0), saturation, luma)
}

fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_solid(r: u8, g: u8, b: u8) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(64, 64, image::Rgb([r, g, b]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn extracts_a_saturated_swatch_from_a_solid_cover() {
        let palette = extract(&encode_solid(200, 30, 30)).expect("palette");
        // A mid-luma saturated red satisfies the Vibrant target, and no other
        // swatch may reuse it.
        assert_eq!(palette.vibrant, "#c81e1e");
        assert_ne!(palette.dark_vibrant, palette.vibrant);
    }

    #[test]
    fn falls_back_when_every_pixel_is_filtered_out() {
        assert!(extract(&encode_solid(0, 0, 0)).is_none());
    }

    #[test]
    fn rejects_bytes_that_are_not_an_image() {
        assert!(extract(b"definitely not an image").is_none());
    }
}
