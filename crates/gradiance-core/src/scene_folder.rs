//! The folder of the open scene file (brief S-13), where a body texture's
//! relative path resolves. The persist layer writes it on every save and
//! load; the render layer reads it.

use bevy::prelude::Resource;
use std::path::{Path, PathBuf};

/// The open scene file's folder; `None` before the first save or load.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneFolder(pub Option<PathBuf>);

impl SceneFolder {
    /// Where `path` is: as it is when absolute, else under the scene
    /// folder, else under the working directory.
    #[must_use]
    pub fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        match &self.0 {
            Some(folder) if p.is_relative() => folder.join(p),
            _ => p.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_resolves_under_the_scene_folder() {
        let folder = SceneFolder(Some(PathBuf::from("scenes")));
        assert_eq!(folder.resolve("a.png"), PathBuf::from("scenes/a.png"));
        assert_eq!(SceneFolder(None).resolve("a.png"), PathBuf::from("a.png"));
        let absolute = if cfg!(windows) { "C:\\a.png" } else { "/a.png" };
        assert_eq!(folder.resolve(absolute), PathBuf::from(absolute));
    }
}
