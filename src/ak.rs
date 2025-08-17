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

/// Template for interactive commit messages.
///
/// Placeholders:
/// - %type%
/// - %summary%
/// - %body%
/// - %author%
/// - %author_email%
///
/// The rendered message is stored as an intermediate "commit:pending" event to
/// reserve and discover the final monotonically-increasing commit id.
const COMMIT_TEMPLATE: &str = r#"%type% %summary%

%body%

%author% <%author_email%>

"#;

/// Simple pre-commit pipeline orchestrator.
///
/// - Each task is a tuple (program, args) grouped under a logical name.
/// - Tasks are executed in insertion order in the current working directory.
/// - If any task fails (non-zero exit code), execution stops and an error is returned.
/// - This is intentionally minimal and local-only.
#[derive(Default)]
pub struct PreCommit {
    pub tasks: HashMap<String, HashMap<String, String>>,
}

impl PreCommit {
    /// Construct an empty pipeline.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Add a task to the pipeline.
    ///
    /// - task: human-readable step name (e.g., "fmt", "test")
    /// - program: executable to run (e.g., "cargo", "npm")
    /// - args: argument string split by whitespace (e.g., "fmt --check")
    pub fn add_task(&mut self, task: &str, program: &str, args: &str) -> &mut Self {
        let mut x = HashMap::new();
        x.insert(program.to_string(), args.to_string());
        self.tasks.insert(task.to_string(), x);
        self
    }

    /// Execute all tasks in sequence.
    ///
    /// Returns:
    /// - Ok(()) if all tasks succeed
    /// - Err(Error) on the first failure
    pub fn run(&self) -> Result<(), Error> {
        for (name, programs) in self.tasks.iter() {
            for (program, args) in programs {
                // Spawn the process and wait synchronously for completion.
                let status = std::process::Command::new(program)
                    .args(args.split_whitespace())
                    .current_dir(".")
                    .status()
                    .map_err(|e| Error::other(format!("failed to spawn '{program}': {e}")))?;

                if !status.success() {
                    println!(">> step {name} failed (status: {status})");
                    return Err(Error::other(format!(
                        "pre-commit step '{name}' failed with status {status}"
                    )));
                }

                println!(">> step {name} passed");
            }
        }
        Ok(())
    }
}

/// Wire-format of a commit event stored as phenomenon "commit" with a JSON noumenon.
///
/// This is the durable record extracted from the intermediate "commit:pending" reservation.
#[derive(Serialize)]
struct CommitRecord<'a> {
    id: u64,
    parent: Option<u64>,
    ty: &'a str,
    summary: &'a str,
    body: &'a str,
    author: &'a str,
    author_email: &'a str,
    /// Milliseconds since Unix epoch (UTC).
    timestamp: u64,
}

/// Define the CLI for the local VCS and parse arguments.
///
/// This is side-effect free and only sets up subcommands and flags.
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
        .subcommand(Command::new("diff").about("show changes since the last seal"))
        .get_matches()
}

/// Compute the current author's cube file path with year-month bucketing.
///
/// Layout:
/// - .eikyu/cubes/YYYY-MM/<author>.cube
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

/// Append a phenomenon/noumenon string pair into the target cube.
///
/// Returns the byte offset of the appended record (useful for random access).
fn save_string_in_cube(cube_path: &str, phenomenon: &str, content: &str) -> std::io::Result<u64> {
    let mut w = Writer::create(cube_path)?;
    let off = w.append(phenomenon, content)?;
    Ok(off)
}

/// Read all events from a cube, filter to commits, and return them ordered by id.
///
/// The index is rebuilt from the log and used to fetch each event.
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

/// Return the last commit id present in the cube, if any.
fn last_commit_id(cube_path: &str) -> std::io::Result<Option<u64>> {
    let commits = read_commits_from_cube(cube_path)?;
    Ok(commits.last().map(|e| e.id))
}

/// Pre-commit checks for Rust/Cargo projects.
/// - fmt --check
/// - test --no-fail-fast
/// - clippy with warnings as errors
pub fn cargo_project_hook() -> Result<(), Error> {
    println!("cargo project detected");
    PreCommit::new()
        .add_task("fmt", "cargo", "fmt --check")
        .add_task("test", "cargo", "test --no-fail-fast")
        .add_task("lint", "cargo", "clippy -- -D clippy::all")
        .run()
}

/// Pre-commit checks for Node.js projects detected via package manager files.
///
/// Discovers scripts in package.json and attempts to run a reasonable subset
/// (format/fmt, lint, test). Defaults to running `test` when nothing is found.
pub fn npm_project_hook() -> Result<(), Error> {
    println!("npm project detected");

    // Detect package manager and normalize "run" invocation.
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

/// Auto-detect project type and run the appropriate pre-commit hook.
///
/// No-op for unrecognized projects.
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

    // Resolve author identity from the environment. These are required for commit metadata.
    let author = match var(AK_USERNAME) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Missing {AK_USERNAME}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let author_email = match var(AK_EMAIL) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Missing {AK_EMAIL}: {e}");
            return ExitCode::FAILURE;
        }
    };

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

            // Ensure the current cube file exists for this author/month.
            let cube = cube_path_for(&author);
            let _ = Writer::create(&cube).expect("failed to initialize cube");
            println!("Initialized repository. Cube: {cube}");
            println!("Reference tree: {tree_path}");
            ExitCode::SUCCESS
        }

        Some(("inscribe", sub)) => {
            // Gate the operation through pre-commit hooks. If hooks fail, abort inscription.
            if let Err(e) = hooks() {
                eprintln!("Pre-commit hooks failed: {e}");
                return ExitCode::FAILURE;
            }

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
            // Gate the operation through pre-commit hooks. If hooks fail, abort the commit.
            if let Err(e) = hooks() {
                eprintln!("Pre-commit hooks failed: {e}");
                return ExitCode::FAILURE;
            }

            // Resolve editor for interactive body capture.
            let editor = match var(EDITOR) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Missing {EDITOR}: {e}");
                    return ExitCode::FAILURE;
                }
            };

            // Commit type (interactive fallback).
            let ty = if let Some(t) = sub.get_one::<String>("type") {
                t.to_owned()
            } else {
                let types = ["feat", "fix", "refactor", "docs", "test", "chore"];
                Select::new("type:", types.to_vec())
                    .prompt()
                    .expect("type prompt failed")
                    .to_string()
            };

            // Commit summary (interactive fallback).
            let summary = if let Some(s) = sub.get_one::<String>("summary") {
                s.to_owned()
            } else {
                Text::new("summary:")
                    .prompt()
                    .expect("summary prompt failed")
            };

            // Commit body (interactive editor fallback).
            let body = if let Some(b) = sub.get_one::<String>("body") {
                b.to_owned()
            } else {
                Editor::new("body:")
                    .with_editor_command(editor.as_ref())
                    .prompt()
                    .expect("body prompt failed")
            };

            // Render the user-facing commit message and reserve an id via a "commit:pending" event.
            let commit_message = COMMIT_TEMPLATE
                .replace("%type%", &ty)
                .replace("%summary%", &summary)
                .replace("%body%", &body)
                .replace("%author%", &author)
                .replace("%author_email%", &author_email);

            let cube = cube_path_for(&author);
            let parent = last_commit_id(&cube).expect("read last commit failed");

            // Reserve an id by appending a pending record, then read it back to obtain the assigned id.
            let placeholder_off = save_string_in_cube(&cube, "commit:pending", &commit_message)
                .expect("failed to reserve commit id");
            let pending_event =
                Writer::read_one_at(&cube, placeholder_off).expect("failed to read back pending");
            let assigned_id = pending_event.id;

            // Durable commit record (wire format).
            let record = CommitRecord {
                id: assigned_id,
                parent,
                ty: &ty,
                summary: &summary,
                body: &body,
                author: &author,
                author_email: &author_email,
                // Convert internal nanoseconds to milliseconds (bounded).
                timestamp: u64::try_from(pending_event.timestamp / 1_000_000).unwrap_or(0),
            };
            let json = serde_json::to_string_pretty(&record).expect("serialize commit failed");

            save_string_in_cube(&cube, "commit", &json).expect("failed to save commit record");

            // Refresh the on-disk reference tree to match the sealed state.
            match tree::update_tree(&author) {
                Ok(_) => println!("Reference tree updated successfully."),
                Err(e) => eprintln!("Error updating reference tree: {}", e),
            }

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
                // Parse the commit JSON payload; tolerate errors by skipping malformed entries.
                match serde_json::from_str::<serde_json::Value>(&ev.noumenon) {
                    Ok(v) => {
                        let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(ev.id);

                        // Robust timestamp handling: accept number or string; allow ns or ms.
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

                        // Normalize to milliseconds and format according to flags.
                        let when = if let Some(ts_raw) = ts_raw_u128 {
                            let mut ts_ms_i128: i128 = ts_raw as i128;
                            // Heuristic: treat very large values as nanoseconds and convert to ms.
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
                    Err(e) => {
                        eprintln!("warning: failed to parse commit #{}, reason: {e}", ev.id);
                        // Skip malformed entries but keep showing the rest of the timeline.
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

        // Show changes between working directory and the last sealed reference tree.
        Some(("diff", _)) => diff::diff(),

        _ => {
            println!("unknown command");
            ExitCode::FAILURE
        }
    }
}