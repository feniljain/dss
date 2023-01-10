use libc::getenv;
use nix::{
    sys::wait::{waitpid, WaitStatus},
    unistd::{chdir, execve, fork, ForkResult},
};
use signal_hook::consts;
use std::{
    convert::Infallible,
    ffi::{CStr, CString},
    io::{self, Write},
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

const BUILTIN_COMMANDS: [&str; 1] = ["cd"];

#[derive(Default, Clone, Debug)]
struct Command {
    path: PathBuf,
    args: Vec<CString>,
}

// FIXME: Handle error properly everywhere

// Tasks
// [X] correct command split by space
// [X] handle empty commands
// [X] add / ./ handling
// [X] correct path parsing and argument parsing according to the `man execve`
// [] use current directory path in env paths
// [] add Ctrl-C + Ctrl-D handling
// [X] pass stage 1 tests
// [X] parsing all paths
// [X] trying all paths robustly
// [X] proper handling for command not found
// [] include handling of `!` while parsing and also while checking exit status
// [] use exit status of wait: The Unix convention is that a zero exit status represents success, and any non-zero exit status represents failure.
// [X] implement your own `cd` in C
// [X] implement `cd` builtin in your own shell
// [] add support for multiline commands
// [] after stage 1 refactor code to have a separate engine and cmd parsing module
// [] for invalid path command ( e.g. ./a.sh ) give no such file or directory error
//
// Bonus
// [] add red color to output if last command exited with a non-zero status
fn main() -> io::Result<()> {
    write_to_shell("Welcome to Dead Simple Shell!\n")?;

    let env_paths = parse_paths();

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(consts::SIGINT, Arc::clone(&term))?;

    while !term.load(Ordering::Relaxed) {
        write_to_shell("$ ")?;

        let mut cmd_str = String::new();

        io::stdin().read_line(&mut cmd_str)?;

        cmd_str = cmd_str.trim().to_string();

        if cmd_str == "" {
            continue;
        }

        if cmd_str == "exit" {
            break;
        }

        let args_with_cmd: Vec<&str> = cmd_str.split_ascii_whitespace().collect();

        let cmd_path = PathBuf::from_str(&args_with_cmd[0])
            .expect("Could not construct path buf from command");

        // Unqualified path = A path not starting with "/" or "../" or "./"
        let mut is_unqualified_path = false;

        let command = Command {
            path: cmd_path.clone(),
            args: args_with_cmd
                .iter()
                .enumerate()
                .filter_map(|(idx, x)| {
                    if idx == 0 {
                        if cmd_path.starts_with("/")
                            || cmd_path.starts_with("./")
                            || cmd_path.starts_with("../")
                            || cmd_path.components().count() > 1
                        {
                            // cmd_path.components().last().and_then(|last_component| {
                            //     Some(
                            //         CString::new(last_component.as_os_str().as_bytes())
                            //             .expect("Could not construct CString path"),
                            //     )
                            // })
                            //
                            Some(
                                CString::new(args_with_cmd[0])
                                    .expect("Could not construct CString path"),
                            )
                        } else {
                            is_unqualified_path = true;
                            Some(
                                CString::new(args_with_cmd[0])
                                    .expect("Could not construct CString path"),
                            )
                        }
                    } else {
                        Some(
                            CString::new(x.bytes().collect::<Vec<u8>>())
                                .expect(&format!("Could not construct CString arg: {:?}", x)),
                        )
                    }
                })
                .collect(),
        };

        if is_builtin_command(args_with_cmd[0]) {
            let mut path_to_go_str = "/";
            if args_with_cmd.len() > 1 {
                // If we receive `~` after cd, we want to go to
                // absolute root, which is what "/" denotes already
                if path_to_go_str != "~" {
                    path_to_go_str = args_with_cmd[1];
                }
            }

            handle_builtin_command(&args_with_cmd[0], path_to_go_str)?;
        } else {
            match unsafe { fork() } {
                Ok(ForkResult::Parent { child, .. }) => {
                    let wait_status = waitpid(child, None)
                        .expect(&format!("Expected to wait for child with pid: {:?}", child));
                    match wait_status {
                        WaitStatus::Exited(_pid, exit_code) => {
                            if exit_code != 0 {
                                println!("Execution failed!");
                            }
                        }
                        _ => write_to_shell(&format!("Did not get exited: {:?}", wait_status))?,
                    }
                }
                Ok(ForkResult::Child) => {
                    // FIXME: Optimize this .len() out,
                    // we just wanna know if there are more
                    // than 1 elements
                    let args: &[CString] = if command.args.len() < 1 {
                        &[]
                    } else {
                        &command.args
                    };

                    // If command starts with "/" or "./" or "../", do not do PATH appending
                    if is_unqualified_path {
                        let mut err = None;
                        for env_path_str in env_paths {
                            let mut path = PathBuf::from_str(&env_path_str)
                                .expect("Could not construct path buf from env_path");

                            path.push(command.path.clone());

                            match execve_(&path, args) {
                                Ok(_) => break,
                                Err(err_) => {
                                    err = Some(err_);
                                }
                            }
                        }
                        if err.is_some() {
                            write_to_shell(&format!("dss: command not found: {}\n", cmd_str))?;
                        }
                    } else {
                        if execve_(&command.path, args).is_err() {
                            write_to_shell(&format!("dss: command not found: {}\n", cmd_str))?
                        }
                    }

                    unsafe { libc::_exit(0) };
                }
                Err(err) => panic!("Fork failed: {err:?}"),
            }
        }
    }

    Ok(())
}

fn execve_(path: &PathBuf, args: &[CString]) -> nix::Result<Infallible> {
    let path = CString::new(path.as_os_str().as_bytes()).expect("Could not construct CString path");

    // println!("path: {:?}", path);
    // println!("args: {:?}", args);

    // match execve::<CString, CString>(&path, args, &[]) {
    //     Ok(_) => {}
    //     Err(_err) => println!("{:?}", _err),
    // }

    execve::<CString, CString>(&path, args, &[])
}

fn is_builtin_command(cmd: &str) -> bool {
    BUILTIN_COMMANDS.contains(&cmd)
}

fn handle_builtin_command(cmd_str: &str, path_to_go_str: &str) -> io::Result<()> {
    if cmd_str == "cd" {
        let cmd_path = Path::new(path_to_go_str);
        chdir(cmd_path)?;
    }

    Ok(())
}

fn parse_paths() -> Vec<String> {
    let path_cstring = CString::new("PATH").expect("could not construct PATH C String");
    let envs_cstr: CString = unsafe { CStr::from_ptr(getenv(path_cstring.as_ptr())) }.into();
    return envs_cstr
        .to_str()
        .expect("could not parse concenated path str")
        .split(":")
        .map(|x| String::from(x))
        .collect();
}

fn write_to_shell(output: &str) -> io::Result<()> {
    io::stdout().write_all(output.as_bytes())?;
    // Flushing is important because:
    // https://stackoverflow.com/questions/34993744/why-does-this-read-input-before-printing
    io::stdout().flush().expect("flush failed!");

    Ok(())
}
