use crate::data::write::{self, Writer};
use clap::{Arg, ArgMatches, Command};
use std::path::Path;

// ... existing code ...
pub mod data;
pub mod event;

fn cli() -> ArgMatches {
    Command::new("akasha")
        .about("A CLI for the Akasha Living Wisdom System")
        .version("0.1.0")
        .author("hackiado <seidogitan@example.com>")
        .subcommand(
            Command::new("save")
                .about("Probe semantic hyperspace with a query")
                .subcommand(
                    Command::new("file")
                        .about("Save a file in a cube")
                        .arg(Arg::new("if").required(true))
                        .arg(Arg::new("of").required(true)),
                )
                .subcommand(
                    Command::new("directory")
                        .about("Save directory content in a cube")
                        .arg(Arg::new("path").required(true))
                        .arg(Arg::new("of").required(true)),
                )
                .subcommand(
                    Command::new("hierarchy")
                        .about("Save a tree structure in a cube")
                        .arg(Arg::new("directory").required(true)),
                ),
        )
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
                            clap::Arg::new("name")
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
                // Initialize or open cube without truncation; ensures header is present.
                Writer::create(name.as_str()).expect("failed to create cube");
                println!("Cube created successfully.");
            }
            Some(("read", create_matches)) => {
                if Path::new(create_matches.get_one::<String>("name").unwrap()).exists() {
                    let name: &String = create_matches
                        .get_one::<String>("name")
                        .expect("name is required");
                    println!("\nReading cube: {name}\n");
                    // Use helper to get a reader-capable Writer and print all records.
                    let mut reader =
                        write::read_cube(name.as_str()).expect("failed to open cube file");
                    reader.read_all().expect("failed to read cube file");
                    println!("Cube reading successfully.");
                } else {
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
    } else if let Some(("save", save_matches)) = app.subcommand() {
        match save_matches.subcommand() {
            Some(("file", file_matches)) => {
                let cube: &String = file_matches
                    .get_one::<String>("of")
                    .expect("of is required");
                let name: &String = file_matches
                    .get_one::<String>("if")
                    .expect("if is required");
                println!("Saving filename {name} to the {cube} cube");

                // Open or create cube in append-safe mode.
                let mut writer =
                    write::open_cube(cube.as_str()).expect("failed to open/create cube");
                writer
                    .store_directory(name)
                    .expect("failed to save the directory content to the cube");

                println!("File saved successfully.");
            }
            Some(("directory", file_matches)) => {
                let cube: &String = file_matches
                    .get_one::<String>("of")
                    .expect("of is required");
                let name: &String = file_matches
                    .get_one::<String>("path")
                    .expect("if is required");
                println!("Saving directory {name} content to the {cube} cube");

                // Use Writer::create to append without truncating and keep header/id state
                let mut writer = Writer::create(cube.as_str()).expect("failed to open/create cube");
                writer
                    .store_directory(Path::new(name))
                    .expect("failed to save the directory to the cube");
            }
            Some((cmd, _)) => {
                println!("save subcommand: {cmd}");
            }
            None => {
                println!("Use a save subcommand (e.g., file, directory, hierarchy)");
            }
        }
    } else {
        println!("{app:?}");
    }
}
