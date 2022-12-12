use std::path::Path;

// For now we assume that all paths are valid UTF-8 and can be converted to Strings without error.
pub fn path_to_string(path: impl AsRef<Path>) -> String {
    String::from(path.as_ref().to_str().unwrap_or_else(|| {
        panic!(
            "Failed to convert path '{}' to a string",
            path.as_ref().display()
        )
    }))
}
