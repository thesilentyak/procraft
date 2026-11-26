//! How the canvas's colours look on the screen alone: a matrix each colour goes through in linear
//! light, as a plugin on the view look's hook gives it (and the grey view, after it). Nothing read,
//! saved, exported or picked from the canvas goes through it.

/// Rows red, green and blue: three weights for a colour's linear red, green and blue, then what is
/// added.
pub type ViewLook = [[f32; 4]; 3];

/// The look that changes nothing.
pub(crate) const IDENTITY: ViewLook = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

/// `first`, then `next`: one matrix for both.
pub(crate) fn then(first: &ViewLook, next: &ViewLook) -> ViewLook {
    let [r, g, b] = *first;
    next.map(|[x, y, z, add]| {
        [
            x * r[0] + y * g[0] + z * b[0],
            x * r[1] + y * g[1] + z * b[1],
            x * r[2] + y * g[2] + z * b[2],
            x * r[3] + y * g[3] + z * b[3] + add,
        ]
    })
}

/// A linear colour through `look`, not clipped.
#[cfg(test)]
pub(crate) fn through(look: &ViewLook, rgb: [f32; 3]) -> [f32; 3] {
    look.map(|row| row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2] + row[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_looks_make_one_that_does_both_in_turn() {
        let swap: ViewLook = [[0.0, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let invert: ViewLook = [[-1.0, 0.0, 0.0, 1.0], [0.0, -1.0, 0.0, 1.0], [0.0, 0.0, -1.0, 1.0]];
        let colour = [0.2, 0.5, 0.9];
        let both = then(&swap, &invert);
        let step = through(&invert, through(&swap, colour));
        for (a, b) in through(&both, colour).iter().zip(step) {
            assert!((a - b).abs() < 1e-6, "{both:?}");
        }
        assert_eq!(then(&IDENTITY, &swap), swap);
        assert_eq!(then(&swap, &IDENTITY), swap);
    }
}
