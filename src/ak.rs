use crate::data::write::Writer;
use crate::event::Event;
use chrono::DateTime;
use clap::{Arg, ArgAction, ArgMatches, Command};
use inquire::{Editor, Select, Text};
use serde::Serialize;
use std::collections::HashMap;
use std::env::var;
use std::fs::{create_dir_all, read_to_string};
use std::io::Error;
use std::path::{MAIN_SEPARATOR_STR, Path};
use std::process::ExitCode;

pub mod data;
pub mod event;
use crate::data::diff;
use crate::data::tree;

const AK_USERNAME: &str = "AK_USERNAME";
const AK_EMAIL: &str = "AK_EMAIL";
const EDITOR: &str = "EDITOR";

const COMMIT_TEMPLATE: &str = r#"%type% %summary%

%body%

%author% <%author_email%>

"#;

#[derive(Default)]
pub struct PreCommit {
    pub tasks: HashMap<String, HashMap<String, String>>,
}

impl PreCommit {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }
    pub fn add_task(&mut self, task: &str, program: &str, args: &str) -> &mut Self {
        let mut x = HashMap::new();
        x.insert(program.to_string(), args.to_string());
        self.tasks.insert(task.to_string(), x);
        self
    }
    pub fn run(&self) -> Result<(), Error> {
        for (name, p) in self.tasks.iter() {
            for (program, args) in p {
                if std::process::Command::new(program)
                    .args(args.split_whitespace())
                    .current_dir(".")
                    .spawn()
                    .expect("")
                    .wait()
                    .expect("")
                    .success()
                    .eq(&false)
                {
                    println!(">> step {name} failed");
                    return Err(Error::other("test failed"));
                } else {
                    println!(">> step {name} passed");
                    continue;
                }
            }
        }
        Ok(())
    }
}
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
    timestamp: u64,
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
        .subcommand(
            Command::new("timeline")
                .about("show event timeline (commits)")
                .arg(
                    Arg::new("utc")
                        .long("utc")
                        .help("Display timestamps in UTC instead of local time")
                        .required(false)
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("iso")
                        .long("iso")
                        .help("Display timestamps in ISO 8601 format with timezone offset")
                        .required(false)
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(Command::new("view").about("show the latest commit"))
        // Ajout de la commande diff
        .subcommand(Command::new("diff").about("show changes since the last seal"))
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

pub fn cargo_project_hook() -> Result<(), Error> {
    println!("cargo project detected");
    PreCommit::new()
        .add_task("fmt", "cargo", "fmt --check")
        .add_task("test", "cargo", "test --no-fail-fast")
        .add_task("lint", "cargo", "clippy -- -D clippy::all")
        .run()
}

pub fn npm_project_hook() -> Result<(), Error> {
    println!("npm project detected");

    // Detect package manager
    let (pm_prog, run_args_for): (&str, fn(&str) -> String) =
        if Path::new("pnpm-lock.yaml").exists() {
            ("pnpm", |script: &str| format!("run -s {script}"))
        } else if Path::new("yarn.lock").exists() {
            // yarn v1: `yarn <script>`
            ("yarn", |script: &str| script.to_string())
        } else {
            // default to npm
            ("npm", |script: &str| format!("run -s {script}"))
        };

    // Parse package.json to discover available scripts
    let mut available: HashMap<String, String> = HashMap::new();

    if let Some(serde_json::Value::Object(obj)) = read_to_string("package.json")
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("scripts").cloned())
    {
        for (k, v) in obj {
            if let Some(cmd) = v.as_str() {
                available.insert(k, cmd.to_string());
            }
        }
    }

    // Build pre-commit pipeline from discovered scripts
    let mut pc = PreCommit::new();

    // format/fmt
    if available.contains_key("format") {
        pc.add_task("format", pm_prog, &run_args_for("format"));
    } else if available.contains_key("fmt") {
        pc.add_task("fmt", pm_prog, &run_args_for("fmt"));
    }

    // lint
    if available.contains_key("lint") {
        pc.add_task("lint", pm_prog, &run_args_for("lint"));
    }

    // test (present by défaut chez npm, mais on vérifie quand même)
    if available.contains_key("test") || Path::new("package.json").exists() {
        pc.add_task("test", pm_prog, &run_args_for("test"));
    }

    // If nothing was detected, at least try tests to gate commits
    if pc.tasks.is_empty() {
        pc.add_task("test", pm_prog, &run_args_for("test"));
    }

    pc.run()
}
fn hooks() -> Result<(), Error> {
    if Path::new("Cargo.toml").exists() {
        cargo_project_hook()
    } else if Path::new("package.json").exists() {
        npm_project_hook()
    } else {
        Ok(())
    }
}
fn main() -> ExitCode {
    let args = apps();
    let author = var(AK_USERNAME).expect("get username failed");
    let author_email = var(AK_EMAIL).expect("get username email failed");

    match args.subcommand() {
        Some(("init", _)) => {
            // Layout:
            // - .eikyu/
            //   - cubes/<YYYY-MM>/<author>.cube
            //   - branches/ (reserved)
            //   - tree/<author>
            create_dir_all("./.eikyu").expect("create dir failed");
            create_dir_all(format!(".eikyu{MAIN_SEPARATOR_STR}cubes"))
                .expect("create cubes dir failed");
            create_dir_all(format!(".eikyu{MAIN_SEPARATOR_STR}branches"))
                .expect("create branches dir failed");
            let tree_path = format!(".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{author}");
            create_dir_all(&tree_path).expect("create tree dir failed");

            // Ensure the current cube file exists
            let cube = cube_path_for(&author);
            let _ = Writer::create(&cube).expect("failed to initialize cube");
            println!("Initialized repository. Cube: {cube}");
            println!("Reference tree: {tree_path}");
            ExitCode::SUCCESS
        }

        Some(("inscribe", sub)) => {
            assert!(hooks().is_ok(), ">> !! source code refused !!");
            let target = sub
                .get_one::<String>("path")
                .map(String::as_str)
                .unwrap_or(".");
            let cube = cube_path_for(&author);
            let mut w = Writer::create(&cube).expect("open cube failed");
            w.store_directory(target).expect("store directory failed");
            println!("Inscribed: {target}");
            ExitCode::SUCCESS
        }

        Some(("seal", sub)) => {
            assert!(hooks().is_ok(), "source code refused");
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

            let commit_message = COMMIT_TEMPLATE
                .replace("%type%", &ty)
                .replace("%summary%", &summary)
                .replace("%body%", &body)
                .replace("%author%", &author)
                .replace("%author_email%", &author_email);

            let cube = cube_path_for(&author);
            let parent = last_commit_id(&cube).expect("read last commit failed");

            let placeholder_off = save_string_in_cube(&cube, "commit:pending", &commit_message)
                .expect("failed to reserve commit id");
            let pending_event =
                Writer::read_one_at(&cube, placeholder_off).expect("failed to read back pending");
            let assigned_id = pending_event.id;

            let record = CommitRecord {
                id: assigned_id,
                parent,
                ty: &ty,
                summary: &summary,
                body: &body,
                author: &author,
                author_email: &author_email,
                timestamp: u64::try_from(pending_event.timestamp / 1_000_000).unwrap_or(0),
            };
            let json = serde_json::to_string_pretty(&record).expect("serialize commit failed");

            save_string_in_cube(&cube, "commit", &json).expect("failed to save commit record");

            // --- MISE À JOUR DE L'ARBRE APRÈS LE COMMIT ---
            match tree::update_tree(&author) {
                Ok(_) => println!("Reference tree updated successfully."),
                Err(e) => eprintln!("Error updating reference tree: {}", e),
            }
            // --- FIN DE LA MISE À JOUR ---

            println!(
                "Sealed: {} {} (id={} parent={})",
                ty,
                summary,
                assigned_id,
                parent
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            ExitCode::SUCCESS
        }

        Some(("timeline", sub)) => {
            let cube = cube_path_for(&author);
            let show_utc = sub.get_flag("utc");
            let show_iso = sub.get_flag("iso");
            let commits = read_commits_from_cube(&cube).expect("read commits failed");
            if commits.is_empty() {
                println!("No commits.");
                return ExitCode::SUCCESS;
            }
            for ev in commits {
                match serde_json::from_str::<serde_json::Value>(&ev.noumenon) {
                    Ok(v) => {
                        let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(ev.id);
                        let ts_raw_u128: Option<u128> = match v.get("timestamp") {
                            Some(serde_json::Value::Number(n)) => n.as_u64().map(|u| u as u128),
                            Some(serde_json::Value::String(s)) => s.parse::<u128>().ok(),
                            _ => None,
                        };

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

                        let when = if let Some(ts_raw) = ts_raw_u128 {
                            let mut ts_ms_i128: i128 = ts_raw as i128;
                            if ts_ms_i128 > 1_000_000_000_000_000_i128 {
                                ts_ms_i128 /= 1_000_000;
                            }
                            if let Ok(ts_ms) = i64::try_from(ts_ms_i128) {
                                if let Some(naive) = DateTime::from_timestamp_millis(ts_ms) {
                                    if show_utc {
                                        if show_iso {
                                            naive.to_rfc3339()
                                        } else {
                                            naive.format("%Y-%m-%d %H:%M:%S UTC").to_string()
                                        }
                                    } else {
                                        let local = naive.with_timezone(&chrono::Local);
                                        if show_iso {
                                            local.to_rfc3339()
                                        } else {
                                            local.format("%Y-%m-%d %H:%M:%S").to_string()
                                        }
                                    }
                                } else {
                                    "-".to_string()
                                }
                            } else {
                                "-".to_string()
                            }
                        } else {
                            "-".to_string()
                        };

                        println!("#{id} [{ty}] {summary} @ {when}");
                    }
                    Err(_) => {
                        println!("#{} [commit] <unparsed>", ev.id);
                        return ExitCode::FAILURE;
                    }
                }
            }
            ExitCode::SUCCESS
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
                        ExitCode::SUCCESS
                    }
                    Err(_) => {
                        println!("#{} [commit]", ev.id);
                        ExitCode::FAILURE
                    }
                }
            } else {
                println!("No commits.");
                ExitCode::SUCCESS
            }
        }

        // Ajout du handler pour la commande diff
        Some(("diff", _)) => diff::diff(),

        _ => {
            println!("unknown command");
            ExitCode::FAILURE
        }
    }
}
