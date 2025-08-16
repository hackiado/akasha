use colored::Colorize;
use std::env::{current_dir, var};
use std::io;
use std::path::{MAIN_SEPARATOR_STR, Path};

pub fn diff() {
    let repository_root = current_dir().expect("Failed to get current directory");
    let auteur = var("AK_USERNAME").expect("Failed to get auteur");
    let tree_dir = repository_root.join(format!(
        ".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{auteur}"
    ));

    if !tree_dir.exists() {
        eprintln!("No stored tree found at: {}", tree_dir.to_string_lossy());
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

    // Au lieu de comparer uniquement les manifestes, calculons:
    // - Ajouts (dans repo mais pas dans tree)
    // - Suppressions (dans tree mais pas dans repo)
    // - Modifications (présents des deux côtés mais contenu différent)
    use std::collections::HashSet;
    use std::fs;

    let repo_set: HashSet<_> = repo_list.iter().cloned().collect();
    let tree_set: HashSet<_> = tree_list.iter().cloned().collect();

    // Fichiers ajoutés
    for path in repo_set.difference(&tree_set) {
        println!("{} {} {}", "+".green().bold(), path, "".normal());
    }
    // Fichiers supprimés
    for path in tree_set.difference(&repo_set) {
        println!("{} {} {}", "-".red().bold(), path, "".normal());
    }

    // Fichiers potentiellement modifiés (présents des deux côtés)
    for path in repo_set.intersection(&tree_set) {
        let repo_p = repository_root.join(path);
        let tree_p = tree_dir.join(path);

        // Compare par octets; si différent, tente un diff texte
        let repo_bytes = match fs::read(&repo_p) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let tree_bytes = match fs::read(&tree_p) {
            Ok(b) => b,
            Err(_) => continue,
        };

        if repo_bytes != tree_bytes {
            // Essaie diff texte si UTF-8, sinon marque comme modifié
            match (
                std::str::from_utf8(&tree_bytes),
                std::str::from_utf8(&repo_bytes),
            ) {
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
                                // Optionnel: afficher le contexte inchangé en gris clair
                                // println!("  {}", line.dimmed());
                                let _ = line; // ne rien afficher pour rester concis
                            }
                        }
                    }
                }
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
}

fn collect_files(root: &Path) -> io::Result<Vec<String>> {
    let dir = format!("{}{}", root.display(), MAIN_SEPARATOR_STR);
    let mut out = Vec::new();
    ignore::WalkBuilder::new(root)
        .add_custom_ignore_filename(".ignore")
        .build()
        .filter(Result::is_ok)
        .filter(|f| {
            f.clone()
                .expect("")
                .file_type()
                .expect("failed to get file type")
                .is_file()
        })
        .into_iter()
        .for_each(|a| {
            let x = a.expect("failed to get file");
            if x.path().is_file() {
                let rel = x.path().display().to_string().replace(dir.as_str(), "");
                // Ignore le dossier interne de travail pour l'énumération du repo
                let ignore_prefix = format!(".eikyu{MAIN_SEPARATOR_STR}");
                if rel.starts_with(&ignore_prefix) {
                    return;
                }
                out.push(rel);
            }
        });
    Ok(out)
}
