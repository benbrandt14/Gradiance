//! An image's opacity as a field (brief S-14): the input of
//! [`polygonize_alpha`](crate::polygonize::polygonize_alpha), which traces a
//! body outline from the opaque pixels.

use bevy::math::Vec2;

/// The alpha level that counts as solid.
pub const ALPHA_THRESHOLD: f32 = 0.5;

/// Per-pixel opacity, row 0 at the top, values in `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct AlphaMask {
    width: usize,
    height: usize,
    alpha: Vec<f32>,
}

impl AlphaMask {
    /// A mask of `width × height` pixels, or `None` when the sizes do not
    /// match or a side is zero.
    #[must_use]
    pub fn new(width: usize, height: usize, alpha: Vec<f32>) -> Option<Self> {
        (width > 0 && height > 0 && alpha.len() == width * height).then(|| Self {
            width,
            height,
            alpha: alpha.into_iter().map(|a| a.clamp(0.0, 1.0)).collect(),
        })
    }

    /// The size in pixels.
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// The alpha of pixel `(x, y)`; 0 outside the image.
    fn at(&self, x: i64, y: i64) -> f32 {
        match (usize::try_from(x), usize::try_from(y)) {
            (Ok(x), Ok(y)) if x < self.width && y < self.height => self.alpha[y * self.width + x],
            _ => 0.0,
        }
    }

    /// Bilinear alpha at `p` in pixel units (pixel centres at `i + 0.5`).
    #[must_use]
    pub fn sample(&self, p: Vec2) -> f32 {
        let q = p - Vec2::splat(0.5);
        let (x0, y0) = (q.x.floor(), q.y.floor());
        let (tx, ty) = (q.x - x0, q.y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let top = self.at(x0, y0) * (1.0 - tx) + self.at(x0 + 1, y0) * tx;
        let bottom = self.at(x0, y0 + 1) * (1.0 - tx) + self.at(x0 + 1, y0 + 1) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    /// The solid field for an image `width_m` metres wide, centred on the
    /// origin, y up: negative where the image is opaque (the SDF sign).
    /// Not a distance: only its zero set is used.
    pub fn field(&self, width_m: f32) -> impl Fn(Vec2) -> f32 + '_ {
        let scale = self.width as f32 / width_m;
        let half = Vec2::new(self.width as f32, self.height as f32) * 0.5;
        move |p: Vec2| {
            let pixel = Vec2::new(p.x * scale + half.x, half.y - p.y * scale);
            ALPHA_THRESHOLD - self.sample(pixel)
        }
    }

    /// The image's extent in metres for a width of `width_m`.
    #[must_use]
    pub fn extent(&self, width_m: f32) -> Vec2 {
        Vec2::new(width_m, width_m * self.height as f32 / self.width as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_is_bilinear_and_zero_outside() {
        let mask = AlphaMask::new(2, 1, vec![0.0, 1.0]).unwrap_or_else(|| unreachable!());
        assert!((mask.sample(Vec2::new(1.0, 0.5)) - 0.5).abs() < 1e-6);
        assert_eq!(mask.sample(Vec2::new(-5.0, 0.5)), 0.0);
        assert!(AlphaMask::new(2, 2, vec![0.0]).is_none());
    }
}
