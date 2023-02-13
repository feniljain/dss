use libc::getenv;
use nix::{
    errno::Errno,
    sys::wait::{waitpid, WaitStatus},
    unistd::{chdir, close, dup2, execve, fork, pipe, ForkResult},
};
use signal_hook::consts;

use std::{
    convert::Infallible,
    ffi::{CStr, CString},
    io,
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    command::{
        lexer::Lexer,
        parser::{ExecuteMode, OpType, Parser},
        token::Token,
        Command,
    },
    errors::ShellError,
    frontend::{write_error_to_shell, write_to_shell, Prompt},
};

const BUILTIN_COMMANDS: [&str; 2] = ["cd", "exec"];

#[derive(Clone, Debug)]
pub struct Engine {
    pub execution_successful: bool,
    pub env_paths: Vec<String>,
    pub in_subshell: bool,
    pub set_stdin_to: i32,
    pub set_stdout_to: i32,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            execution_successful: true,
            env_paths: parse_paths(),
            in_subshell: false,
            set_stdin_to: 0,
            set_stdout_to: 1,
        }
    }

    pub fn fire_on(&mut self) -> anyhow::Result<()> {
        write_to_shell("Welcome to Dead Simple Shell!\n")?;

        let term = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(consts::SIGINT, Arc::clone(&term))?;

        let mut prompt = Prompt::new();
        while !term.load(Ordering::Relaxed) {
            let mut lexer = Lexer::new();
            while !lexer.complete_processing() {
                if lexer.tokens.len() > 0 {
                    prompt.activate_multiline_prompt();
                }

                prompt.render(self.execution_successful)?;

                let mut input_str = String::new();

                io::stdin().read_line(&mut input_str)?;

                if input_str.trim() == "" {
                    continue;
                }

                lexer.scan(&input_str)?;

                prompt.deactivate_multiline_prompt();
            }

            let break_term_loop = self.parse_and_execute(&lexer.tokens)?;
            if break_term_loop {
                break;
            }
        }

        Ok(())
    }

    pub fn parse_and_execute(&mut self, tokens: &Vec<Token>) -> anyhow::Result<bool> {
        let mut parser = Parser::new(tokens);
        let mut last_operator = None;
        let mut set_stdin_to = None;

        while let Some(parse_result) = parser.get_command()? {
            if parse_result.exit_term {
                return Ok(true);
            }

            match parse_result.execute_mode {
                ExecuteMode::Normal => {
                    // Currently trying to follow a philosophy of only executing
                    // one command at a time
                    assert_eq!(parse_result.cmds.len(), 1);

                    // Read fd from previous pipe operation
                    // to set curr stdin
                    if let Some(fd) = set_stdin_to {
                        self.set_stdin_to = fd;
                    }
                    set_stdin_to = None;

                    // Operators which needs addressing current execution cycle
                    // ( that's why we operate on currernt operator here )
                    match parse_result.associated_operator {
                        Some(OpType::RedirectOutput(_fd)) => {}
                        Some(OpType::RedirectInput(_fd)) => {}
                        Some(OpType::Or) => {
                            let (fd0, fd1) = pipe()?;
                            set_stdin_to = Some(fd0);
                            self.set_stdout_to = fd1;
                        }
                        _ => {}
                    }

                    // Operators which needs addressing next execution cycle
                    // ( that's why we operate on last operator here )
                    match last_operator {
                        Some(OpType::OrIf) => {
                            if self.execution_successful {
                                break;
                            }
                        }
                        Some(OpType::AndIf) => {
                            if !self.execution_successful {
                                break;
                            }
                        }
                        Some(OpType::Semicolon) => {}
                        _ => {}
                    }

                    self.execute_command(parse_result.cmds[0].clone())?;
                }
                ExecuteMode::Subshell(tokens) => {
                    self.in_subshell = true;
                    self.fork_process_and_execute(false, None, ExecuteMode::Subshell(tokens))?;
                    self.in_subshell = false;
                }
            }

            last_operator = parse_result.associated_operator;
            self.reset_stdin_out_fds();
        }

        Ok(false)
    }

    fn reset_stdin_out_fds(&mut self) {
        self.set_stdin_to = 0;
        self.set_stdout_to = 1;
    }

    fn execute_command(&mut self, command: Command) -> anyhow::Result<()> {
        if is_builtin_command(&command.tokens[0].lexeme) {
            // FIXME: Handle this error properly
            self.execution_successful = !self.handle_builtin_command(command).is_err();
        } else if self.in_subshell {
            execute_external_cmd(command, self.env_paths.clone())?;
        } else {
            self.fork_process_and_execute(
                command.negate_exit_status,
                Some(command),
                ExecuteMode::Normal,
            )?;
        }

        Ok(())
    }

    fn handle_builtin_command(&mut self, mut command: Command) -> anyhow::Result<()> {
        let cmd_str = command.tokens[0].lexeme.as_str();

        match cmd_str {
            "cd" => {
                let mut path_to_go_str = "/";
                if command.tokens.len() > 1 {
                    // If we receive `~` after cd, we want to go to
                    // absolute root, which is what "/" denotes already
                    if command.tokens[1].lexeme != "~" {
                        path_to_go_str = &command.tokens[1].lexeme;
                    }
                }

                let cmd_path = Path::new(path_to_go_str);
                chdir(cmd_path)?;
                Ok(())
            }
            "exec" => {
                // Remove `exec` keyword and then pass the remaining command
                command.tokens.remove(0);
                self.parse_and_execute(&command.tokens)?;
                Ok(())
            }
            _ => Err(ShellError::CommandNotFound(cmd_str.to_string()).into()),
        }
    }

    fn fork_process_and_execute(
        &mut self,
        negate_exit_status: bool,
        command: Option<Command>,
        execute_mode: ExecuteMode,
    ) -> anyhow::Result<bool> {
        match unsafe { fork() } {
            Ok(ForkResult::Parent {
                child: child_pid, ..
            }) => {
                // This fd is not needed in parent anymore,
                // child will write to it
                if self.set_stdout_to != 1 {
                    close(self.set_stdout_to)?;
                }
                let wait_status = waitpid(child_pid, None).expect(&format!(
                    "Expected to wait for child with pid: {:?}",
                    child_pid
                ));
                match wait_status {
                    WaitStatus::Exited(_pid, mut exit_code) => {
                        // FIXME: Ugly if/else, replace
                        // with binary operations
                        if negate_exit_status {
                            if exit_code == 0 {
                                exit_code = 1;
                            } else {
                                exit_code = 0;
                            }
                        }
                        self.execution_successful = exit_code == 0;
                        return Ok(exit_code == 0);
                    }
                    _ => write_to_shell(&format!("Did not get exited: {:?}", wait_status))?,
                }
            }
            Ok(ForkResult::Child) => match execute_mode {
                ExecuteMode::Normal => {
                    // Only change file descriptors of stdin
                    // and stdout if they are different from
                    // standard ones
                    if self.set_stdin_to != 0 {
                        dup2(self.set_stdin_to, 0)?;
                        close(self.set_stdin_to)?;
                    }
                    if self.set_stdout_to != 1 {
                        dup2(self.set_stdout_to, 1)?;
                        close(self.set_stdout_to)?;
                    }

                    let command =
                        command.expect("internal error: should have contained valid command");
                    execute_external_cmd(command.clone(), self.env_paths.clone())?;
                }
                ExecuteMode::Subshell(tokens) => {
                    self.parse_and_execute(&tokens)?;
                }
            },
            Err(err) => panic!("Fork failed: {err:?}"),
        }

        Ok(false)
    }
}

// GOTCHA: This currently executes the command and stops the complete program
// due to libc::exit at the end
fn execute_external_cmd(command: Command, env_paths: Vec<String>) -> anyhow::Result<()> {
    let cmd_args = command.get_args();
    let args: &[CString] = if cmd_args.len() < 1 { &[] } else { &cmd_args };

    let mut exit_status = 0;
    let mut errno_opt: Option<Errno> = None;
    // If command starts with "/" or "./" or "../", do not do PATH appending
    if command.is_unqualified_path {
        'env_paths: for env_path_str in &env_paths {
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
            &command.tokens[0].lexeme,
            command.is_unqualified_path,
        )?;
        // FIXME: Pass proper errno here
        exit_status = 1;
    }

    unsafe { libc::_exit(exit_status) };
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

#[cfg(test)]
mod tests {
    use crate::command::lexer::Lexer;

    use super::Engine;

    // Trying to use `true` and `false` in tests here
    // cause they are readily available on UNIX systems
    // or are easy to replicate behaviour of too

    fn get_tokens(input_str: &str) -> anyhow::Result<Lexer> {
        let mut lexer = Lexer::new();
        lexer.scan(input_str)?;
        Ok(lexer)
    }

    fn check(input_str: &str) -> Engine {
        let mut engine = Engine::new();

        let ip_str = input_str.to_string() + "\n";
        let lexer = get_tokens(&ip_str).expect("lexer failed, check lexer tests");

        engine
            .parse_and_execute(&lexer.tokens)
            .expect("expected successful execution");

        engine
    }

    #[test]
    fn test_simple_cmd_execution() {
        let engine = check("ls");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_simple_cmd_with_args_execution() {
        let engine = check("ls -la");
        assert!(engine.execution_successful);

        let engine = check("ls -la src/");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_with_semicolon_separator() {
        let engine = check("ls -la ; true");
        assert!(engine.execution_successful);

        let engine = check("false ; true");
        assert!(engine.execution_successful);

        let engine = check("true ; false");
        assert!(!engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_with_logical_or_separator() {
        let engine = check("true || true");
        assert!(engine.execution_successful);

        let engine = check("false || false");
        assert!(!engine.execution_successful);

        let engine = check("true || false");
        assert!(engine.execution_successful);

        let engine = check("false || true");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_with_logical_and_separator() {
        let engine = check("true && true");
        assert!(engine.execution_successful);

        let engine = check("true && true");
        assert!(engine.execution_successful);

        let engine = check("true && false");
        assert!(!engine.execution_successful);

        let engine = check("false && true");
        assert!(!engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_with_negate_exit_status() {
        let engine = check("true && ! false");
        assert!(engine.execution_successful);

        let engine = check("! false || ! true");
        assert!(engine.execution_successful);

        let engine = check("! true");
        assert!(!engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_subshell_cmds() {
        let engine = check("(true)");
        assert!(engine.execution_successful);

        let engine = check("(false)");
        assert!(!engine.execution_successful);

        // MANUAL: check if pwds get printed correctly
        let engine = check("(mkdir testdir && cd testdir && pwd) && pwd");
        assert!(engine.execution_successful);

        // cleanup
        let engine = check("rm -r testdir");
        assert!(engine.execution_successful);

        // MANUAL: check if this exit does not exit the main shell
        let engine = check("(mkdir testdir && cd testdir && exit) && pwd");
        assert!(engine.execution_successful);

        // cleanup
        let engine = check("rm -r testdir");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_piped_cmds() {
        let engine = check("ls | grep c");
        assert!(engine.execution_successful);
    }
}
