use nix::{
    sys::wait::waitpid,
    unistd::{execve, fork, ForkResult},
};
use signal_hook::consts;
use std::{
    ffi::CString,
    io::{self, Write},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

// const BUILTIN_COMMANDS: [&str; 1] = ["cd"];

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
// [] correct path parsing and argument parsing according to the `man execve`
// [] use current directory path in env paths
// [] add Ctrl-C + Ctrl-D handling
// [] pass stage 1 tests
// [] parsing all paths
// [] trying all paths robustly
// [] use exit status of wait: The Unix convention is that a zero exit status represents success, and any non-zero exit status represents failure.
// [] implement your own `cd` in C
// [] add support for multiline commands
//
// Bonus
// [] add red color to output if last command exited with a non-zero status
fn main() -> io::Result<()> {
    io::stdout().write_all(b"Welcome to Dead Simple Shell!\n")?;

    let env_paths = vec!["/bin/", "/usr/bin/"];

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(consts::SIGINT, Arc::clone(&term))?;

    while !term.load(Ordering::Relaxed) {
        io::stdout().write_all(b"$ ")?;
        // Flushing is important because:
        // https://stackoverflow.com/questions/34993744/why-does-this-read-input-before-printing
        io::stdout().flush().expect("flush failed!");

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

        let mut evaluate_with_path_env = false;

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
                            Some(
                                CString::new(args_with_cmd[0])
                                    .expect("Could not construct CString path"),
                            )
                        } else {
                            evaluate_with_path_env = true;
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

        match unsafe { fork() } {
            Ok(ForkResult::Parent { child, .. }) => {
                waitpid(child, None)
                    .expect(&format!("Expected to wait for child with pid: {:?}", child));
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
                if evaluate_with_path_env {
                    for env_path_str in env_paths {
                        let mut path = PathBuf::from_str(env_path_str)
                            .expect("Could not construct path buf from env_path");

                        path.push(command.path.clone());

                        execve_(&path, args)
                    }
                } else {
                    execve_(&command.path, args);
                }

                unsafe { libc::_exit(0) };
            }
            Err(err) => panic!("Fork failed: {err:?}"),
        }
    }

    Ok(())
}

fn execve_(path: &PathBuf, args: &[CString]) {
    let path = CString::new(path.as_os_str().as_bytes()).expect("Could not construct CString path");

    // println!("path: {:?}", path);
    // println!("args: {:?}", args);

    // match execve::<CString, CString>(&path, args, &[]) {
    //     Ok(_) => {}
    //     Err(_err) => println!("{:?}", _err),
    // }

    let _ = execve::<CString, CString>(&path, args, &[]);
}
