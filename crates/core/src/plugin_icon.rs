//! A plugin's icon, as its manifest draws it: SVG path data on the interface icons' 24-unit grid,
//! filled even-odd in the interface's ink (tinted as the app tints its own when chosen). What comes
//! in is read and written out again in plain absolute moves, lines and curves, so the apps only ever
//! draw numbers this checked.

/// The grid an icon is drawn on, units across and down.
pub(crate) const GRID: f32 = 24.0;
/// The most an icon's path data may be, in bytes, and the most numbers in it.
const MOST_TEXT: usize = 16 << 10;
const MOST_NUMBERS: usize = 4096;

/// A piece of path data: a command's letter, or a number.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Token {
    Command(u8),
    Number(f32),
}

/// `d`'s commands and numbers, if it holds nothing else (spaces and commas between them).
fn tokens(d: &str) -> Option<Vec<Token>> {
    let bytes = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b.is_ascii_whitespace() || b == b',' {
            i += 1;
        } else if b"MmLlHhVvCcZz".contains(&b) {
            out.push(Token::Command(b));
            i += 1;
        } else if b.is_ascii_digit() || b"+-.".contains(&b) {
            // a sign, digits, one point, more digits (an exponent is never needed on this grid)
            let start = i;
            i += usize::from(b"+-".contains(&b));
            let mut point = false;
            while let Some(&c) = bytes.get(i) {
                if c == b'.' && !point {
                    point = true;
                } else if !c.is_ascii_digit() {
                    break;
                }
                i += 1;
            }
            let number: f32 = d.get(start..i)?.parse().ok()?;
            out.push(Token::Number(number));
        } else {
            return None;
        }
    }
    Some(out)
}

/// A number as written out: to a hundredth of a unit, with nothing after it that says nothing.
fn written(v: f32) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".to_owned() } else { s.to_owned() }
}

/// The icon `d` draws, written out again in absolute `M`, `L`, `C` and `Z`, if it holds up: path data
/// of moves, lines (`L`, `H`, `V`), cubic curves and closes, absolute or relative, every point on the
/// grid (0 to 24 each way), something drawn, and not too much of it.
pub(crate) fn icon_path(d: &str) -> Option<String> {
    if d.len() > MOST_TEXT {
        return None;
    }
    let tokens = tokens(d)?;
    if tokens.iter().filter(|t| matches!(t, Token::Number(_))).count() > MOST_NUMBERS {
        return None;
    }
    let on_grid = |v: f32| v.is_finite() && (0.0..=GRID).contains(&v);
    let mut out = String::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    // where the open shape started, for a close; whether one is open
    let (mut start_x, mut start_y) = (0.0f32, 0.0f32);
    let mut open = false;
    let mut drawn = false;
    let mut i = 0;
    let mut command: Option<u8> = None;
    let number = |at: usize| match tokens.get(at) {
        Some(Token::Number(v)) => Some(*v),
        _ => None,
    };
    while let Some(token) = tokens.get(i) {
        let c = match *token {
            Token::Command(c) => {
                i += 1;
                c
            }
            // numbers after a command's own go on with it (after a move, as lines)
            Token::Number(_) => match command? {
                b'M' => b'L',
                b'm' => b'l',
                b'Z' | b'z' => return None,
                c => c,
            },
        };
        command = Some(c);
        let relative = c.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        match c.to_ascii_uppercase() {
            b'Z' => {
                if !open {
                    return None;
                }
                out.push('Z');
                (x, y) = (start_x, start_y);
                open = false;
            }
            b'M' => {
                let (nx, ny) = (number(i)? + ox, number(i + 1)? + oy);
                i += 2;
                (x, y) = (nx, ny);
                (start_x, start_y) = (nx, ny);
                open = true;
                out.push_str(&format!("M{} {}", written(x), written(y)));
            }
            b'L' | b'H' | b'V' => {
                if !open {
                    return None;
                }
                match c.to_ascii_uppercase() {
                    b'L' => {
                        (x, y) = (number(i)? + ox, number(i + 1)? + oy);
                        i += 2;
                    }
                    b'H' => {
                        x = number(i)? + ox;
                        i += 1;
                    }
                    _ => {
                        y = number(i)? + oy;
                        i += 1;
                    }
                }
                out.push_str(&format!("L{} {}", written(x), written(y)));
                drawn = true;
            }
            b'C' => {
                if !open {
                    return None;
                }
                let mut points = [0.0f32; 6];
                for (k, point) in points.iter_mut().enumerate() {
                    *point = number(i + k)? + if k % 2 == 0 { ox } else { oy };
                }
                i += 6;
                if !points.iter().all(|&v| on_grid(v)) {
                    return None;
                }
                let [x1, y1, x2, y2, nx, ny] = points;
                out.push_str(&format!(
                    "C{} {} {} {} {} {}",
                    written(x1),
                    written(y1),
                    written(x2),
                    written(y2),
                    written(nx),
                    written(ny)
                ));
                (x, y) = (nx, ny);
                drawn = true;
            }
            _ => return None,
        }
        if !on_grid(x) || !on_grid(y) {
            return None;
        }
    }
    drawn.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_is_written_out_again_in_absolute_moves_lines_and_curves() {
        // as the icon tool writes them
        let square = "M3 4.4L3 19.6L21 19.6L21 4.4Z";
        assert_eq!(icon_path(square).as_deref(), Some(square));
        // relative, with H, V, commas, numbers after numbers and a curve
        let d = "m2,2 h20 v20 h-20 z M12 6c2.2 0 4 1.8 4 4s0 0 0 0";
        assert_eq!(icon_path(d), None, "S is no command here");
        let d = "m2,2 h20 v20 h-20 z M12 6c2.2 0 4 1.8 4 4 l -1 1 1 1";
        assert_eq!(icon_path(d).as_deref(), Some("M2 2L22 2L22 22L2 22ZM12 6C14.2 6 16 7.8 16 10L15 11L16 12"),);
        // numbers after a move go on as lines; a long fraction to a hundredth
        assert_eq!(icon_path("M1 1 5 5 1 5Z").as_deref(), Some("M1 1L5 5L1 5Z"));
        assert_eq!(icon_path("M1.004 1 L5.555 5").as_deref(), Some("M1 1L5.55 5"));
    }

    #[test]
    fn an_icon_that_doesn_t_hold_up_is_none() {
        for bad in [
            "",
            "M1 1",
            "M1 1Z",
            "L5 5",
            "M1 1 L25 5",
            "M1 1 L-1 5",
            "M1 1 C 1 1 30 1 5 5",
            "M1 1 Q 2 2 3 3",
            "M1 1 A 1 1 0 0 0 3 3",
            "M1 1 L5 5 Z Z",
            "M1 1 L5",
            "M1 1 L5 5 <script>",
            "M1 1 L5e1 5",
            "M1 1 L nan 5",
            "M1 1 L 5 5 Z 3",
        ] {
            assert_eq!(icon_path(bad), None, "{bad:?}");
        }
        // nor one too long
        let long = format!("M1 1{}", " L2 2".repeat(3000));
        assert_eq!(icon_path(&long), None);
        let fine = format!("M1 1{}", " L2 2".repeat(100));
        assert!(icon_path(&fine).is_some());
    }
}
