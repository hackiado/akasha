use std::env::current_dir;
use std::fs;
use std::io;
use std::path::MAIN_SEPARATOR_STR;

/// Update or recreate the on-disk snapshot tree for the given `author`.
///
/// Overview:
/// - The snapshot tree is stored under `.eikyu/tree/{author}` relative to the current working directory.
/// - The existing tree (if present) is removed entirely and then rebuilt from the current repository contents.
/// - File enumeration respects standard ignore rules via the `ignore` crate (e.g., `.gitignore`, `.ignore`).
///
/// Behavior and guarantees:
/// - Destructive refresh: the target tree directory is deleted and recreated to mirror the current state.
/// - Only regular files are copied; directories are created on demand to preserve structure.
/// - Paths are replicated relative to the working directory, preserving hierarchy.
/// - Best-effort traversal: errors from walker entries are logged to stderr without aborting the whole operation.
/// - Returns `Ok(())` on success; propagates I/O errors for critical operations (remove, create, copy).
///
/// Notes:
/// - Permissions and timestamps are not preserved; this is a content mirroring step focused on bytes and structure.
/// - Symbolic links are followed according to the default behavior of `ignore::WalkBuilder`.
///   If your use case requires preserving symlinks as symlinks, handle them explicitly.
/// - Large repositories: this operation is O(number_of_files) and copies bytes once per file.
///   Consider incremental strategies if performance becomes a concern.
/// - Internal state: callers may want to ensure `.eikyu/` itself is ignored when building the tree to avoid recursion.
///
/// Errors:
/// - Returns early if removal/creation of the snapshot root fails.
/// - Individual file copy failures cause an early return for that file; traversal continues for other entries.
///
/// Example:
/// - Given current dir `/repo` and `author="alice"`, the snapshot root will be `/repo/.eikyu/tree/alice`.
pub fn update_tree(author: &str) -> io::Result<()> {
    let root = current_dir()?;
    let tree_dir = root.join(format!(
        ".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{author}"
    ));

    // 1) Ensure a clean destination: remove any previous snapshot then recreate the root directory.
    if tree_dir.exists() {
        fs::remove_dir_all(&tree_dir)?;
    }
    fs::create_dir_all(&tree_dir)?;

    // 2) Walk the current working directory and mirror files into the snapshot tree.
    //    `ignore::WalkBuilder` respects .gitignore and .ignore files to avoid copying undesired entries.
    for result in ignore::WalkBuilder::new(&root).build() {
        match result {
            Ok(entry) => {
                let path = entry.path();

                // Skip directories; only mirror regular files.
                if path.is_dir() {
                    continue;
                }

                // Compute the relative path with respect to `root` to preserve structure.
                if let Ok(relative_path) = path.strip_prefix(&root) {
                    let dest_path = tree_dir.join(relative_path);

                    // Ensure parent directories exist before copying the file.
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    // Copy file bytes to the destination. Overwrites any existing file at the location.
                    fs::copy(path, &dest_path)?;
                }
            }
            // Non-fatal: log walker errors and continue. This avoids failing the whole operation for a single entry.
            Err(err) => eprintln!("ERROR: {}", err),
        }
    }

    Ok(())
}