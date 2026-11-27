//! The animation as a sprite sheet for a game: every frame that plays in one PNG, a row for each
//! animation (by name), the background left out, and beside it a JSON file in Aseprite's format
//! (each frame's place, size and time, and the animations' names), which game engines' importers
//! read.

use std::path::Path;

use serde_json::{Value, json};

use crate::project::{file_error, write_atomically};
use crate::{CanvasSession, MobileError};

/// The widest a sprite sheet may be, at its scale (PNG readers and game engines stumble past it).
const MOST_SIDE: u64 = 32_768;

/// One frame of a sheet: its name, how long it shows in thousandths of a second, its row and
/// place along it, its pixels.
struct SheetFrame {
    name: String,
    duration: u32,
    row: u32,
    column: u32,
    rgba: Vec<u8>,
}

/// A named animation's frames, as places in the sheet's list of frames.
struct SheetTag {
    name: String,
    from: usize,
    to: usize,
}

#[uniffi::export]
impl CanvasSession {
    /// The frames that play as a sprite sheet: a PNG at `png_path`, a row for each animation (one
    /// row without names), the frames left to right (a frame held longer once, its time in the
    /// JSON), each pixel `scale` across (1: as drawn), transparent where the background was; and
    /// Aseprite's JSON at `json_path`, naming the PNG as `image_name` and the animations as its
    /// frame tags. With named animations and an `each_prefix`, each named one also gets a sheet
    /// of its own, one row, at `<each_prefix> <name> (<n> frames).png<each_suffix>` (a game
    /// engine without an Aseprite importer cuts it by its frame count); their paths are returned.
    /// Nothing is written without frames, or past 32 768 pixels a side.
    pub fn export_sprite_sheet(
        &self,
        png_path: String,
        json_path: String,
        image_name: String,
        scale: u32,
        each_prefix: String,
        each_suffix: String,
    ) -> Result<Vec<String>, MobileError> {
        let scale = scale.clamp(1, 16);
        // each frame with its row and place along it, and the named animations
        let (units, fps, tags) = {
            let st = self.shared.lock();
            let all = procraft_engine::animation::frames(&st.doc);
            let animation = st.doc.animation.clone().unwrap_or_default();
            let mut units = Vec::new();
            let mut tags = Vec::new();
            for (row, (name, range)) in procraft_engine::animation::animations(&st.doc).into_iter().enumerate() {
                let from = units.len();
                for (column, unit) in range.filter_map(|i| all.get(i).copied()).enumerate() {
                    let layer_name = st.doc.layer(unit).map(|l| l.name.clone()).unwrap_or_default();
                    units.push((unit, animation.hold(unit), layer_name, row as u32, column as u32));
                }
                if let Some(name) = name
                    && units.len() > from
                {
                    tags.push(SheetTag { name, from, to: units.len() - 1 });
                }
            }
            (units, animation.fps.max(1), tags)
        };
        if units.is_empty() {
            return Err(file_error("saving", "there are no frames"));
        }
        let count = units.len();
        self.exported(0.0);
        let frame_ms = 1000.0 / fps as f32;
        let mut frames = Vec::with_capacity(units.len());
        let mut size = None;
        for (k, (unit, hold, name, row, column)) in units.into_iter().enumerate() {
            let (w, h, rgba) = self.render_frame(unit, true, 0)?;
            if *size.get_or_insert((w, h)) != (w, h) {
                return Err(file_error("saving", "the frames differ in size"));
            }
            let duration = (frame_ms * (1 + hold) as f32).round() as u32;
            frames.push(SheetFrame { name, duration, row, column, rgba });
            self.exported(0.9 * (k + 1) as f32 / count as f32);
        }
        let (w, h) = size.unwrap_or((1, 1));
        let columns = frames.iter().map(|f| f.column + 1).max().unwrap_or(1);
        let rows = frames.iter().map(|f| f.row + 1).max().unwrap_or(1);
        if u64::from(w) * u64::from(scale) * u64::from(columns) > MOST_SIDE
            || u64::from(h) * u64::from(scale) * u64::from(rows) > MOST_SIDE
        {
            return Err(file_error("saving", "the sheet would be too big at this scale"));
        }
        let (sheet_w, sheet_h, rgba) = sheet(&frames, w, h, scale)?;
        let png = crate::project::png_of(sheet_w, sheet_h, &rgba)?;
        let json = aseprite_json(&frames, &tags, w * scale, h * scale, &image_name, scale);
        write_atomically(Path::new(&png_path), |f| {
            crate::storage::write(f, &png).map_err(|e| file_error("saving", e))
        })?;
        let text = serde_json::to_vec_pretty(&json).map_err(|e| file_error("saving", e))?;
        write_atomically(Path::new(&json_path), |f| {
            crate::storage::write(f, &text).map_err(|e| file_error("saving", e))
        })?;
        // each named animation on its own, one row
        let mut each = Vec::new();
        if !each_prefix.is_empty() {
            let mut taken = std::collections::HashSet::new();
            for tag in &tags {
                let own: Vec<SheetFrame> = frames
                    .get(tag.from..=tag.to)
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                    .map(|(k, f)| SheetFrame {
                        name: f.name.clone(),
                        duration: f.duration,
                        row: 0,
                        column: k as u32,
                        rgba: f.rgba.clone(),
                    })
                    .collect();
                let mut name = file_safe(&tag.name);
                // two animations with one name: the second numbered
                let mut n = 2;
                while !taken.insert(name.to_lowercase()) {
                    name = format!("{} {n}", file_safe(&tag.name));
                    n += 1;
                }
                let count = if own.len() == 1 { "1 frame".to_owned() } else { format!("{} frames", own.len()) };
                let path = format!("{each_prefix} {name} ({count}).png{each_suffix}");
                let (ow, oh, rgba) = sheet(&own, w, h, scale)?;
                let png = crate::project::png_of(ow, oh, &rgba)?;
                write_atomically(Path::new(&path), |f| {
                    crate::storage::write(f, &png).map_err(|e| file_error("saving", e))
                })?;
                each.push(path);
            }
        }
        self.exported(1.0);
        Ok(each)
    }
}

/// `name` safe in a file's name: letters, digits, spaces, `-` and `_` kept, anything else a space.
fn file_safe(name: &str) -> String {
    let kept: String =
        name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { ' ' }).collect();
    let tidy = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    if tidy.is_empty() { "animation".to_owned() } else { tidy }
}

/// The frames (each `w × h`, straight RGBA) in their rows and places, each pixel `scale` across;
/// a short row's end left clear.
fn sheet(frames: &[SheetFrame], w: u32, h: u32, scale: u32) -> Result<(u32, u32, Vec<u8>), MobileError> {
    let (fw, fh) = (w as usize * scale as usize, h as usize * scale as usize);
    let columns = frames.iter().map(|f| f.column as usize + 1).max().unwrap_or(1);
    let rows = frames.iter().map(|f| f.row as usize + 1).max().unwrap_or(1);
    let line = fw * columns * 4;
    let mut rgba = vec![0u8; line * fh * rows];
    for frame in frames {
        let big = procraft_engine::pixel::scaled_up(&frame.rgba, w, h, scale)
            .ok_or_else(|| file_error("saving", "a frame couldn't be scaled"))?;
        for (y, part) in big.chunks_exact(fw * 4).enumerate() {
            let start = (frame.row as usize * fh + y) * line + frame.column as usize * fw * 4;
            if let Some(dest) = rgba.get_mut(start..start + fw * 4) {
                dest.copy_from_slice(part);
            }
        }
    }
    let width = u32::try_from(fw * columns).map_err(|_| file_error("saving", "the sheet is too wide"))?;
    let height = u32::try_from(fh * rows).map_err(|_| file_error("saving", "the sheet is too tall"))?;
    Ok((width, height, rgba))
}

/// Aseprite's JSON for a sheet of `frames` (each `w × h` as placed) and the animations `tags`:
/// the "array" kind, read by Godot's, Unity's, GameMaker's and Phaser's importers alike.
fn aseprite_json(frames: &[SheetFrame], tags: &[SheetTag], w: u32, h: u32, image: &str, scale: u32) -> Value {
    let size = |w: u32, h: u32| json!({ "w": w, "h": h });
    let list: Vec<Value> = frames
        .iter()
        .enumerate()
        .map(|(k, f)| {
            json!({
                "filename": if f.name.is_empty() { format!("frame {k}") } else { f.name.clone() },
                "frame": { "x": w * f.column, "y": h * f.row, "w": w, "h": h },
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
            "size": size(
                w * frames.iter().map(|f| f.column + 1).max().unwrap_or(1),
                h * frames.iter().map(|f| f.row + 1).max().unwrap_or(1),
            ),
            "scale": scale.to_string(),
            "frameTags": tags
                .iter()
                .map(|t| json!({ "name": t.name, "from": t.from, "to": t.to, "direction": "forward", "color": "#000000ff" }))
                .collect::<Vec<_>>(),
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
        assert!(
            s.export_sprite_sheet(path(&png), path(&json), "a.png".into(), 2, path(&dir.join("a")), String::new())
                .unwrap()
                .is_empty(),
            "no names, no sheets of their own"
        );
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
    fn named_animations_are_rows_and_frame_tags() {
        let Some(s) = two_frames() else { return };
        let ids = s.animation_frames();
        let third = s.add_layer();
        assert!(s.start_animation(ids[0], "idle".into()));
        assert!(s.start_animation(ids[1], "walk".into()));
        // the new third frame runs on with walk
        assert_eq!(s.animation_name_of(third).as_deref(), Some("walk"));
        let names: Vec<(String, u32, u32)> =
            s.animation_names().into_iter().map(|n| (n.name, n.first, n.count)).collect();
        assert_eq!(names, [("idle".to_owned(), 0, 1), ("walk".to_owned(), 1, 2)]);
        let dir = std::env::temp_dir().join(format!("procraft-tags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (png, json) = (dir.join("a.png"), dir.join("a.json"));
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
        let each = s
            .export_sprite_sheet(path(&png), path(&json), "a.png".into(), 1, path(&dir.join("a")), String::new())
            .unwrap();
        // a sheet each, named after the animation and its frame count
        let names: Vec<String> =
            each.iter().map(|p| std::path::Path::new(p).file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["a idle (1 frame).png", "a walk (2 frames).png"]);
        let walk = crate::project::png_pixels(&std::fs::read(&each[1]).unwrap(), 16, 8).unwrap();
        assert_eq!((&walk[..4], &walk[walk.len() - 4..]), (&[0, 0, 255, 255][..], &[0, 0, 0, 0][..]));
        // idle on the first row alone, walk's two on the second
        let pixels = crate::project::png_pixels(&std::fs::read(&png).unwrap(), 16, 16).unwrap();
        let at = |x: usize, y: usize| &pixels[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
        assert_eq!(at(2, 2), [255, 0, 0, 255]);
        assert_eq!(at(10, 2), [0, 0, 0, 0], "idle's row ends after its one frame");
        assert_eq!(at(2, 10), [0, 0, 255, 255]);
        let json: Value = serde_json::from_slice(&std::fs::read(&json).unwrap()).unwrap();
        let tags = &json["meta"]["frameTags"];
        assert_eq!(
            (tags[0]["name"].as_str(), tags[0]["from"].as_u64(), tags[0]["to"].as_u64()),
            (Some("idle"), Some(0), Some(0))
        );
        assert_eq!(
            (tags[1]["name"].as_str(), tags[1]["from"].as_u64(), tags[1]["to"].as_u64()),
            (Some("walk"), Some(1), Some(2))
        );
        assert_eq!(json["frames"][2]["frame"], json!({ "x": 8, "y": 8, "w": 8, "h": 8 }));
        // playing from walk's frame plays walk's places only
        assert!(s.select_layer(third));
        let sequence = s.animation_sequence();
        let playing: Vec<u32> = s.animation_playing_places().into_iter().map(|p| sequence[p as usize]).collect();
        assert!(playing.iter().all(|&f| f >= 1) && playing.contains(&2), "{playing:?}");
        // renamed, then removed: walk's frames join idle
        assert!(s.rename_animation(third, "run".into()));
        assert_eq!(s.animation_name_of(ids[1]).as_deref(), Some("run"));
        assert!(s.remove_animation(third));
        assert_eq!(s.animation_name_of(third).as_deref(), Some("idle"));
        s.undo();
        assert_eq!(s.animation_name_of(third).as_deref(), Some("run"), "one undo step brings it back");
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

    fn frame(name: &str, duration: u32, colour: [u8; 4], row: u32, column: u32) -> SheetFrame {
        SheetFrame { name: name.into(), duration, row, column, rgba: colour.repeat(2 * 2) }
    }

    #[test]
    fn a_sheet_lays_the_frames_in_a_row_each_pixel_a_square() {
        let frames = [frame("a", 100, [255, 0, 0, 255], 0, 0), frame("b", 200, [0, 0, 255, 128], 0, 1)];
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
        let frames = [frame("Walk 1", 100, [0; 4], 0, 0), frame("", 300, [0; 4], 0, 1)];
        let json = aseprite_json(&frames, &[], 64, 48, "fox sheet.png", 1);
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
