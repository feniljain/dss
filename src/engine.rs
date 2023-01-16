use std::{ffi::{CString, CStr}, path::{PathBuf, Path}, convert::Infallible, str::FromStr, os::unix::prelude::OsStrExt};
use nix::{
    errno::Errno,
    sys::wait::{waitpid, WaitStatus},
    unistd::{chdir, execve, fork, ForkResult},
};
use libc::getenv;

use crate::{command::Command, writer::{write_to_shell, write_error_to_shell}};
use crate::errors::ShellError;

const BUILTIN_COMMANDS: [&str; 2] = ["cd", "exec"];

#[derive(Default, Clone, Debug)]
pub struct Engine {
    pub execution_successful: bool,
    pub env_paths: Vec<String>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            execution_successful: true,
            env_paths: parse_paths(),
        }
    }

    pub fn execute(&mut self, command: Command) -> anyhow::Result<()> {
        // if{

        // }
        if is_builtin_command(&command.args_with_cmd[0]) {
            let result = self.handle_builtin_command(command);
            if result.is_err() {
                self.execution_successful = false;
            } else {
                self.execution_successful = true;
            }
        } else {
            self.execute_command_by_forking(command)?;
        }

        Ok(())
    }

    pub fn execute_command_by_forking(&mut self, command: Command) -> anyhow::Result<()> {
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
                // FIXME: Handle error here
                self.execute_command_without_forking(command)?;
            }
            Err(err) => panic!("Fork failed: {err:?}"),
        }
        Ok(())
    }

    // GOTCHA: This currently executes the command and stops the complete program
    // due to libc::exit at the end
    fn execute_command_without_forking(&mut self, command: Command) -> anyhow::Result<()> {
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

    fn handle_builtin_command(&mut self, mut command: Command) -> anyhow::Result<()> {
        let cmd_str = command.args_with_cmd[0].as_str();

        match cmd_str {
            "cd" => {
                let mut path_to_go_str = "/";
                if command.args.len() > 1 {
                    // If we receive `~` after cd, we want to go to
                    // absolute root, which is what "/" denotes already
                    if command.args_with_cmd[1] != "~" {
                        path_to_go_str = &command.args_with_cmd[1];
                    }
                }

                let cmd_path = Path::new(path_to_go_str);
                chdir(cmd_path)?;
                Ok(())
            }
            "exec" => {
                // Remove `exec` keyword and then pass the remaining command
                command.args_with_cmd.remove(0);
                let command = Command::parse_cmd_str_vec(command.args_with_cmd)?;
                self.execute_command_without_forking(command)?;
                Ok(())
            }
            _ => Err(ShellError::CommandNotFound(cmd_str.to_string()).into()),
        }
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
