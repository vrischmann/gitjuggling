use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Command};

extern crate term;
extern crate clap;

use clap::{Arg, App};

fn find_git_dirs(current_depth: i32, max_depth: i32, path: PathBuf) -> io::Result<Vec<PathBuf>> {
    if current_depth >= max_depth {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();

    let dirs = path.read_dir()?;
    for v in dirs {
        let entry = v?;

        let ft = entry.file_type()?;
        let file_path = entry.path();

        if ft.is_dir() {
            if entry.path().ends_with(".git") {
                result.push(file_path);
                continue;
            }

            let mut sub_result = find_git_dirs(current_depth + 1, max_depth, file_path)?;
            result.append(&mut sub_result);
        }
    }

    return Ok(result);
}

fn get_repositories(depth: i32) -> io::Result<Vec<PathBuf>> {
    let cwd = Path::new(".").to_owned();
    return find_git_dirs(0, depth, cwd);
}

#[derive(Debug)]
struct GitResult {
    exit_code: ExitStatus,
    stderr: Vec<u8>,
    stdout: Vec<u8>,
}

fn run_git_command(repo: &PathBuf, args: &Vec<&str>) -> io::Result<GitResult> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;

    return Ok(GitResult {
        exit_code: output.status,
        stderr: output.stderr,
        stdout: output.stdout,
    });
}

enum Error {
    Io(io::Error),
    Term(term::Error),
    Parse(std::num::ParseIntError),
    NoArguments,
    CommandFailed,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::Io(ref err) => write!(f, "i/o: {}", err),
            Error::Term(ref err) => write!(f, "term: {}", err),
            Error::Parse(ref err) => write!(f, "{}", err),
            Error::NoArguments => write!(f, "no arguments passed"),
            Error::CommandFailed => write!(f, "command failed"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(err: std::num::ParseIntError) -> Error {
        Error::Parse(err)
    }
}

impl From<term::Error> for Error {
    fn from(err: term::Error) -> Error {
        Error::Term(err)
    }
}

fn run_app(matches: clap::ArgMatches) -> Result<(), Error> {
    let depth: i32 = matches.value_of("depth").unwrap_or("9999").parse()?;

    let args = match matches.values_of("args") {
        Some(args) => args.collect(),
        None => Vec::new(),
    };

    if args.is_empty() {
        return Err(Error::NoArguments);
    }


    let repos = get_repositories(depth)?;
    let mut t = term::stdout().unwrap();

    for repo in repos {
        let parent = repo.parent().unwrap().to_path_buf();
        let result = run_git_command(&parent, &args)?;

        let path = parent
            .canonicalize()?
            .as_path()
            .to_string_lossy()
            .into_owned();

        t.fg(term::color::BRIGHT_YELLOW)?;
        println!("{}", path);
        t.reset()?;

        io::stdout().write(&result.stdout)?;
        io::stderr().write(&result.stderr)?;

        if !result.exit_code.success() {
            return Err(Error::CommandFailed);
        }
    }

    return Ok(());
}

fn main() {
    let matches = App::new("gitjuggling")
        .version("1.0")
        .about("Runs a git command on all sub repositories under $PWD")
        .arg(
            Arg::with_name("depth")
                .short("d")
                .long("depth")
                .value_name("N")
                .help("Only go up to a depth of N")
                .takes_value(true),
        )
        .arg(Arg::with_name("args").multiple(true))
        .get_matches();

    std::process::exit(match run_app(matches) {
        Ok(_) => 0,
        Err(err) => {
            writeln!(io::stderr(), "error: {}", err).unwrap();
            1
        }
    });
}
