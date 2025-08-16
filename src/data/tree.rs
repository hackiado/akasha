use std::env::current_dir;
use std::fs;
use std::io;
use std::path::MAIN_SEPARATOR_STR;

/// Updates the reference tree for a given author by mirroring the current working directory.
/// The tree is stored in `.eikyu/tree/{author}`.
/// This function will clean the existing tree and recreate it based on the current state
/// of the repository, respecting ignore files.
pub fn update_tree(author: &str) -> io::Result<()> {
    let root = current_dir()?;
    let tree_dir = root.join(format!(
        ".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{author}"
    ));

    // 1. Clean and recreate the tree directory
    if tree_dir.exists() {
        fs::remove_dir_all(&tree_dir)?;
    }
    fs::create_dir_all(&tree_dir)?;

    // 2. Walk the current directory and copy files to the tree
    // We use the 'ignore' crate to respect .gitignore and .ignore files,
    // ensuring we only copy relevant project files.
    for result in ignore::WalkBuilder::new(&root).build() {
        match result {
            Ok(entry) => {
                let path = entry.path();
                if path.is_dir() {
                    continue; // We only care about files
                }

                // Calculate the destination path inside the tree directory
                if let Ok(relative_path) = path.strip_prefix(&root) {
                    let dest_path = tree_dir.join(relative_path);

                    // Create parent directories if they don't exist
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    // Copy the file
                    fs::copy(path, &dest_path)?;
                }
            }
            Err(err) => eprintln!("ERROR: {}", err),
        }
    }

    Ok(())
}
