//! Design templates for dashboard pages (Phase 10 D5): a PNG or JPEG picture, with the slots the owner drew over it. The picture is
//! made smaller (long side at most 2,000 px), kept as a PNG of at most 3 MB, and stored twice: in `portable_data/dashboard_templates/
//! <name>/` (`design.png` + `slots.json`) and as a copy inside the project with its blake3, so a project moved to another computer still
//! shows its design. A design never recolours a chart: it is only a background.

use crate::dashboard::*;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The long side of a stored design, in pixels.
pub const MAX_SIDE_PX: u32 = 2000;
/// The largest a stored design may be.
pub const MAX_BYTES: usize = 3 * 1024 * 1024;
pub const NOT_A_PICTURE: &str = "This file is not a PNG or JPEG picture.";
/// The folder of the designs inside the data folder.
pub const FOLDER: &str = "dashboard_templates";

/// A picture ready to be stored: a PNG no longer than `MAX_SIDE_PX`, with its size and blake3.
#[derive(Clone, Debug, PartialEq)]
pub struct Design {
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub blake3: String,
    /// What to tell the owner (a picture that is not 16:10 is stretched to the page).
    pub note: Option<String>,
}

fn is_png_or_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// Reads a PNG or JPEG from `bytes`: made smaller when its long side is over 2,000 px, written again as a PNG, refused when that is over 3 MB.
pub fn design_from_bytes(bytes: &[u8]) -> Result<Design> {
    if !is_png_or_jpeg(bytes) {
        bail!("{}", NOT_A_PICTURE);
    }
    let img = image::load_from_memory(bytes).map_err(|_| anyhow::anyhow!("{} It could not be read; export it again from the design program.", NOT_A_PICTURE))?;
    let img = if img.width().max(img.height()) > MAX_SIDE_PX { img.resize(MAX_SIDE_PX, MAX_SIDE_PX, image::imageops::FilterType::Lanczos3) } else { img };
    let rgba = img.to_rgba8();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(rgba.clone()).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).context("The picture could not be written as a PNG.")?;
    if png.len() > MAX_BYTES {
        bail!("This picture is {:.1} MB after it was made smaller; the limit is 3 MB. Export it with fewer colours or at a smaller size.", png.len() as f64 / 1_048_576.0);
    }
    let (w, h) = (rgba.width(), rgba.height());
    let ratio = w as f64 / h as f64;
    let page = PAGE_W / PAGE_H;
    let note = ((ratio - page).abs() / page > 0.02).then(|| format!("This picture is {}\u{d7}{} px. Pages are 16:10, so it is stretched to fit; export it at 1920\u{d7}1200 for an exact fit.", w, h));
    let blake3 = blake3::hash(&png).to_hex().to_string();
    Ok(Design { png, w, h, blake3, note })
}

/// `design_from_bytes` of a file.
pub fn design_from_file(path: &Path) -> Result<Design> {
    let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("The file '{}' could not be read: {}", path.display(), e))?;
    design_from_bytes(&bytes)
}

/// The slots must be inside the page (the picture is stretched over it), named, and have different names.
pub fn check_slots(slots: &[Slot]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for s in slots {
        if s.name.trim().is_empty() {
            bail!("Every slot needs a name.");
        }
        if !seen.insert(s.name.as_str()) {
            bail!("Two slots are named '{}'. Give each slot its own name.", s.name);
        }
        let r = s.rect;
        if r.x < -1e-9 || r.y < -1e-9 || r.right() > PAGE_W + 1e-9 || r.bottom() > PAGE_H + 1e-9 || r.w < MIN_ITEM_W || r.h < MIN_ITEM_H {
            bail!("The slot '{}' is outside the picture or too small. Draw it inside the picture, at least {} \u{d7} {} units.", s.name, MIN_ITEM_W, MIN_ITEM_H);
        }
    }
    Ok(())
}

/// A design to keep in the project: the picture as text (base64), its hash and its slots.
pub fn project_copy(name: &str, d: &Design, slots: &[Slot]) -> TemplateData {
    TemplateData { name: name.to_string(), blake3: d.blake3.clone(), w: d.w, h: d.h, png_b64: base64_encode(&d.png), slots: slots.to_vec() }
}

/// The folder of one design in the data folder.
pub fn folder_of(data_dir: &Path, name: &str) -> PathBuf {
    data_dir.join(FOLDER).join(name)
}

fn safe_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
        bail!("The template name '{}' cannot be used as a folder name. Use letters, digits, spaces and dashes.", name);
    }
    Ok(())
}

/// Saves the design in the data folder: `design.png` and `slots.json`.
pub fn save_to_folder(data_dir: &Path, name: &str, d: &Design, slots: &[Slot]) -> Result<()> {
    safe_name(name)?;
    check_slots(slots)?;
    let dir = folder_of(data_dir, name);
    std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("The folder '{}' could not be made: {}", dir.display(), e))?;
    std::fs::write(dir.join("design.png"), &d.png).map_err(|e| anyhow::anyhow!("The design could not be saved: {}", e))?;
    std::fs::write(dir.join("slots.json"), serde_json::to_string_pretty(slots)?).map_err(|e| anyhow::anyhow!("The slots could not be saved: {}", e))?;
    Ok(())
}

/// Where a template was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    DataFolder,
    ProjectCopy,
}

#[derive(Clone, Debug)]
pub struct Resolved {
    pub png: Vec<u8>,
    pub slots: Vec<Slot>,
    pub blake3: String,
    pub from: Origin,
    /// What to tell the owner (the copy of this computer differs from the project's).
    pub warning: Option<String>,
}

/// The design `name` for a dashboard whose copy is `copy`: the data folder's, when it has the template and its hash is the one the
/// project recorded; the project's own copy when the folder has no such template. A folder copy with another hash is reported, and the
/// project's copy is used, so the page looks as it was made.
pub fn resolve(data_dir: &Path, name: &str, copy: Option<&TemplateData>) -> Result<Resolved> {
    let dir = folder_of(data_dir, name);
    let png_path = dir.join("design.png");
    let from_project = |warning: Option<String>| -> Result<Resolved> {
        let c = copy.ok_or_else(|| anyhow::anyhow!("The template '{}' is not in this project and not in the data folder. Import it again.", name))?;
        let png = base64_decode(&c.png_b64).map_err(|e| anyhow::anyhow!("The picture kept in the project for the template '{}' is damaged: {}", name, e))?;
        if blake3::hash(&png).to_hex().as_str() != c.blake3 {
            bail!("The picture kept in the project for the template '{}' is damaged (its hash is not the recorded one). Import the template again.", name);
        }
        Ok(Resolved { png, slots: c.slots.clone(), blake3: c.blake3.clone(), from: Origin::ProjectCopy, warning })
    };
    if !png_path.exists() {
        return from_project(None);
    }
    let png = std::fs::read(&png_path).map_err(|e| anyhow::anyhow!("The design '{}' could not be read: {}", png_path.display(), e))?;
    let hash = blake3::hash(&png).to_hex().to_string();
    if let Some(c) = copy {
        if c.blake3 != hash {
            return from_project(Some(format!("The template '{}' on this computer is not the one this dashboard was made with (its hash differs). The copy kept in the project is used.", name)));
        }
    }
    let slots: Vec<Slot> = serde_json::from_str(&std::fs::read_to_string(dir.join("slots.json")).map_err(|e| anyhow::anyhow!("The slots of '{}' could not be read: {}", name, e))?).map_err(|e| anyhow::anyhow!("The slots of '{}' are not readable: {}", name, e))?;
    Ok(Resolved { png, slots, blake3: hash, from: Origin::DataFolder, warning: None })
}

/// The names of the templates in the data folder, in order.
pub fn list_folder(data_dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(data_dir.join(FOLDER)).map(|d| d.flatten().filter(|e| e.path().join("design.png").exists()).filter_map(|e| e.file_name().into_string().ok()).collect()).unwrap_or_default();
    v.sort();
    v
}

// ------------------------------------------------------------------------------------------------ base64 (no new crate)

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn base64_decode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buf = [0u32; 4];
    let mut n = 0;
    let mut pad = 0;
    for ch in text.bytes().filter(|b| !b.is_ascii_whitespace()) {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                pad += 1;
                0
            }
            _ => bail!("a character that is not base64"),
        };
        buf[n] = v as u32;
        n += 1;
        if n == 4 {
            let x = buf[0] << 18 | buf[1] << 12 | buf[2] << 6 | buf[3];
            out.push((x >> 16) as u8);
            if pad < 2 {
                out.push((x >> 8) as u8);
            }
            if pad < 1 {
                out.push(x as u8);
            }
            n = 0;
        }
    }
    if n != 0 {
        bail!("the text ends in the middle of a group");
    }
    Ok(out)
}
