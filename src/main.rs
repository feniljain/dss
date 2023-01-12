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
    ops::ControlFlow,
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
    args_with_cmd: Vec<String>,
    path: PathBuf,
    args: Vec<CString>,
    negate_exit_status: bool,
    // Unqualified path = A path not starting with "/" or "../" or "./"
    is_unqualified_path: bool,
}

#[derive(Clone, Debug)]
enum Separator {
    Semicolon,
    LogicalOr,
    LogicalAnd,
}

impl Separator {
    // fn as_str(&self) -> &str {
    //     match self {
    //         Separator::Semicolon => ";",
    //         Separator::LogicalOr => "||",
    //         Separator::LogicalAnd => "&&",
    //     }
    // }

    fn is_separator<T: ToString>(input: T) -> bool {
        let input_str = input.to_string();
        input_str == ";" || input_str == "||" || input_str == "&&"
    }

    fn to_separator<T: ToString>(input: T) -> Option<Separator> {
        let input_str = input.to_string();

        match input_str.as_str() {
            ";" => Some(Separator::Semicolon),
            "||" => Some(Separator::LogicalOr),
            "&&" => Some(Separator::LogicalAnd),
            _ => None,
        }
    }
}

#[derive(Default, Clone, Debug)]
struct Engine {
    execution_successful: bool,
    env_paths: Vec<String>,
}

#[derive(Debug)]
enum Color {
    Green,
    Red,
}

// FIXME: Handle error properly everywhere
// FIXME: Remove all unnecessary clones

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
// [] after stage 1 refactor code to have a separate engine and cmd parsing
//    module, as well as break the functions in it down too
//
// Bonus
// [X] add color depending on exit status
// [] add last segment of current folder like my own zsh with some color
// [] Implement readline like https://github.com/kkawakam/rustyline

// Bugs
// [X] builtin command execution successful handling
// [] builtin command execution error case handling
// [] correct signal handling by referencing https://github.com/kkawakam/rustyline

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

        input_str = input_str.to_string();

        if input_str == "" {
            continue;
        }

        if input_str == "exit" {
            break;
        }

        let (commands, separators) = Command::parse_input(input_str);
        if separators.len() == 0 {
            assert!(commands.len() == 1);
            if commands[0].args_with_cmd[0] == "exit" {
                break;
            }
            engine.execute_command(commands[0].clone())?;
        } else {
            let mut execution_result: Option<(bool, &Separator)> = None;
            let n_cmds = commands.len();
            let mut break_term_loop = false;
            commands.iter().enumerate().for_each(|(i, command)| {
                if command.args_with_cmd[0] == "exit" {
                    break_term_loop = true;
                    ControlFlow::Break::<bool>(true);
                    return;
                }

                // FIXME: Do not ignore result of execution here
                match execution_result {
                    Some((last_execution_result, separator)) => {
                        match separator {
                            Separator::Semicolon => {
                                let _ = engine.execute_command(command.clone());
                            }
                            Separator::LogicalOr => {
                                if !last_execution_result {
                                    let _ = engine.execute_command(command.clone());
                                }
                            }
                            Separator::LogicalAnd => {
                                if last_execution_result {
                                    let _ = engine.execute_command(command.clone());
                                }
                            }
                        }

                        if i < n_cmds - 1 {
                            execution_result = Some((engine.execution_successful, &separators[i]));
                        }
                    }
                    None => {
                        let _ = engine.execute_command(command.clone());
                        execution_result = Some((engine.execution_successful, &separators[i]));
                    }
                }
            });

            if break_term_loop {
                break;
            }
        }
    }

    Ok(())
}

impl Command {
    fn parse_input(input_str: String) -> (Vec<Command>, Vec<Separator>) {
        // FIXME: This is an ad-hoc implementation,
        // implement proper tokenizer acc. to spec
        let mut commands = vec![];
        let mut separators = vec![];

        let mut word = String::new();
        let mut command_strs = vec![];
        input_str.chars().for_each(|ch| {
            // FIXME: Implement multiline commands here
            // For single line commands this will be the end
            if ch == '\n' {
                command_strs.push(word.clone());
                let command = Command::parse_cmd_str_vec(command_strs.clone());
                commands.push(command);
                command_strs = vec![];
                ControlFlow::Break::<char>(ch);
            }

            // FIXME: Here we make an assumption that
            // separators will always be space paadded,
            // correct this assumption
            if ch == ' ' {
                if Separator::is_separator(&word) {
                    if let Some(separator) = Separator::to_separator(&word) {
                        separators.push(separator);
                        let command = Command::parse_cmd_str_vec(command_strs.clone());
                        commands.push(command);
                        command_strs = vec![];
                    }
                } else {
                    command_strs.push(word.clone());
                }
                word = String::new();
            } else {
                word.push(ch);
            }
        });

        (commands, separators)
    }

    // fn parse_command(cmd_str: &str) -> Command {
    //     let mut args_with_cmd: Vec<&str> = cmd_str.split_ascii_whitespace().collect();
    //     Self::parse_cmd_str_vec(args_with_cmd)
    // }

    fn parse_cmd_str_vec(mut args_with_cmd: Vec<String>) -> Command {
        let mut negate_exit_status = false;

        if args_with_cmd[0] == "!" {
            args_with_cmd.remove(0);

            if args_with_cmd.len() == 0 {
                // true will resolve to /usr/bin/true
                args_with_cmd.push("true".to_string())
            }

            negate_exit_status = true;
        }

        let cmd_path = PathBuf::from_str(&args_with_cmd[0])
            .expect("Could not construct path buf from command");

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
                        Some(
                            CString::new(args_with_cmd[0].clone())
                                .expect("Could not construct CString path"),
                        )
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
        if is_builtin_command(&command.args_with_cmd[0]) {
            let mut path_to_go_str = "/";
            if command.args.len() > 1 {
                // If we receive `~` after cd, we want to go to
                // absolute root, which is what "/" denotes already
                if command.args_with_cmd[1] != "~" {
                    path_to_go_str = &command.args_with_cmd[1];
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
                        write_error_to_shell(
                            errno,
                            &command.args_with_cmd[0],
                            command.is_unqualified_path,
                        )?;
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

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn test_command_input_str_parsing() {
        let (commands, separators) = Command::parse_input("ls\n".to_string());

        assert!(commands.len() == 1);
        assert!(separators.len() == 0);
        assert!(commands[0].args_with_cmd[0] == "ls".to_string());

        let (commands, separators) = Command::parse_input("ls -la\n".to_string());

        assert!(commands.len() == 1);
        assert!(separators.len() == 0);
        assert!(commands[0].args_with_cmd[0] == "ls".to_string());
        assert!(commands[0].args_with_cmd[1] == "-la".to_string());

        let (commands, separators) = Command::parse_input("ls -la ; echo foo\n".to_string());

        assert!(commands.len() == 2);
        assert!(separators.len() == 1);
        assert!(commands[0].args_with_cmd[0] == "ls".to_string());
        assert!(commands[0].args_with_cmd[1] == "-la".to_string());
        assert!(commands[1].args_with_cmd[0] == "echo".to_string());
        assert!(commands[1].args_with_cmd[1] == "foo".to_string());

        let (commands, separators) = Command::parse_input("ls -la || echo foo\n".to_string());

        assert!(commands.len() == 2);
        assert!(separators.len() == 1);
        assert!(commands[0].args_with_cmd[0] == "ls".to_string());
        assert!(commands[0].args_with_cmd[1] == "-la".to_string());
        assert!(commands[1].args_with_cmd[0] == "echo".to_string());
        assert!(commands[1].args_with_cmd[1] == "foo".to_string());

        let (commands, separators) = Command::parse_input("ls -la && echo foo\n".to_string());

        assert!(commands.len() == 2);
        assert!(separators.len() == 1);
        assert!(commands[0].args_with_cmd[0] == "ls".to_string());
        assert!(commands[0].args_with_cmd[1] == "-la".to_string());
        assert!(commands[1].args_with_cmd[0] == "echo".to_string());
        assert!(commands[1].args_with_cmd[1] == "foo".to_string());
    }
}
