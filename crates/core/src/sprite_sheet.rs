//! The animation as a sprite sheet for a game: every frame that plays in one PNG, side by side in
//! a row, the background left out, and beside it a JSON file in Aseprite's format (each frame's
//! place, size and time), which game engines' importers read.

use std::path::Path;

use serde_json::{Value, json};

use crate::project::{file_error, write_atomically};
use crate::{CanvasSession, MobileError};

/// The widest a sprite sheet may be, at its scale (PNG readers and game engines stumble past it).
const MOST_SIDE: u64 = 32_768;

/// One frame of a sheet: its name, how long it shows in thousandths of a second, its pixels.
struct SheetFrame {
    name: String,
    duration: u32,
    rgba: Vec<u8>,
}

#[uniffi::export]
impl CanvasSession {
    /// The frames that play as a sprite sheet: a PNG at `png_path`, the frames left to right
    /// (a frame held longer once, its time in the JSON), each pixel `scale` across (1: as drawn),
    /// transparent where the background was; and Aseprite's JSON at `json_path`, naming the PNG as
    /// `image_name`. Nothing is written without frames, or past 32 768 pixels across.
    pub fn export_sprite_sheet(
        &self,
        png_path: String,
        json_path: String,
        image_name: String,
        scale: u32,
    ) -> Result<(), MobileError> {
        let scale = scale.clamp(1, 16);
        let (units, fps, holds, names) = {
            let st = self.shared.lock();
            let units = crate::animation::played(&st.doc);
            let animation = st.doc.animation.clone().unwrap_or_default();
            let holds: Vec<u32> = units.iter().map(|&u| animation.hold(u)).collect();
            let names: Vec<String> =
                units.iter().map(|&u| st.doc.layer(u).map(|l| l.name.clone()).unwrap_or_default()).collect();
            (units, animation.fps.max(1), holds, names)
        };
        if units.is_empty() {
            return Err(file_error("saving", "there are no frames"));
        }
        self.exported(0.0);
        let frame_ms = 1000.0 / fps as f32;
        let mut frames = Vec::with_capacity(units.len());
        let mut size = None;
        for (k, ((&unit, hold), name)) in units.iter().zip(&holds).zip(names).enumerate() {
            let (w, h, rgba) = self.render_frame(unit, true, 0)?;
            if *size.get_or_insert((w, h)) != (w, h) {
                return Err(file_error("saving", "the frames differ in size"));
            }
            let duration = (frame_ms * (1 + hold) as f32).round() as u32;
            frames.push(SheetFrame { name, duration, rgba });
            self.exported(0.9 * (k + 1) as f32 / units.len() as f32);
        }
        let (w, h) = size.unwrap_or((1, 1));
        if u64::from(w) * u64::from(scale) * frames.len() as u64 > MOST_SIDE {
            return Err(file_error("saving", "the sheet would be too wide at this scale"));
        }
        let (sheet_w, sheet_h, rgba) = sheet(&frames, w, h, scale)?;
        let png = crate::project::png_of(sheet_w, sheet_h, &rgba)?;
        let json = aseprite_json(&frames, w * scale, h * scale, &image_name, scale);
        write_atomically(Path::new(&png_path), |f| {
            crate::storage::write(f, &png).map_err(|e| file_error("saving", e))
        })?;
        let text = serde_json::to_vec_pretty(&json).map_err(|e| file_error("saving", e))?;
        write_atomically(Path::new(&json_path), |f| {
            crate::storage::write(f, &text).map_err(|e| file_error("saving", e))
        })?;
        self.exported(1.0);
        Ok(())
    }
}

/// The frames (each `w × h`, straight RGBA) in one row, each pixel `scale` across.
fn sheet(frames: &[SheetFrame], w: u32, h: u32, scale: u32) -> Result<(u32, u32, Vec<u8>), MobileError> {
    let (fw, fh) = (w as usize * scale as usize, h as usize * scale as usize);
    let count = frames.len();
    let row = fw * count * 4;
    let mut rgba = vec![0u8; row * fh];
    for (k, frame) in frames.iter().enumerate() {
        let big = procraft_engine::pixel::scaled_up(&frame.rgba, w, h, scale)
            .ok_or_else(|| file_error("saving", "a frame couldn't be scaled"))?;
        for (y, line) in big.chunks_exact(fw * 4).enumerate() {
            let start = y * row + k * fw * 4;
            if let Some(dest) = rgba.get_mut(start..start + fw * 4) {
                dest.copy_from_slice(line);
            }
        }
    }
    let width = u32::try_from(fw * count).map_err(|_| file_error("saving", "the sheet is too wide"))?;
    Ok((width, u32::try_from(fh).unwrap_or(u32::MAX), rgba))
}

/// Aseprite's JSON for a sheet of `frames` in a row, each `w × h` as placed: the "array" kind,
/// read by Godot's, Unity's, GameMaker's and Phaser's importers alike.
fn aseprite_json(frames: &[SheetFrame], w: u32, h: u32, image: &str, scale: u32) -> Value {
    let size = |w: u32, h: u32| json!({ "w": w, "h": h });
    let list: Vec<Value> = frames
        .iter()
        .enumerate()
        .map(|(k, f)| {
            let x = w * k as u32;
            json!({
                "filename": if f.name.is_empty() { format!("frame {k}") } else { f.name.clone() },
                "frame": { "x": x, "y": 0, "w": w, "h": h },
                "rotated": false,
                "trimmed": false,
                "spriteSourceSize": { "x": 0, "y": 0, "w": w, "h": h },
                "sourceSize": size(w, h),
                "duration": f.duration,
            })
        })
        .collect();
    json!({
        "frames": list,
        "meta": {
            "app": "https://github.com/artcraft-android/procraft",
            "version": env!("CARGO_PKG_VERSION"),
            "image": image,
            "format": "RGBA8888",
            "size": size(w * frames.len() as u32, h),
            "scale": scale.to_string(),
            "frameTags": [],
            "layers": [],
            "slices": [],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use procraft_engine::document::LayerId;
    use procraft_engine::geom::IRect;
    use std::sync::Arc;

    /// An 8 × 8 pixel art canvas on the GPU (none without one): a red frame and a blue one held
    /// twice as long.
    fn two_frames() -> Option<Arc<CanvasSession>> {
        let gpu = procraft_gpu::Gpu::headless().map_err(|e| eprintln!("skipped: {e}")).ok()?;
        let s = CanvasSession::new(8, 8);
        {
            let mut st = s.shared.lock();
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        assert!(s.make_pixel_art());
        let first = s.layers().iter().find(|l| l.current).map(|l| l.id)?;
        let second = s.add_layer();
        for (id, colour) in [(first, [255u8, 0, 0, 255]), (second, [0, 0, 255, 255])] {
            let mut guard = s.shared.lock();
            let st = &mut *guard;
            let (canvas, gpu) = (st.canvas.as_mut()?, st.gpu.as_ref()?);
            canvas.upload_layer_part(gpu, LayerId(id), IRect::from_size(8, 8), &colour.repeat(64)).ok()?;
        }
        assert!(s.set_animation_assist(true));
        s.set_frame_hold(second, 1);
        Some(s)
    }

    #[test]
    fn the_frames_become_a_sheet_and_aseprite_s_json() {
        let Some(s) = two_frames() else { return };
        let dir = std::env::temp_dir().join(format!("procraft-sheet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (png, json) = (dir.join("a.png"), dir.join("a.json"));
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
        s.export_sprite_sheet(path(&png), path(&json), "a.png".into(), 2).unwrap();
        assert_eq!(s.export_progress(), 1.0);
        let pixels = crate::project::png_pixels(&std::fs::read(&png).unwrap(), 32, 16).unwrap();
        let at = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!((at(3, 3), at(20, 12)), (&[255, 0, 0, 255][..], &[0, 0, 255, 255][..]));
        let json: Value = serde_json::from_slice(&std::fs::read(&json).unwrap()).unwrap();
        // pixel art plays at 10 a second: 100 ms, and twice that for the held frame
        let times: Vec<u64> =
            json["frames"].as_array().unwrap().iter().map(|f| f["duration"].as_u64().unwrap()).collect();
        assert_eq!(times, [100, 200]);
        assert_eq!(json["meta"]["size"], json!({ "w": 32, "h": 16 }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pixel_art_animated_png_is_scaled_up_crisply() {
        let Some(s) = two_frames() else { return };
        let path = std::env::temp_dir().join(format!("procraft-crisp-{}.png", std::process::id()));
        s.export_apng(path.to_string_lossy().into_owned(), 0, 10.0, true).unwrap();
        let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        // 8 pixels to about 512: each pixel 64 across
        assert_eq!((reader.info().width, reader.info().height), (512, 512));
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        assert!(buf.chunks_exact(4).all(|p| p == [255, 0, 0, 255]), "solid red, no soft edge");
        let _ = std::fs::remove_file(&path);
    }

    fn frame(name: &str, duration: u32, colour: [u8; 4]) -> SheetFrame {
        SheetFrame { name: name.into(), duration, rgba: colour.repeat(2 * 2) }
    }

    #[test]
    fn a_sheet_lays_the_frames_in_a_row_each_pixel_a_square() {
        let frames = [frame("a", 100, [255, 0, 0, 255]), frame("b", 200, [0, 0, 255, 128])];
        let (w, h, rgba) = sheet(&frames, 2, 2, 3).unwrap();
        assert_eq!((w, h), (12, 6));
        let at = |x: usize, y: usize| &rgba[(y * 12 + x) * 4..(y * 12 + x) * 4 + 4];
        assert_eq!(at(0, 0), [255, 0, 0, 255]);
        assert_eq!(at(5, 5), [255, 0, 0, 255]);
        assert_eq!(at(6, 0), [0, 0, 255, 128]);
        assert_eq!(at(11, 5), [0, 0, 255, 128]);
    }

    #[test]
    fn the_json_is_aseprite_s_array_with_each_frame_s_place_and_time() {
        let frames = [frame("Walk 1", 100, [0; 4]), frame("", 300, [0; 4])];
        let json = aseprite_json(&frames, 64, 48, "fox sheet.png", 1);
        let list = json["frames"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["filename"], "Walk 1");
        assert_eq!(list[1]["filename"], "frame 1");
        assert_eq!(list[1]["frame"], json!({ "x": 64, "y": 0, "w": 64, "h": 48 }));
        assert_eq!(list[1]["duration"], 300);
        assert_eq!(json["meta"]["image"], "fox sheet.png");
        assert_eq!(json["meta"]["size"], json!({ "w": 128, "h": 48 }));
        assert_eq!(json["meta"]["frameTags"], json!([]));
    }
}
