use crate::data::write::Writer;
use clap::{ArgMatches, Command};
use glob::glob;
use inquire::{Select, Text};
use std::env::var;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::MAIN_SEPARATOR_STR;

pub mod data;
pub mod event;
const COMMIT_TEMPLATE: &str =
    "%type% %summary%\n\n\t%body%\n\nSigned-off-by: %author% <%author_email%>\n\n";
fn apps() -> ArgMatches {
    Command::new("ak")
        .about("a new vcs")
        .subcommand(Command::new("init").about("init data"))
        .subcommand(Command::new("inscribe").about("track data"))
        .subcommand(Command::new("seal").about("register changes"))
        .subcommand(Command::new("replay").about("checkout changes"))
        .subcommand(Command::new("revert").about("revert changes"))
        .subcommand(Command::new("merge").about("merge branches"))
        .subcommand(Command::new("diverge").about("manage branches"))
        .subcommand(Command::new("diff").about("show differences"))
        .subcommand(Command::new("root").about("show the first commit"))
        .subcommand(Command::new("breath"))
        .subcommand(Command::new("timeline").about("show event timeline"))
        .subcommand(Command::new("shard"))
        .subcommand(Command::new("link"))
        .subcommand(Command::new("echoes").about("tags or symbolic refs"))
        .subcommand(Command::new("view").about("show branch head, working perspective"))
        .subcommand(Command::new("interlace").about("merge history of multiple cubes"))
        .get_matches()
}
fn main() {
    let args = apps();
    let author = var("AK_USERNAME").expect("get username failed");
    let author_email = var("AK_EMAIL").expect("get username email failed");
    let now = chrono::Local::now().format("%m").to_string();
    if args.subcommand_matches("init").is_some() {
        create_dir_all("./.eikyu").expect("create dir failed");

        create_dir_all(format!(
            ".eikyu{MAIN_SEPARATOR_STR}cubes"
        ))
        .expect("create cubes dir failed");
        create_dir_all(format!(
            ".eikyu{MAIN_SEPARATOR_STR}branches"
        ))
        .expect("create branches dir failed");

        create_dir_all(format!(
            ".eikyu{MAIN_SEPARATOR_STR}cubes{MAIN_SEPARATOR_STR}{now}"
        ))
        .expect("create cubes now dir failed");

        create_dir_all(format!(
            ".eikyu{MAIN_SEPARATOR_STR}tree{MAIN_SEPARATOR_STR}{author}"
        ))
        .expect("create tree dir failed");
    } else if args.subcommand_matches("seal").is_some() {
        let types = vec!["feat", "fix", "refactor", "docs", "test", "chore"];
        let ty = Select::new("types: ", types.to_vec()).prompt().unwrap();
        let summary = Text::new("summary : ").prompt().unwrap();
        let body = Text::new("body : ").prompt().unwrap();
        let commit_template = COMMIT_TEMPLATE.replace("%type%", &types[0]);

        let commit_message = commit_template
            .replace("%type%", &ty)
            .replace("%summary%", &summary)
            .replace("%body%", &body)
            .replace("%author%", &author)
            .replace("%author_email%", &author_email);
        let mut cube = Writer::create(
            format!(
                ".eikyu{MAIN_SEPARATOR_STR}cubes{MAIN_SEPARATOR_STR}{now}{MAIN_SEPARATOR_STR}{author}.cube"
            )
            .as_str(),
        )
        .expect("failed to create cube");

        let mut commit = File::create("/tmp/commit").expect("failed to create file");
        commit
            .write_all(commit_message.as_bytes())
            .expect("failed to write file");
        commit.sync_all().expect("failed to sync file");

        cube.append_file("/tmp/commit")
            .expect("failed to save commit");
    } else {
        println!("unknow command");
    }
}
