use std::env::{current_dir, var};
use std::fs;
use std::io;
use std::path::{Path, PathBuf, MAIN_SEPARATOR_STR};

pub fn diff() {
    let repository_root = current_dir().expect("Failed to get current directory");
    let auteur = var("AK_USERNAME").expect("Failed to get auteur");
    let tree_dir = repository_root.join(format!(".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{auteur}"));

    if !tree_dir.exists() {
        eprintln!(
            "No stored tree found at: {}",
            tree_dir.to_string_lossy()
        );
        eprintln!("Tip: run a command that creates the tree snapshot first.");
        return;
    }

    let repo_list = match collect_files(&repository_root) {
        Ok(mut v) => {
            v.sort();
            v
        }
        Err(e) => {
            eprintln!("Failed to enumerate repository files: {e}");
            return;
        }
    };

    let tree_list = match collect_files(&tree_dir) {
        Ok(mut v) => {
            v.sort();
            v
        }
        Err(e) => {
            eprintln!("Failed to enumerate stored tree files: {e}");
            return;
        }
    };

    // Build comparable, line-based manifests.
    // We print paths relative to the repository root to keep them comparable across both sets.
    let repo_manifest = repo_list.join("\n");
    let tree_manifest = tree_list.join("\n");

    // Use `diff` crate to compute line-level differences of manifests
    // This shows added/removed files between the stored tree and current repository state.
    // Left  => only in repo (new files)
    // Right => only in tree (deleted files from current repo)
    // Both  => same path present in both
    for d in diff::lines(&tree_manifest, &repo_manifest) {
        match d {
            diff::Result::Left(line) => {
                // File present in stored tree but not in working repo -> likely removed
                if !line.is_empty() {
                    println!("- {line}");
                }
            }
            diff::Result::Right(line) => {
                // File present in repo but not in stored tree -> likely added
                if !line.is_empty() {
                    println!("+ {line}");
                }
            }
            diff::Result::Both(_, _) => {
                // Unchanged presence; we don't print to keep the output concise
            }
        }
    }
}

fn collect_files(root: &Path) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(err) => {
                // Skip unreadable directories but continue
                eprintln!("Warning: cannot read {}: {err}", dir.to_string_lossy());
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();

            // Skip our own metadata and VCS directories
            if is_ignored_dir(&path) {
                continue;
            }

            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                if let Some(rel) = to_relative(path.as_path(), root) {
                    // Normalize to forward slashes for stable diff output across platforms
                    out.push(rel.replace('\\', "/"));
                }
            }
        }
    }

    Ok(out)
}

fn is_ignored_dir(p: &Path) -> bool {
    if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
        return name == ".git" || name == ".eikyu" || name == "target";
    }
    false
}

fn to_relative(p: &Path, root: &Path) -> Option<String> {
    let rel = if p.starts_with(root) {
        p.strip_prefix(root).ok()?.to_path_buf()
    } else {
        // Already relative or from a different root; try best-effort
        p.to_path_buf()
    };
    let mut rel_norm = PathBuf::new();
    for comp in rel.components() {
        rel_norm.push(comp);
    }
    rel_norm.to_str().map(|s| s.trim_start_matches(['/','\\']).to_string())
}
