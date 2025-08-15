pub mod data;
pub mod event;

use crate::data::write::Writer;
use clap::{Arg, ArgMatches, Command};
use std::fs::File;
use std::path::Path;

fn cli() -> ArgMatches {
    Command::new("akasha")
        .about("A CLI for the Akasha Living Wisdom System")
        .version("0.1.0")
        .author("hackiado <seidogitan@example.com>")
        .subcommand(Command::new("sonar").about("Probe semantic hyperspace with a query"))
        .subcommand(Command::new("merge").about("Merge multiple cubes into one"))
        .subcommand(Command::new("connect").about("Connect two cubes to exchange wisdom"))
        .subcommand(Command::new("perspective").about("Manage autonomous AI perspectives"))
        .subcommand(
            Command::new("cube")
                .about("Manage Akasha cubes (start, stop, status, etc.)")
                .subcommand(
                    Command::new("create").about("Create a cube").arg(
                        Arg::new("name")
                            .help("Name of the cube")
                            .required(true)
                            .value_parser(clap::builder::NonEmptyStringValueParser::new()),
                    ),
                )
                .subcommand(Command::new("start").about("Start a cube instance"))
                .subcommand(Command::new("stop").about("Stop a cube instance"))
                .subcommand(Command::new("restart").about("Restart a cube instance"))
                .subcommand(Command::new("status").about("Display the status of a cube"))
                .subcommand(
                    Command::new("show")
                        .about("Show detailed information about a cube")
                        .arg(
                            Arg::new("name")
                                .help("Name of the cube")
                                .required(true)
                                .value_parser(clap::builder::NonEmptyStringValueParser::new()),
                        ),
                )
                .subcommand(
                    Command::new("read")
                        .about("Read information inside a cube")
                        .arg(
                            Arg::new("name")
                                .help("Name of the cube")
                                .required(true)
                                .value_parser(clap::builder::NonEmptyStringValueParser::new()),
                        ),
                )
                .subcommand(Command::new("ping").about("Check if a cube is responsive"))
                .subcommand(Command::new("validate").about("Validate the integrity of a cube"))
                .subcommand(Command::new("clone").about("Clone a cube"))
                .subcommand(Command::new("bubble").about("Create a ephemeral clone of a cube"))
                .subcommand(Command::new("export").about("Export a cube to a file"))
                .subcommand(Command::new("import").about("Import a cube from a file"))
                .subcommand(
                    Command::new("mode")
                        .about("Change the cube's cognitive mode (e.g., analytical, creative)"),
                ),
        )
        .get_matches()
}

fn main() {
    let app = cli();

    // Properly handle nested subcommands
    if let Some(("cube", cube_matches)) = app.subcommand() {
        match cube_matches.subcommand() {
            Some(("create", create_matches)) => {
                let name: &String = create_matches
                    .get_one::<String>("name")
                    .expect("name is required");
                println!("Creating cube: {name}");
                Writer::create(name.as_str()).expect("failed to create cube");
                println!("Cube created successfully.");
            }
            Some(("read", create_matches)) => {
                if Path::new(create_matches.get_one::<String>("name").unwrap()).exists() {
                    let name: &String = create_matches
                        .get_one::<String>("name")
                        .expect("name is required");
                    println!("Reading cube: {name}");
                    Writer::new(File::open(Path::new(name)).expect("failed to open cube file"))
                        .read_all()
                        .expect("failed to read cube file");
                    println!("Cube reading successfully.");
                }else{
                    println!("Cube not exists.");
                }
            }
            Some((cmd, _)) => {
                println!("cube subcommand: {cmd}");
            }
            None => {
                println!("Use a cube subcommand (e.g., create, start, stop, ...)");
            }
        }
    } else {
        println!("{app:?}");
    }
}
