//! A shared scene as one file (brief S-18): a ZIP archive with the scene
//! RON at its root, its thumbnail, and every image its bodies show.
//!
//! Layout: `scene.ron`, `thumbnail.png`, `images/<file>`. Each body
//! texture's path is rewritten to its `images/` entry, so the RON inside is
//! an ordinary scene whose images resolve beside it. RON stays the one
//! scene format; the archive is only a container.
//!
//! The archive is written **stored** (no compression) by the small writer
//! here, with its CRC-32s, and read back by the reader here, which takes
//! stored entries only (decision `share-bundle-stored-zip`): no archive
//! crate is in the lockfile, the PNGs inside are compressed already, and
//! any unzip tool opens the file. Opening a bundle extracts its images to
//! a folder under the temp directory and makes that the scene folder.

use crate::browser::thumbnail_path;
use gradiance_core::scene_folder::SceneFolder;
use gradiance_scene::{PersistError, SceneRecord, from_ron, to_ron};
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

/// The scene's entry in a bundle.
pub const SCENE_ENTRY: &str = "scene.ron";
/// The thumbnail's entry in a bundle.
pub const THUMBNAIL_ENTRY: &str = "thumbnail.png";
/// The folder of the images in a bundle.
pub const IMAGES_DIR: &str = "images";

/// Whether `path` names a bundle (ends in `.zip`).
#[must_use]
pub fn is_bundle(path: &Path) -> bool {
    crate::ops::has_extension(path, "zip")
}

/// A bundle error: the archive is not one this reader takes.
fn invalid(why: impl Into<String>) -> PersistError {
    PersistError::Io(Error::new(ErrorKind::InvalidData, why.into()))
}

/// The bundle of `scene`: its RON with every texture path pointing into
/// `images/`, its thumbnail, and the image files, read through `folder`
/// (the scene folder relative paths resolve in). An image that cannot be
/// read keeps its path and stays out of the bundle; the returned list
/// names those paths.
///
/// # Errors
///
/// The scene does not serialize.
pub fn share(
    scene: &SceneRecord,
    folder: &SceneFolder,
) -> Result<(Vec<u8>, Vec<String>), PersistError> {
    let mut scene = scene.clone();
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    let mut entry_of: HashMap<String, String> = HashMap::new();
    let mut missing = Vec::new();
    for body in &mut scene.bodies {
        let Some(texture) = body.texture.as_mut() else {
            continue;
        };
        if let Some(entry) = entry_of.get(&texture.path) {
            texture.path.clone_from(entry);
            continue;
        }
        let Ok(bytes) = std::fs::read(folder.resolve(&texture.path)) else {
            missing.push(texture.path.clone());
            continue;
        };
        let entry = image_entry(&texture.path, &images);
        entry_of.insert(texture.path.clone(), entry.clone());
        images.push((entry.clone(), bytes));
        texture.path = entry;
    }
    let ron = to_ron(&scene)?;
    let thumbnail = crate::browser::thumbnail_png(&scene).map_err(invalid)?;
    let mut entries: Vec<(&str, &[u8])> = vec![
        (SCENE_ENTRY, ron.as_bytes()),
        (THUMBNAIL_ENTRY, thumbnail.as_slice()),
    ];
    entries.extend(images.iter().map(|(n, b)| (n.as_str(), b.as_slice())));
    Ok((write_zip(&entries), missing))
}

/// A unique `images/` entry for the image at `path`: its file name, with a
/// number added when another image took that name.
fn image_entry(path: &str, taken: &[(String, Vec<u8>)]) -> String {
    let p = Path::new(path);
    let stem = p
        .file_stem()
        .map_or_else(|| "image".into(), |s| s.to_string_lossy().into_owned());
    let ext = p
        .extension()
        .map_or_else(|| "png".into(), |s| s.to_string_lossy().into_owned());
    // One of `taken.len() + 1` names is free.
    (0..=taken.len())
        .map(|n| {
            if n == 0 {
                format!("{IMAGES_DIR}/{stem}.{ext}")
            } else {
                format!("{IMAGES_DIR}/{stem}-{n}.{ext}")
            }
        })
        .find(|name| taken.iter().all(|(t, _)| t != name))
        .unwrap_or_else(|| format!("{IMAGES_DIR}/{stem}.{ext}"))
}

/// The scene in the bundle `bytes`, with its images written under `into`
/// (whose `images/` the scene's texture paths name).
///
/// # Errors
///
/// Not a bundle this reader takes, no `scene.ron`, a scene that does not
/// parse, or a disk error.
pub fn open(bytes: &[u8], into: &Path) -> Result<SceneRecord, PersistError> {
    let entries = read_zip(bytes)?;
    let scene = parse(&entries)?;
    for (name, data) in &entries {
        let Some(file) = name.strip_prefix(&format!("{IMAGES_DIR}/")) else {
            continue;
        };
        // Only plain file names: an entry never writes outside `into`.
        if file.is_empty() || file.contains(['/', '\\']) || file == ".." {
            continue;
        }
        let dir = into.join(IMAGES_DIR);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(file), data)?;
    }
    if let Some((_, png)) = entries.iter().find(|(n, _)| n == THUMBNAIL_ENTRY) {
        std::fs::create_dir_all(into)?;
        std::fs::write(thumbnail_path(&into.join(SCENE_ENTRY)), png)?;
    }
    Ok(scene)
}

/// The scene in a bundle's entries, without touching the disk.
///
/// # Errors
///
/// No `scene.ron`, or one that does not parse.
pub fn parse(entries: &[(String, Vec<u8>)]) -> Result<SceneRecord, PersistError> {
    let (_, ron) = entries
        .iter()
        .find(|(n, _)| n == SCENE_ENTRY)
        .ok_or_else(|| invalid(format!("no {SCENE_ENTRY} in the bundle")))?;
    let text = std::str::from_utf8(ron).map_err(|e| invalid(e.to_string()))?;
    from_ron(text)
}

/// Where a bundle's images go when it opens: a folder under the temp
/// directory, named by the bundle and its content (so two different
/// bundles of one name never share it).
#[must_use]
pub fn extract_dir(bundle: &Path, bytes: &[u8]) -> PathBuf {
    let stem = bundle
        .file_stem()
        .map_or_else(|| "bundle".into(), |s| s.to_string_lossy().into_owned());
    std::env::temp_dir()
        .join("gradiance-bundles")
        .join(format!("{stem}-{:08x}", crc32(bytes)))
}

// ---------------------------------------------------------------------------
// The stored-ZIP writer and reader (PKWARE APPNOTE 4.3: local headers, the
// central directory, the end record; method 0, no data descriptors).

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const END_SIG: u32 = 0x0605_4b50;
/// Version 2.0: what a stored entry needs.
const VERSION: u16 = 20;
/// Bit 11: names are UTF-8.
const UTF8_FLAG: u16 = 1 << 11;

/// The CRC-32 (IEEE, reflected) of `data`.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A stored ZIP of `entries` (name, bytes), in order.
///
/// # Panics
///
/// Never in practice: an entry or the archive past 4 GiB (ZIP64) is out of
/// this format's range, and a scene bundle is far smaller.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "a scene bundle is far below the 4 GiB a non-ZIP64 archive holds"
)]
pub fn write_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_len = name.len() as u16;
        put32(&mut out, LOCAL_SIG);
        put16(&mut out, VERSION);
        put16(&mut out, UTF8_FLAG);
        put16(&mut out, 0); // stored
        put32(&mut out, 0); // time and date: 1980, none
        put32(&mut out, crc);
        put32(&mut out, size);
        put32(&mut out, size);
        put16(&mut out, name_len);
        put16(&mut out, 0); // no extra field
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        put32(&mut central, CENTRAL_SIG);
        put16(&mut central, VERSION);
        put16(&mut central, VERSION);
        put16(&mut central, UTF8_FLAG);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, crc);
        put32(&mut central, size);
        put32(&mut central, size);
        put16(&mut central, name_len);
        put16(&mut central, 0); // extra
        put16(&mut central, 0); // comment
        put16(&mut central, 0); // disk
        put16(&mut central, 0); // internal attributes
        put32(&mut central, 0); // external attributes
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let central_at = out.len() as u32;
    let central_len = central.len() as u32;
    out.extend_from_slice(&central);
    let count = entries.len() as u16;
    put32(&mut out, END_SIG);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, count);
    put16(&mut out, count);
    put32(&mut out, central_len);
    put32(&mut out, central_at);
    put16(&mut out, 0); // no comment
    out
}

fn get16(b: &[u8], at: usize) -> Result<u16, PersistError> {
    b.get(at..at + 2)
        .and_then(|s| s.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| invalid("the archive ends early"))
}

fn get32(b: &[u8], at: usize) -> Result<u32, PersistError> {
    b.get(at..at + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| invalid("the archive ends early"))
}

/// The entries (name, bytes) of a ZIP archive whose entries are stored.
///
/// # Errors
///
/// Not a ZIP, a compressed or encrypted entry, or a CRC that does not
/// match.
pub fn read_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, PersistError> {
    // The end record is the last 22 bytes, before a comment of at most
    // 64 KiB.
    let end = (0..=bytes.len().saturating_sub(22))
        .rev()
        .take(65_536 + 1)
        .find(|&i| get32(bytes, i).is_ok_and(|s| s == END_SIG))
        .ok_or_else(|| invalid("not a ZIP archive"))?;
    let count = usize::from(get16(bytes, end + 10)?);
    let mut at = get32(bytes, end + 16)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if get32(bytes, at)? != CENTRAL_SIG {
            return Err(invalid("a damaged central directory"));
        }
        let flags = get16(bytes, at + 8)?;
        let method = get16(bytes, at + 10)?;
        let crc = get32(bytes, at + 16)?;
        let size = get32(bytes, at + 20)? as usize;
        let name_len = usize::from(get16(bytes, at + 28)?);
        let extra_len = usize::from(get16(bytes, at + 30)?);
        let comment_len = usize::from(get16(bytes, at + 32)?);
        let local = get32(bytes, at + 42)? as usize;
        let name = bytes
            .get(at + 46..at + 46 + name_len)
            .ok_or_else(|| invalid("the archive ends early"))?;
        let name = String::from_utf8_lossy(name).into_owned();
        if flags & 1 != 0 {
            return Err(invalid(format!("`{name}` is encrypted")));
        }
        if method != 0 {
            return Err(invalid(format!(
                "`{name}` is compressed: unzip the bundle and open its {SCENE_ENTRY}"
            )));
        }
        if get32(bytes, local)? != LOCAL_SIG {
            return Err(invalid(format!("`{name}`: a damaged local header")));
        }
        let data_at = local
            + 30
            + usize::from(get16(bytes, local + 26)?)
            + usize::from(get16(bytes, local + 28)?);
        let data = bytes
            .get(data_at..data_at + size)
            .ok_or_else(|| invalid("the archive ends early"))?;
        if crc32(data) != crc {
            return Err(invalid(format!("`{name}`: the CRC does not match")));
        }
        if !name.ends_with('/') {
            entries.push((name, data.to_vec()));
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crc_is_the_ieee_one() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn an_archive_reads_back_what_was_written() {
        let zip = write_zip(&[("a.txt", b"hello"), ("images/b.png", &[0, 1, 2, 255])]);
        let entries = read_zip(&zip).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            entries,
            vec![
                ("a.txt".to_owned(), b"hello".to_vec()),
                ("images/b.png".to_owned(), vec![0, 1, 2, 255]),
            ]
        );
    }

    #[test]
    fn a_damaged_archive_is_refused() {
        let mut zip = write_zip(&[("a.txt", b"hello")]);
        assert!(read_zip(&zip[..10]).is_err(), "cut short");
        // Flip a data byte: the CRC no longer matches.
        let at = 30 + "a.txt".len();
        zip[at] ^= 0xFF;
        assert!(read_zip(&zip).is_err(), "bad CRC");
        assert!(read_zip(b"not a zip at all, just some text").is_err());
    }

    #[test]
    fn image_names_stay_unique() {
        let taken = vec![("images/a.png".to_owned(), Vec::new())];
        assert_eq!(image_entry("x/a.png", &taken), "images/a-1.png");
        assert_eq!(image_entry("/abs/c.png", &taken), "images/c.png");
    }
}
