use anyhow::anyhow;
use colored::Colorize;
use rayon::prelude::*;
use std::fmt::Write as FmtWrite;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

struct GitOutput {
    output: std::process::Output,
}

fn do_git_command(path: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
    match process::Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
    {
        Ok(output) => Ok(GitOutput { output }),
        Err(err) => Err(anyhow!(err)),
    }
}

fn get_repositories_paths() -> anyhow::Result<Vec<PathBuf>> {
    let mut repositories_paths = Vec::<PathBuf>::new();

    for entry in WalkDir::new(".") {
        let entry = entry?;
        let entry_path = entry.into_path();

        let mut path = match entry_path.canonicalize() {
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => continue,
                _ => return Err(anyhow!(err)),
            },
            Ok(v) => v,
        };
        let path_string = path.to_string_lossy();

        if !path_string.ends_with(".git")
            || path_string.contains("third_party")
            || path_string.contains(".zigmod")
        {
            continue;
        }

        path.pop();

        repositories_paths.push(path);
    }

    Ok(repositories_paths)
}

fn main() {
    let matches = clap::Command::new("gitjuggling")
        .author("Vincent Rischmann <vincent@rischmann.fr>")
        .version("1.0")
        .about("Git juggler")
        .trailing_var_arg(true)
        .arg(clap::Arg::new("git_args").multiple_values(true))
        .get_matches();

    let git_args: Vec<&str> = matches
        .get_many::<String>("git_args")
        .unwrap()
        .map(|v| v.as_str())
        .collect();

    // Collect all local git repositories

    let repositories_paths = match get_repositories_paths() {
        Err(err) => panic!("unable to get repositories paths: {}", err),
        Ok(v) => v,
    };

    //

    let items_failed = Arc::<AtomicUsize>::new(AtomicUsize::new(0));
    let items_succeeded = Arc::<AtomicUsize>::new(AtomicUsize::new(0));

    repositories_paths.into_par_iter().for_each(|path| {
        let mut stdout = String::new();
        let mut stderr = String::new();

        writeln!(
            &mut stdout,
            "{} executing {}",
            &path.to_string_lossy().to_string().green(),
            &git_args.join(" ").yellow()
        )
        .unwrap();

        match do_git_command(&path, &git_args) {
            Err(err) => {
                items_failed.fetch_add(1, Ordering::SeqCst);
                write!(&mut stderr, "unable to do git command, err: {}", err).unwrap();
            }
            Ok(go) => {
                let go_stdout = String::from_utf8_lossy(&go.output.stdout);
                let go_stderr = String::from_utf8_lossy(&go.output.stderr);

                if go.output.status.success() {
                    items_succeeded.fetch_add(1, Ordering::SeqCst);
                    write!(&mut stdout, "{}", go_stdout).unwrap();
                } else {
                    items_failed.fetch_add(1, Ordering::SeqCst);
                    write!(&mut stdout, "{}", go_stdout,).unwrap();
                    write!(&mut stderr, "{}", go_stderr,).unwrap();
                }
            }
        }

        io::stdout().write_all(stdout.as_bytes()).unwrap();
        io::stderr().write_all(stderr.as_bytes()).unwrap();
    });

    //

    let succeeded = items_succeeded.load(Ordering::SeqCst);
    let failed = items_failed.load(Ordering::SeqCst);

    println!(
        "{} {} {} {}",
        format!("{}", succeeded).magenta(),
        "items succeeded,".blue(),
        format!("{}", failed).magenta(),
        "items failed".blue(),
    );

    if failed > 0 {
        process::exit(1);
    }
}
