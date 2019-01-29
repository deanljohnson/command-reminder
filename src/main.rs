extern crate clap;
use clap::{App, Arg};

extern crate dirs;
extern crate nix;

use std::collections::HashSet;
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::iter::FromIterator;

fn main() -> std::io::Result<()> {
    let matches = App::new("Command Reminder")
        .version("1.0")
        .author("Dean Johnson <dean@deanljohnson.com>")
        .about("Stores commands behind keywords and allows you to search for them later.")
        .arg(
            Arg::with_name("add")
                .short("a")
                .long("add")
                .takes_value(true)
                .value_names(&["command", "keywords"])
                .help("Adds a command to your reminders with the given keywords."),
        )
        .arg(
            Arg::with_name("remove")
                .short("r")
                .long("remove")
                .takes_value(true)
                .value_name("keywords")
                .help("Removes a command matching any of the given keywords."),
        )
        .arg(Arg::with_name("search").multiple(true))
        .get_matches();

    // Handle add command
    if let Some(values) = matches.values_of("add") {
        let mut values = values;
        return do_add(values.next().unwrap(), values.next().unwrap());
    }
    // Handle remove command
    if let Some(values) = matches.value_of("remove") {
        return do_remove(values);
    }
    // Handle searching for keywords
    if let Some(values) = matches.values_of("search") {
        return do_search(values.collect::<Vec<&str>>());
    }

    Ok(())
}

/// Handles the command "--add [command] [keywords]".
/// Will either add the command to the reminders file
/// or ask the user if they want to merge these keywords
/// with any other keywords already existing for the command.
fn do_add(command: &str, keywords: &str) -> std::result::Result<(), std::io::Error> {
    let data = read_reminders_file()?;

    // TODO: verify non-empty command

    let mut line_index: usize = 0;
    for line in data.lines() {
        if line == command {
            return add_to_preexisting_command(&data, command, keywords, line_index);
        }
        line_index = line_index + 1;
    }

    return add_new_command(&data, command, keywords);
}

/// Handles removing commands for a given keyword.
/// Will ask the user before removing each command.
fn do_remove(keywords: &str) -> std::result::Result<(), std::io::Error> {
    // TODO: what happens if keywords has a "#"?
    let data = read_reminders_file()?;
    let data_lines = data.lines().collect();
    let keywords_vec = keywords.split(" ").collect();

    let matching_indices = find_matching_commands(&data_lines, &keywords_vec);
    let removed_indices = {
        let remove_command_filter =
            |l: &&usize| match ask_yes_no(&format!("Remove \"{}\"? (y/n) ", data_lines[**l])) {
                Err(_) => true,
                Ok(v) => v,
            };

        let mut cmd_vec = matching_indices
            .iter()
            .filter(remove_command_filter)
            .collect::<Vec<&usize>>();
        cmd_vec.reverse();
        cmd_vec
    };

    let mut data_lines = data_lines;
    for cmd_line in removed_indices {
        data_lines.remove(*cmd_line);
        data_lines.remove(*cmd_line - 1);
    }

    return write_reminders_file(&data_lines.join("\n"));
}

/// Handles the command "[keywords]".
/// Will search for commands with any of the given keywords.
fn do_search(keywords: Vec<&str>) -> std::result::Result<(), std::io::Error> {
    let data = read_reminders_file()?;
    let data_lines = data.lines().collect();
    let cmd_vec = find_matching_commands(&data_lines, &keywords);

    match cmd_vec.len() {
        0 => println!("No commands found with any of the given keywords"),
        1 => {
            if ask_yes_no(&format!("Run '{}'? (y/n) ", data_lines[0]))? {
                run_command(data_lines[0])?;
            }
            return Ok(());
        }
        _ => {
            let options = cmd_vec
                .iter()
                .map(|l| data_lines[*l])
                .collect::<Vec<&str>>();
            let cmd_number = ask_multiple(&options)?;
            return run_command(options[cmd_number]);
        }
    }

    return Ok(());
}

/// Handles adding keywords to an already existing command reminder.
fn add_to_preexisting_command(
    data: &str,
    command: &str,
    keywords: &str,
    command_line: usize,
) -> std::result::Result<(), std::io::Error> {
    if ask_yes_no("A reminder already exists for the given command. Merge keywords? (y/n) ")? {
        merge_keywords(data.as_ref(), command, keywords, command_line)?;
    }
    return Ok(());
}

/// Handles adding a new command reminder
fn add_new_command(
    data: &str,
    command: &str,
    keywords: &str,
) -> std::result::Result<(), std::io::Error> {
    let new_data = format!("{}# {}\n{}", data, keywords, command);
    return write_reminders_file(&new_data);
}

/// Reads the reminders file into a string.
fn read_reminders_file() -> std::result::Result<String, std::io::Error> {
    // Setup path to file
    let mut path = dirs::config_dir().unwrap();
    path.push("command-reminder");
    path.push("reminders");

    // Open the file
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)?;

    // Read file into string and return
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    return Ok(data);
}

/// Overwrites the reminders file with the given string.
fn write_reminders_file(data: &str) -> std::result::Result<(), std::io::Error> {
    // Setup path to file
    let mut path = dirs::config_dir().unwrap();
    path.push("command-reminder");
    path.push("reminders");

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    return writeln!(file, "{}", data);
}

/// Merges the given keywords with any existing keywords for the given command.
fn merge_keywords(
    data: &str,
    _command: &str,
    keywords: &str,
    command_line: usize,
) -> std::result::Result<(), std::io::Error> {
    let mut data_lines = data.lines().collect::<Vec<&str>>();
    let keywords_str = data_lines[command_line - 1];

    // TODO: verify syntax of keywords string
    let new_keywords = keywords.split(' ');
    let existing_keywords = keywords_str.split(' ');

    // Collect unique keywords
    let mut keywords_set = HashSet::<&str>::from_iter(new_keywords);
    for keyword in existing_keywords {
        keywords_set.insert(keyword);
    }

    // Remove leading # from set - need to guarantee it is first and cant rely on set iterator ordering
    keywords_set.remove("#");

    // Create new keyword string
    let mut merged_keywords = keywords_set.into_iter().collect::<Vec<&str>>().join(" ");
    merged_keywords.insert_str(0, "# ");
    data_lines[command_line - 1] = &merged_keywords;

    return write_reminders_file(&data_lines.join("\n"));
}

/// Runs the given command via "exec", thereby replacing this processes image.
fn run_command(cmd: &str) -> std::result::Result<(), std::io::Error> {
    println!("running {}", cmd);
    let cmd_parts = cmd.splitn(2, " ").collect::<Vec<&str>>();
    let cmd_args = cmd_parts[1]
        .split(" ")
        .map(|s| CString::new(s).unwrap())
        .collect::<Vec<CString>>();

    println!("{:?}", cmd_parts);
    println!("{:?}", cmd_args);

    return match nix::unistd::execvp(&CString::new(cmd_parts[0])?, &cmd_args) {
        Ok(_) => Ok(()),
        Err(error) => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            error.to_string(),
        )),
    };
}

fn find_matching_commands(data_lines: &Vec<&str>, keywords: &Vec<&str>) -> Vec<usize> {
    let mut cmd_vec: Vec<usize> = Vec::new();

    // Collect commands that have matching keywords
    for idx in 0..data_lines.len() {
        if data_lines[idx].starts_with("#") && keywords.iter().any(|k| data_lines[idx].contains(k))
        {
            cmd_vec.push(idx + 1);
        }
    }

    return cmd_vec;
}

/// Asks the user to select from one of the given options and returns
/// the zero based index of the selected option.
fn ask_multiple(options: &Vec<&str>) -> std::result::Result<usize, std::io::Error> {
    loop {
        // Print available command options
        for idx in 0..options.len() {
            println!("{}: {}", idx + 1, options[idx]);
        }
        print!("Select available command: ");
        std::io::stdout().flush()?;

        let mut response = String::new();
        std::io::stdin().read_line(&mut response)?;
        let response = response.trim().parse::<usize>();

        match response {
            Ok(cmd_number) => {
                if cmd_number == 0 || cmd_number > options.len() {
                    println!("Unrecognized response");
                    continue;
                }

                return Ok(cmd_number - 1);
            }
            Err(_) => {
                println!("Unrecognized response");
                continue;
            }
        }
    }
}

/// Asks a yes/no question of the user. Returns true for yes and false for no.
/// If the user gives an unexpected answer, the question is asked again.
fn ask_yes_no(question: &str) -> std::result::Result<bool, std::io::Error> {
    loop {
        print!("{}", question);
        std::io::stdout().flush()?;

        let mut response = String::new();
        std::io::stdin().read_line(&mut response)?;
        let response = response.trim();

        match response {
            "y" | "Y" => {
                return Ok(true);
            }
            "n" | "N" => {
                return Ok(false);
            }
            _ => println!("Unexpected response {}", response),
        };
    }
}
