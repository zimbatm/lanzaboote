use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::{DirEntry, WalkDir};

/// Keeps track of the garbage collection roots.
///
/// The internal HashSet contains all the paths still in use. These paths
/// are used to find all **unused** paths and delete them.
#[derive(Debug)]
pub struct Roots(HashSet<PathBuf>);

impl Roots {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn extend<'a>(&mut self, other: impl Iterator<Item = &'a PathBuf>) {
        self.0.extend(other.cloned().into_iter());
    }

    fn in_use(&self, entry: Option<&DirEntry>) -> bool {
        match entry {
            Some(e) => self.0.contains(e.path()),
            None => false,
        }
    }

    pub fn collect_garbage(&self, directory: impl AsRef<Path>) -> Result<()> {
        // Find all the paths not used anymore.
        let entries_not_in_use = WalkDir::new(directory.as_ref())
            .into_iter()
            .filter(|e| !self.in_use(e.as_ref().ok()));

        // Remove all entries not in use.
        for e in entries_not_in_use {
            let entry = e?;
            let path = entry.path();
            println!("'{}' not in use anymore. Removing...", path.display());

            if path.is_dir() {
                // If a directory is marked as unused all its children can be deleted too.
                fs::remove_dir_all(path)
                    .with_context(|| format!("Failed to remove directory: {:?}", path))?;
            } else {
                // Ignore failing to remove path because the parent directory might have been removed before.
                fs::remove_file(path).ok();
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_file(path: PathBuf) -> Result<PathBuf> {
        fs::File::create(&path)?;
        Ok(path)
    }

    fn create_dir(path: PathBuf) -> Result<PathBuf> {
        fs::create_dir(&path)?;
        Ok(path)
    }

    #[test]
    fn keep_root_file() -> Result<()> {
        let tmpdir = tempfile::tempdir()?;
        let rootdir = create_dir(tmpdir.path().join("root"))?;

        let root_file = create_file(rootdir.join("root_file"))?;

        let mut roots = Roots::new();
        roots.extend(vec![&rootdir, &root_file].into_iter());

        roots.collect_garbage(&rootdir)?;

        assert!(root_file.exists());
        Ok(())
    }

    #[test]
    fn delete_unused_file() -> Result<()> {
        let tmpdir = tempfile::tempdir()?;
        let rootdir = create_dir(tmpdir.path().join("root"))?;

        let unused_file = create_file(rootdir.join("unused_file"))?;

        let mut roots = Roots::new();
        roots.extend(vec![&rootdir].into_iter());

        roots.collect_garbage(&rootdir)?;

        assert!(!unused_file.exists());
        Ok(())
    }

    #[test]
    fn delete_unused_directory() -> Result<()> {
        let tmpdir = tempfile::tempdir()?;
        let rootdir = create_dir(tmpdir.path().join("root"))?;

        let unused_directory = create_dir(rootdir.join("unused_directory"))?;
        let unused_file_in_directory =
            create_file(unused_directory.join("unused_file_in_directory"))?;

        let mut roots = Roots::new();
        roots.extend(vec![&rootdir].into_iter());

        roots.collect_garbage(&rootdir)?;

        assert!(!unused_directory.exists());
        assert!(!unused_file_in_directory.exists());
        Ok(())
    }
}
