use std::path::Path;

/// Helper to convert a Path to &str, returning an error if not valid UTF-8.
pub fn path_to_str(path: &Path) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid path (not UTF-8)"))
}
