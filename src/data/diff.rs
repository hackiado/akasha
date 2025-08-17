//! Repository vs. stored tree diff utility.
//!
//! Compares the current working directory against a previously captured snapshot
//! stored under `.eikyu/tree/<AK_USERNAME>` and prints a concise, colorized summary:
//! - Green “+” for files added in the repository (not present in the stored tree)
//! - Red “-” for files removed from the repository (present only in the stored tree)
//! - Yellow “~” for files modified. For UTF‑8 text files, a unified line diff is shown;
//!   for binaries or invalid UTF‑8, a single “(modified binary)” marker is printed.
//!
//! This command is read‑only and does not modify the repository or the stored tree.

use colored::Colorize;
use std::env::{current_dir, var};
use std::io;
use std::path::{MAIN_SEPARATOR_STR, Path};
use std::process::ExitCode;

/// Compare the current repository state against the last stored tree snapshot and print differences.
///
/// Flow:
/// 1) Locate repository root (current_dir) and resolve the tree snapshot path using `AK_USERNAME`.
/// 2) Enumerate files (relative paths) for both the repository and the stored tree,
///    excluding `.eikyu/` from the repository listing and applying `.ignore` rules.
/// 3) Compute set differences:
///    - Added: present in repo only
///    - Removed: present in tree only
///    - Modified: present in both but with different content
/// 4) For modified files:
///    - If both sides are valid UTF‑8, print a line-by-line diff
///    - Otherwise, print a “modified binary” marker
///
/// Returns:
/// - ExitCode::SUCCESS on success
/// - ExitCode::FAILURE if the snapshot is missing or enumeration fails
pub fn diff() -> ExitCode {
    // Determine repository root and author (used to address the stored tree).
    let repository_root = current_dir().expect("Failed to get current directory");
    let auteur = var("AK_USERNAME").expect("Failed to get auteur");

    // Stored tree layout: .eikyu/tree/<AK_USERNAME>
    let tree_dir = repository_root.join(format!(
        ".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{auteur}"
    ));

    // Early exit if there is no stored snapshot yet.
    if !tree_dir.exists() {
        eprintln!("No stored tree found at: {}", tree_dir.to_string_lossy());
        eprintln!("Tip: run a command that creates the tree snapshot first.");
        return ExitCode::FAILURE;
    }

    // Enumerate repo files as relative paths, sorted for stable output.
    let repo_list = match collect_files(&repository_root) {
        Ok(mut v) => {
            v.sort();
            v
        }
        Err(e) => {
            eprintln!("Failed to enumerate repository files: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Enumerate stored tree files as relative paths, sorted for stable output.
    let tree_list = match collect_files(&tree_dir) {
        Ok(mut v) => {
            v.sort();
            v
        }
        Err(e) => {
            eprintln!("Failed to enumerate stored tree files: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Instead of comparing only manifests, compute a 3-way classification:
    // - additions (repo only)
    // - deletions (tree only)
    // - modifications (present on both sides but content differs)
    use std::collections::HashSet;
    use std::fs;

    let repo_set: HashSet<_> = repo_list.iter().cloned().collect();
    let tree_set: HashSet<_> = tree_list.iter().cloned().collect();

    // Added files (present in repo, absent in tree).
    for path in repo_set.difference(&tree_set) {
        println!("{} {} {}", "+".green().bold(), path, "".normal());
    }
    // Removed files (present in tree, absent in repo).
    for path in tree_set.difference(&repo_set) {
        println!("{} {} {}", "-".red().bold(), path, "".normal());
    }

    // Potentially modified files (present on both sides).
    for path in repo_set.intersection(&tree_set) {
        let repo_p = repository_root.join(path);
        let tree_p = tree_dir.join(path);

        // Compare raw bytes first; if different, attempt a line-oriented diff for UTF‑8 text.
        let repo_bytes = match fs::read(&repo_p) {
            Ok(b) => b,
            Err(_) => continue, // Skip unreadable files; report is best-effort
        };
        let tree_bytes = match fs::read(&tree_p) {
            Ok(b) => b,
            Err(_) => continue,
        };

        if repo_bytes != tree_bytes {
            match (
                std::str::from_utf8(&tree_bytes),
                std::str::from_utf8(&repo_bytes),
            ) {
                // Text diff for UTF‑8 on both sides.
                (Ok(left), Ok(right)) => {
                    println!("\n{} {}", "diff:".yellow().bold(), path);
                    for d in diff::lines(left, right) {
                        match d {
                            diff::Result::Left(line) => {
                                println!("{} {}", "-".red().bold(), line.red());
                            }
                            diff::Result::Right(line) => {
                                println!("{} {}", "+".green().bold(), line.green());
                            }
                            diff::Result::Both(line, _) => {
                                // Optionally display unchanged context as dimmed text.
                                // Keeping output concise by default.
                                let _ = line;
                            }
                        }
                    }
                }
                // Non-text or invalid UTF‑8: mark as modified binary.
                _ => {
                    println!(
                        "{} {} {}",
                        "~".yellow().bold(),
                        path,
                        "(modified binary)".yellow()
                    );
                }
            }
        }
    }
    ExitCode::SUCCESS
}

/// Recursively collect all file paths under `root` and return them as relative strings.
///
/// Behavior:
/// - Uses `ignore::WalkBuilder` with support for a custom `.ignore` file.
/// - Filters to include regular files only.
/// - Produces paths relative to `root`.
/// - Skips the internal working directory prefix `.eikyu/` when scanning the repository root,
///   so that internal state does not pollute the diff output.
///
/// Returns:
/// - Ok(Vec<String>) sorted by the caller for stable output
/// - Err(io::Error) if traversal cannot be constructed or read
fn collect_files(root: &Path) -> io::Result<Vec<String>> {
    // Precompute a path prefix that will be stripped to create relative paths.
    let dir = format!("{}{}", root.display(), MAIN_SEPARATOR_STR);
    let mut out = Vec::new();

    ignore::WalkBuilder::new(root)
        .add_custom_ignore_filename(".ignore")
        .build()
        .filter(Result::is_ok)
        .filter(|f| {
            f.clone()
                .expect("walker item should be Ok after filter")
                .file_type()
                .expect("failed to get file type")
                .is_file()
        })
        .for_each(|a| {
            let x = a.expect("failed to get file");

            if x.path().is_file() {
                // Make the file path relative to `root`.
                let rel = x.path().display().to_string().replace(dir.as_str(), "");

                // Skip internal working directory entries (e.g., snapshot cache).
                let ignore_prefix = format!(".eikyu{MAIN_SEPARATOR_STR}");
                if rel.starts_with(&ignore_prefix) {
                    return;
                }
                out.push(rel);
            }
        });

    Ok(out)
}
