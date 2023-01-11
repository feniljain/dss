use colored::Colorize;
use libc::getenv;
use nix::{
    errno::Errno,
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
struct Command<'a> {
    args_with_cmd: Vec<&'a str>,
    path: PathBuf,
    args: Vec<CString>,
    negate_exit_status: bool,
    // Unqualified path = A path not starting with "/" or "../" or "./"
    is_unqualified_path: bool,
}

#[derive(Default, Clone, Debug)]
struct Engine {
    execution_successful: bool,
    env_paths:  Vec<String>,
}

#[derive(Debug)]
enum Color {
    Green,
    Red,
}

// FIXME: Handle error properly everywhere

// Tasks
// [X] correct command split by space
// [X] handle empty commands
// [X] add / ./ handling
// [X] correct path parsing and argument parsing according to the `man execve`
// [] add Ctrl-C + Ctrl-D handling
// [X] pass stage 1 tests
// [X] parsing all paths
// [X] trying all paths robustly
// [X] proper handling for command not found
// [X] include handling of `!` while parsing and also while checking exit status
// [X] use exit status of wait: The Unix convention is that a zero exit status represents success, and any non-zero exit status represents failure.
// [X] implement your own `cd` in C
// [X] implement `cd` builtin in your own shell
// [X] print error messages according to errno
//      [X] for invalid path command ( e.g. ./a.sh ) give no such file or directory error
// [] add support for `;`, `||` and `&&` in commands
// [] after stage 1 refactor code to have a separate engine and cmd parsing module
// [] add support for multiline commands
// [] after stage 1 refactor code to have a separate engine and cmd parsing module
//
// Bonus
// [X] add color depending on exit status
// [] add last segment of current folder like my own zsh with some color

// Bugs
// [X] builtin command execution successful handling
// [] builtin command execution error case handling

fn main() -> anyhow::Result<()> {
    write_to_shell("Welcome to Dead Simple Shell!\n")?;

    // FIXME: Move all variables here to engine/command struct

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(consts::SIGINT, Arc::clone(&term))?;

    let mut engine = Engine::new();

    while !term.load(Ordering::Relaxed) {
        if !engine.execution_successful {
            write_to_shell_colored("$ ", Color::Red)?;
        } else {
            write_to_shell_colored("$ ", Color::Green)?;
        }

        let mut input_str = String::new();

        io::stdin().read_line(&mut input_str)?;

        input_str = input_str.trim().to_string();

        if input_str == "" {
            continue;
        }

        if input_str == "exit" {
            break;
        }

        // FIXME: This is hacky, implement proper tokenizer
        // if input_str.contains(";") {
        //     let commands = input_str.split(";").for_each(|| {
        //     });
        // }
        let command = Command::parse_command(&input_str, false);

        engine.execute_command(command)?;
    }

    Ok(())
}

impl<'a> Command<'a> {
    fn parse_command(cmd_str: &str, mut negate_exit_status: bool) -> Command {
        let mut args_with_cmd: Vec<&str> = cmd_str.split_ascii_whitespace().collect();

        if args_with_cmd[0] == "!" {
            args_with_cmd.remove(0);

            if args_with_cmd.len() == 0 {
                // true will resolve to /usr/bin/true
                args_with_cmd.push("true")
            }

            negate_exit_status = true;
        }

        let cmd_path =
        PathBuf::from_str(&args_with_cmd[0]).expect("Could not construct path buf from command");

        let mut is_unqualified_path = false;

        let mut command = Command {
            path: cmd_path.clone(),
            args: args_with_cmd
                .iter()
                .enumerate()
                .filter_map(|(idx, x)| {
                    if idx == 0 {
                        if !(cmd_path.starts_with("/")
                        || cmd_path.starts_with("./")
                        || cmd_path.starts_with("../")
                        || cmd_path.components().count() > 1)
                        {
                            // cmd_path.components().last().and_then(|last_component| {
                            //     Some(
                            //         CString::new(last_component.as_os_str().as_bytes())
                            //             .expect("Could not construct CString path"),
                            //     )
                            // })
                            //
                            is_unqualified_path = true;
                        }
                        Some(CString::new(args_with_cmd[0]).expect("Could not construct CString path"))
                    } else {
                        Some(
                            CString::new(x.bytes().collect::<Vec<u8>>())
                                .expect(&format!("Could not construct CString arg: {:?}", x)),
                        )
                    }
                })
                .collect(),
            negate_exit_status,
            is_unqualified_path: false,
            args_with_cmd,
        };

        command.is_unqualified_path = is_unqualified_path;

        return command;
    }
}

impl Engine {
    fn new() -> Self {
        Self {
            execution_successful: true,
            env_paths: parse_paths(),
        }
    }

    fn execute_command(&mut self, command: Command) -> anyhow::Result<()> {
        if is_builtin_command(command.args_with_cmd[0]) {
            let mut path_to_go_str = "/";
            if command.args.len() > 1 {
                // If we receive `~` after cd, we want to go to
                // absolute root, which is what "/" denotes already
                if command.args_with_cmd[1] != "~" {
                    path_to_go_str = command.args_with_cmd[1];
                }
            }

            let result = handle_builtin_command(&command.args_with_cmd[0], path_to_go_str);
            if result.is_err() {
                self.execution_successful = false;
            } else {
                self.execution_successful = true;
            }
        } else {
            match unsafe { fork() } {
                Ok(ForkResult::Parent {
                    child: child_pid, ..
                }) => {
                        let wait_status = waitpid(child_pid, None).expect(&format!(
                            "Expected to wait for child with pid: {:?}",
                            child_pid
                        ));
                        match wait_status {
                            WaitStatus::Exited(_pid, mut exit_code) => {
                                // FIXME: Ugly if/else, replace
                                // with binary operations
                                if command.negate_exit_status {
                                    if exit_code == 0 {
                                        exit_code = 1;
                                    } else {
                                        exit_code = 0;
                                    }
                                }
                                self.execution_successful = exit_code == 0;
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

                    let mut exit_status = 0;
                    let mut errno_opt: Option<Errno> = None;
                    // If command starts with "/" or "./" or "../", do not do PATH appending
                    if command.is_unqualified_path {
                        'env_paths: for env_path_str in &self.env_paths {
                            let mut path = PathBuf::from_str(&env_path_str)
                                .expect("Could not construct path buf from env_path");

                            path.push(command.path.clone());

                            match execve_(&path, args) {
                                // This Ok() break is actually useless
                                // cause execve() only returns if there's
                                // an error, otherwise it just stops the
                                // child and returns the control to
                                // parent. For more understanding
                                // read: RETURN VALUES section
                                // of execve man page
                                Ok(_) => break 'env_paths,
                                Err(errno_) => {
                                    errno_opt = Some(errno_);
                                }
                            }
                        }
                    } else {
                        let result = execve_(&command.path, args);
                        if let Err(errno) = result {
                            errno_opt = Some(errno);
                        }
                    }

                    if let Some(errno) = errno_opt {
                        write_error_to_shell(errno, &command.args_with_cmd[0], command.is_unqualified_path)?;
                        // FIXME: Pass proper errno here
                        exit_status = 1;
                    }

                    unsafe { libc::_exit(exit_status) };
                }
                Err(err) => panic!("Fork failed: {err:?}"),
            }
        }

        Ok(())
    }
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

fn handle_builtin_command(cmd_str: &str, path_to_go_str: &str) -> anyhow::Result<()> {
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

// Here we try to not use println as it can
// panic, more here:
// https://github.com/BurntSushi/advent-of-code/issues/17

fn write_to_shell(output: &str) -> anyhow::Result<()> {
    io::stdout().write_all(output.as_bytes())?;

    // Flushing is important because:
    // https://stackoverflow.com/questions/34993744/why-does-this-read-input-before-printing
    io::stdout().flush().expect("flush failed!");

    Ok(())
}

fn write_to_shell_colored(output: &str, color: Color) -> anyhow::Result<()> {
    //FIXME: Figure out why colored doesn't work with write_all
    // and replace println here
    match color {
        Color::Red => print!("{}", output.red()),
        Color::Green => print!("{}", output.green()),
    }

    io::stdout().flush().expect("flush failed!");

    Ok(())
}

fn write_error_to_shell(
    errno: Errno,
    cmd_str: &str,
    is_unqualified_path: bool,
) -> anyhow::Result<()> {
    if is_unqualified_path {
        write_to_shell(&format!("dss: command not found: {}\n", cmd_str))?;
    } else {
        write_to_shell(&format!("dss: {}: {}\n", errno.desc(), cmd_str))?;
    }

    Ok(())
}
