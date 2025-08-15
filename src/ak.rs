use crate::data::write::Writer;
use crate::event::Event;
use clap::{Arg, ArgAction, ArgMatches, Command};
use inquire::{Editor, Select, Text};
use serde::Serialize;
use std::env::var;
use std::fs::create_dir_all;
use std::path::MAIN_SEPARATOR_STR;

pub mod data;
pub mod event;

const AK_USERNAME: &str = "AK_USERNAME";
const AK_EMAIL: &str = "AK_EMAIL";
const EDITOR: &str = "EDITOR";

const COMMIT_TEMPLATE: &str = r#"%type% %summary%

%body%

%author% <%author_email%>

"#;

// Commit object stored as phenomenon "commit" with noumenon = JSON
#[derive(Serialize)]
struct CommitRecord<'a> {
    id: u64,
    parent: Option<u64>,
    ty: &'a str,
    summary: &'a str,
    body: &'a str,
    author: &'a str,
    author_email: &'a str,
    timestamp: u128,
}

fn apps() -> ArgMatches {
    Command::new("ak")
        .about("a new vcs")
        .subcommand(Command::new("init").about("init data"))
        .subcommand(
            Command::new("inscribe")
                .about("track data from a path into the current cube")
                .arg(
                    Arg::new("path")
                        .help("Path to scan (defaults to .)")
                        .required(false)
                        .action(ArgAction::Set),
                ),
        )
        .subcommand(
            Command::new("seal")
                .about("register a commit into the current cube")
                .arg(
                    Arg::new("type")
                        .short('t')
                        .long("type")
                        .help("Commit type")
                        .required(false)
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("summary")
                        .short('s')
                        .long("summary")
                        .help("Commit summary")
                        .required(false)
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("body")
                        .short('b')
                        .long("body")
                        .help("Commit body")
                        .required(false)
                        .action(ArgAction::Set),
                ),
        )
        .subcommand(Command::new("timeline").about("show event timeline (commits)"))
        .subcommand(Command::new("view").about("show the latest commit"))
        .get_matches()
}

// Resolve cube path for the current author by year-month granularity
fn cube_path_for(author: &str) -> String {
    let ym = chrono::Local::now().format("%Y-%m").to_string();
    create_dir_all(format!(
        ".eikyu{MAIN_SEPARATOR_STR}cubes{MAIN_SEPARATOR_STR}{ym}"
    ))
    .expect("create cubes dir failed");
    format!(
        ".eikyu{MAIN_SEPARATOR_STR}cubes{MAIN_SEPARATOR_STR}{ym}{MAIN_SEPARATOR_STR}{author}.cube"
    )
}

// Save a string directly into a cube file under a given "phenomenon" label.
fn save_string_in_cube(cube_path: &str, phenomenon: &str, content: &str) -> std::io::Result<u64> {
    let mut w = Writer::create(cube_path)?;
    let off = w.append(phenomenon, content)?;
    Ok(off)
}

// Read all events (commits) from a cube and return them in order of id.
fn read_commits_from_cube(cube_path: &str) -> std::io::Result<Vec<Event>> {
    let mut w = Writer::create(cube_path)?;
    let idx = w.rebuild_index()?; // id -> offset
    let mut out = Vec::with_capacity(idx.len());
    for (_id, off) in idx {
        let ev = Writer::read_one_at(cube_path, off)?;
        if ev.phenomenon == "commit" {
            out.push(ev);
        }
    }
    Ok(out)
}

// Get the last commit id, if any, from the cube.
fn last_commit_id(cube_path: &str) -> std::io::Result<Option<u64>> {
    let commits = read_commits_from_cube(cube_path)?;
    Ok(commits.last().map(|e| e.id))
}

fn main() {
    let args = apps();
    let author = var(AK_USERNAME).expect("get username failed");
    let author_email = var(AK_EMAIL).expect("get username email failed");

    match args.subcommand() {
        Some(("init", _)) => {
            // Layout:
            // - .eikyu/
            //   - cubes/<YYYY-MM>/<author>.cube
            //   - branches/ (reserved)
            //   - tree/<author> (reserved)
            create_dir_all("./.eikyu").expect("create dir failed");
            create_dir_all(format!(".eikyu{MAIN_SEPARATOR_STR}cubes"))
                .expect("create cubes dir failed");
            create_dir_all(format!(".eikyu{MAIN_SEPARATOR_STR}branches"))
                .expect("create branches dir failed");
            create_dir_all(format!(
                ".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{author}"
            ))
            .expect("create tree dir failed");

            // Ensure the current cube file exists
            let cube = cube_path_for(&author);
            let _ = Writer::create(&cube).expect("failed to initialize cube");
            println!("Initialized repository. Cube: {}", cube);
        }

        Some(("inscribe", sub)) => {
            let target = sub
                .get_one::<String>("path")
                .map(String::as_str)
                .unwrap_or(".");
            let cube = cube_path_for(&author);
            let mut w = Writer::create(&cube).expect("open cube failed");
            w.store_directory(target).expect("store directory failed");
        }

        Some(("seal", sub)) => {
            let editor = var(EDITOR).expect("get editor failed");

            let ty = if let Some(t) = sub.get_one::<String>("type") {
                t.to_owned()
            } else {
                let types = ["feat", "fix", "refactor", "docs", "test", "chore"];
                Select::new("type:", types.to_vec())
                    .prompt()
                    .expect("type prompt failed")
                    .to_string()
            };

            let summary = if let Some(s) = sub.get_one::<String>("summary") {
                s.to_owned()
            } else {
                Text::new("summary:")
                    .prompt()
                    .expect("summary prompt failed")
            };

            let body = if let Some(b) = sub.get_one::<String>("body") {
                b.to_owned()
            } else {
                Editor::new("body:")
                    .with_editor_command(editor.as_ref())
                    .prompt()
                    .expect("body prompt failed")
            };

            // Human friendly commit message (for users/tools that want it)
            let commit_message = COMMIT_TEMPLATE
                .replace("%type%", &ty)
                .replace("%summary%", &summary)
                .replace("%body%", &body)
                .replace("%author%", &author)
                .replace("%author_email%", &author_email);

            let cube = cube_path_for(&author);

            // Compute parent id (if any)
            let parent = last_commit_id(&cube).expect("read last commit failed");

            // Reserve next id by doing a dry append? We want to store JSON including the id.
            // Approach: append once to get assigned id, then rewrite is not supported.
            // Instead, write with phenomenon "commit:pending" to get an id, then store final "commit".
            // Simpler approach: store commit with phenomenon "commit" and then immediately read the last id.
            // We'll append once and then fetch last id to include in the JSON we store as noumenon.

            // Append a placeholder to acquire the id
            let placeholder_off = save_string_in_cube(&cube, "commit:pending", &commit_message)
                .expect("failed to reserve commit id");
            let pending_event =
                Writer::read_one_at(&cube, placeholder_off).expect("failed to read back pending");
            let assigned_id = pending_event.id;

            // Build canonical JSON for the commit
            let record = CommitRecord {
                id: assigned_id,
                parent,
                ty: &ty,
                summary: &summary,
                body: &body,
                author: &author,
                author_email: &author_email,
                timestamp: pending_event.timestamp,
            };
            let json = serde_json::to_string_pretty(&record).expect("serialize commit failed");

            // Store the official commit record
            save_string_in_cube(&cube, "commit", &json).expect("failed to save commit record");

            println!(
                "{} {} (id={} parent={})",
                ty,
                summary,
                assigned_id,
                parent
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
        }

        Some(("timeline", _)) => {
            let cube = cube_path_for(&author);
            let commits = read_commits_from_cube(&cube).expect("read commits failed");
            if commits.is_empty() {
                println!("No commits.");
                return;
            }
            for ev in commits {
                // noumenon is JSON CommitRecord
                match serde_json::from_str::<serde_json::Value>(&ev.noumenon) {
                    Ok(v) => {
                        let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(ev.id);
                        let ts = v.get("timestamp").and_then(|x| x.as_u64()).unwrap_or(0);
                        let ty = v
                            .get("ty")
                            .and_then(|x| x.as_str())
                            .unwrap_or("commit")
                            .to_string();
                        let summary = v
                            .get("summary")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let when = if ts > 0 {
                            // ts is u128 millis; best-effort format
                            let dt = chrono::DateTime::<chrono::Local>::from(
                                std::time::UNIX_EPOCH + std::time::Duration::from_millis(ts),
                            );
                            dt.format("%Y-%m-%d %H:%M:%S").to_string()
                        } else {
                            "-".to_string()
                        };
                        println!("#{id} [{ty}] {summary} @ {when}");
                    }
                    Err(_) => {
                        println!("#{} [commit] <unparsed>", ev.id);
                    }
                }
            }
        }

        Some(("view", _)) => {
            let cube = cube_path_for(&author);
            let commits = read_commits_from_cube(&cube).expect("read commits failed");
            if let Some(ev) = commits.last() {
                match serde_json::from_str::<serde_json::Value>(&ev.noumenon) {
                    Ok(v) => {
                        let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(ev.id);
                        let ty = v.get("ty").and_then(|x| x.as_str()).unwrap_or("commit");
                        let summary = v.get("summary").and_then(|x| x.as_str()).unwrap_or("");
                        println!("#{id} [{ty}] {summary}");
                    }
                    Err(_) => println!("#{} [commit]", ev.id),
                }
            } else {
                println!("No commits.");
            }
        }

        _ => {
            println!("unknown command");
        }
    }
}
