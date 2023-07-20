use libc::getenv;
use nix::{
    errno::Errno,
    fcntl::{open, OFlag},
    sys::{
        stat::Mode,
        wait::{waitpid, WaitStatus},
    },
    unistd::{
        chdir, close, dup2, execve, fork, getpid, isatty, pipe, setpgid, tcsetpgrp, ForkResult, Pid,
    },
};
use signal_hook::consts;

use std::{
    collections::HashMap,
    convert::Infallible,
    ffi::{CStr, CString},
    fmt::{Display, Formatter},
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
        parser::{ExecuteMode, OpType, ParseResult, Parser},
        token::Token,
        Command,
    },
    errors::ShellError,
    frontend::{write_error_to_shell, write_to_stderr, write_to_stdout, Prompt},
};

const BUILTIN_COMMANDS: [&str; 4] = ["cd", "exec", "jobs", "fg"];

#[derive(Clone, Debug)]
pub enum ProcessStatus {
    Running,
    // Suspended
    // Done,
}

impl Display for ProcessStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessStatus::Running => write!(f, "running"),
            // ProcessStatus::Suspended => write!(f, "suspended"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Process {
    pid: Pid,
    cmd: String,
    status: ProcessStatus,
}

impl Process {
    fn new(pid: Pid, cmd: String, status: ProcessStatus) -> Self {
        Process { pid, cmd, status }
    }
}

impl Display for Process {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} {}", self.pid, self.status, self.cmd)
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    processes: Vec<Process>,
    pgrp: Pid,
}

impl Job {
    fn new() -> Self {
        Self {
            processes: Vec::new(),
            pgrp: Pid::from_raw(0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    pub execution_successful: bool,
    pub env_paths: Vec<String>,
    execution_mode: ExecutionMode,
    pub is_interactive: bool,
    // Operations to be done on different `fd`s
    fds_ops: HashMap<i32, FdOperation>,
    job: Job,
    tty_fd: i32,
}

#[derive(Copy, Clone, Debug)]
enum FdOperation {
    Set { to: i32 },
    Close,
}

#[derive(Copy, Clone, Debug)]
enum ExecutionMode {
    Normal,
    Subshell,
    Pipeline,
    Redirect,
    Background,
}

impl Engine {
    pub fn new(is_interactive: bool) -> anyhow::Result<Self> {
        let tty_fd = get_tty_fd()?;

        Ok(Self {
            execution_successful: true,
            env_paths: parse_paths(),
            execution_mode: ExecutionMode::Normal,
            fds_ops: HashMap::new(),
            job: Job::new(),
            tty_fd,
            is_interactive,
        })
    }

    pub fn fire_on(&mut self) -> anyhow::Result<()> {
        write_to_stdout("Welcome to Dead Simple Shell!\n")?;

        let term = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(consts::SIGINT, Arc::clone(&term))?;

        let mut prompt = Prompt::new();
        // For handling SIGINT
        while !term.load(Ordering::Relaxed) {
            let mut lexer = Lexer::new();
            while !lexer.complete_processing() {
                // If we have more than 1 tokens
                // at this stage, we would have parsed
                // some last cycle, which means we are
                // in multiline mode
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

        while let Some(parse_result) = parser.get_command()? {
            if parse_result.exit_term {
                return Ok(true);
            }

            match parse_result.execute_mode {
                ExecuteMode::Normal => {
                    self.execution_mode = ExecutionMode::Normal;

                    // Currently trying to follow a philosophy of only executing
                    // one command at a time for separators and other normal stuff
                    //
                    // while 2 commands for redirect opertaor, second command contains
                    // file path, so it is one command in true sense
                    assert!(parse_result.cmds.len() == 1 || parse_result.cmds.len() == 2);

                    let set_stdin_to = self.handle_operations_before_exec(&parse_result)?;

                    self.execute_command(parse_result.cmds[0].clone())?;

                    let break_loop =
                        self.handle_operations_after_exec(&parse_result, set_stdin_to)?;
                    if break_loop {
                        break;
                    }
                }
                ExecuteMode::Subshell(tokens) => {
                    self.execution_mode = ExecutionMode::Subshell;
                    self.fork_process_and_execute(false, None, ExecuteMode::Subshell(tokens))?;
                }
            }
        }

        Ok(false)
    }

    fn handle_operations_before_exec(
        &mut self,
        parse_result: &ParseResult,
    ) -> anyhow::Result<Option<i32>> {
        let mut set_stdin_to: Option<i32> = None;
        let last_cmd = parse_result
            .cmds
            .last()
            .expect("expected file path to be present");
        let file_path = &last_cmd.path;

        // Operators which needs addressing before execution starts
        match parse_result.associated_operator {
            Some(OpType::RedirectAppendOutput(fd_opt)) | Some(OpType::RedirectOutput(fd_opt)) => {
                // Default value: stdout
                let fd_to_be_set = fd_opt.map_or(1, |fd| fd);

                let mut flags = OFlag::O_CREAT;
                if matches!(
                    parse_result.associated_operator,
                    Some(OpType::RedirectOutput(_))
                ) {
                    flags.insert(OFlag::O_TRUNC);
                } else {
                    flags.insert(OFlag::O_APPEND);
                }
                flags.insert(OFlag::O_WRONLY);

                let mut mode = Mode::S_IRUSR;
                mode.insert(Mode::S_IWUSR);

                let file_fd = open(file_path, flags, mode)?;
                self.fds_ops
                    .insert(fd_to_be_set, FdOperation::Set { to: file_fd });

                self.execution_mode = ExecutionMode::Redirect;
            }
            Some(OpType::RedirectInput(fd_opt)) | Some(OpType::RedirectReadWrite(fd_opt)) => {
                // Default value: stdin
                let fd_to_be_set = fd_opt.map_or(0, |fd| fd);

                let (flags, mode) = if matches!(
                    parse_result.associated_operator,
                    Some(OpType::RedirectReadWrite(_))
                ) {
                    let mut flags = OFlag::O_CREAT;
                    flags.insert(OFlag::O_RDWR);

                    (flags, Mode::S_IRWXU)
                } else {
                    (OFlag::O_RDONLY, Mode::S_IRUSR)
                };

                let file_fd = open(file_path, flags, mode)?;
                self.fds_ops
                    .insert(fd_to_be_set, FdOperation::Set { to: file_fd });

                self.execution_mode = ExecutionMode::Redirect;
            }
            Some(OpType::RedirectSquirrelOutput { source, target }) => {
                // Default value: stdout
                let target_fd = target.map_or(1, |fd| fd);

                // None means "-", so we need to close
                // the fd, thats all
                if let Some(source_fd) = source {
                    self.fds_ops
                        .insert(source_fd, FdOperation::Set { to: target_fd });
                } else {
                    self.fds_ops.insert(target_fd, FdOperation::Close);
                }

                self.execution_mode = ExecutionMode::Redirect;
            }
            Some(OpType::RedirectSquirrelInput { source, target }) => {
                // Default value: stdout
                let target_fd = target.map_or(0, |fd| fd);

                // None means "-", so we need to close
                // the fd, thats all
                if let Some(source_fd) = source {
                    self.fds_ops
                        .insert(source_fd, FdOperation::Set { to: target_fd });
                } else {
                    self.fds_ops.insert(target_fd, FdOperation::Close);
                }

                self.execution_mode = ExecutionMode::Redirect;
            }
            Some(OpType::Pipe) => {
                let (fd0, fd1) = pipe()?;
                set_stdin_to = Some(fd0);
                self.fds_ops.insert(1, FdOperation::Set { to: fd1 });
                self.execution_mode = ExecutionMode::Pipeline;
            }
            Some(OpType::Background) => {
                self.execution_mode = ExecutionMode::Background;
            }
            _ => {}
        }

        Ok(set_stdin_to)
    }

    fn handle_operations_after_exec(
        &mut self,
        parse_result: &ParseResult,
        set_stdin_to: Option<i32>,
    ) -> anyhow::Result<bool> {
        let mut break_loop = false;

        // Operators which needs addressing after execution starts
        match parse_result.associated_operator {
            Some(OpType::OrIf) => {
                if self.execution_successful {
                    break_loop = true;
                    return Ok(break_loop);
                }
            }
            Some(OpType::AndIf) => {
                if !self.execution_successful {
                    break_loop = true;
                    return Ok(break_loop);
                }
            }
            Some(OpType::Semicolon) => {}
            _ => {}
        }

        self.reset_fds_ops();

        // If execution mode last cycle is pipeline
        if matches!(self.execution_mode, ExecutionMode::Pipeline) {
            // Read fd from previous pipe operation
            // to set curr stdin
            if let Some(fd) = set_stdin_to {
                self.fds_ops.insert(0, FdOperation::Set { to: fd });
            }
        }

        Ok(break_loop)
    }

    fn reset_fds_ops(&mut self) {
        self.fds_ops = HashMap::new();
    }

    fn execute_command(&mut self, command: Command) -> anyhow::Result<()> {
        if is_builtin_command(&command.tokens[0].lexeme) {
            // FIXME: Handle this error properly
            self.execution_successful = !self.handle_builtin_command(command).is_err();
        } else if matches!(self.execution_mode, ExecutionMode::Subshell) {
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
            "jobs" => {
                self.job
                    .processes
                    .iter()
                    .enumerate()
                    .for_each(|(idx, process)| {
                        // FIXME: use panic safe write_to_stdout here
                        println!("{} {}", idx, process);
                    });

                Ok(())
            }
            "fg" => {
                let pgrp = match self.job.processes.first() {
                    Some(first_process) => {
                        println!("first process pid: {}", first_process.pid);
                        first_process.pid
                    }
                    None => getpid(),
                };

                let curr_pid = getpid();
                println!(
                    "pid received from getpid for shell in foreground: {}",
                    curr_pid
                );

                // let curr_tty_pid = tcgetattr(self.tty_fd)?;
                // curr_tty_pid.

                println!("TTY FD: {}", self.tty_fd);
                tcsetpgrp(self.tty_fd, pgrp)?;
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
                if self.is_interactive {
                    if self.job.processes.len() == 0 {
                        self.job.pgrp = child_pid;
                    }

                    let command = command.expect(
                        "invalid state: should have had command in background process execution flow",
                    );
                    self.job.processes.push(Process::new(
                        child_pid,
                        command.as_string(),
                        ProcessStatus::Running,
                    ));

                    setpgid(child_pid, self.job.pgrp)?;
                }
                // if matches!(self.execution_mode, ExecutionMode::Background) {
                // }

                for (fd, value) in &self.fds_ops {
                    match value {
                        FdOperation::Set { to } => {
                            // We do not to close stdins cause they
                            // need to go to next iteration
                            if *fd == 0 {
                                continue;
                            }
                            close(*to)?;
                        }
                        FdOperation::Close => {
                            close(*fd)?;
                        }
                    }
                }

                // We do not wait for forked children if the command is
                // running in pipeline mode
                //
                // Note: last command in the pipeline is the only one
                // we wait for ( that gets handled cause we only set
                // pipe execution mode when we receive a pipe operator )
                //
                // TIP: While debugging piping related issues, comment this if
                // condition and let it wait on each command execution
                if !matches!(self.execution_mode, ExecutionMode::Pipeline)
                    && !matches!(self.execution_mode, ExecutionMode::Background)
                    && !self.is_interactive
                {
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
                        _ => write_to_stderr(&format!("Did not get exited: {:?}", wait_status))?,
                    }
                }
            }
            Ok(ForkResult::Child) => {
                self.tty_fd = get_tty_fd()?;
                match execute_mode {
                    ExecuteMode::Normal => {
                        if self.is_interactive {
                            let curr_pid = getpid();

                            if self.job.processes.len() == 0 {
                                // first process
                                self.job.pgrp = curr_pid;
                            }

                            println!("pgrp: {}", self.job.pgrp);
                            // passing frist as 0, makes it take it's own
                            // pid by default
                            setpgid(curr_pid, self.job.pgrp)?;
                        }
                        // tcsetpgrp(self.tty_fd, pgrp)?;

                        // if matches!(self.execution_mode, ExecutionMode::Background) {}

                        let command =
                            command.expect("internal error: should have contained valid command");

                        for (fd, op) in &self.fds_ops {
                            match op {
                                FdOperation::Set { to } => {
                                    dup2(*to, *fd)?;
                                    close(*to)?;
                                }
                                FdOperation::Close => {
                                    close(*fd)?;
                                }
                            }
                        }

                        execute_external_cmd(command.clone(), self.env_paths.clone())?;
                    }
                    ExecuteMode::Subshell(tokens) => {
                        self.parse_and_execute(&tokens)?;
                    }
                }
            }
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

fn get_tty_fd() -> anyhow::Result<i32> {
    let tty_fd;

    match open("/dev/tty", OFlag::O_RDWR, Mode::S_IRWXU) {
        Ok(fd) => {
            tty_fd = fd;
        }
        Err(_) => {
            // Comment from busybox:
            /* BTW, bash will try to open(ttyname(0)) if open("/dev/tty") fails.
             * That sometimes helps to acquire controlling tty.
             * Obviously, a workaround for bugs when someone
             * failed to provide a controlling tty to bash! :) */

            let mut fd = 2;

            loop {
                if isatty(fd)? {
                    tty_fd = fd;
                    break;
                }

                fd -= 1;
                if fd < 0 {
                    return Err(ShellError::EngineError(
                        "can't access tty; job control turned off".into(),
                    )
                    .into());
                }
            }
        }
    }

    Ok(tty_fd)
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
        let is_interactive = true;
        let mut engine = Engine::new(is_interactive)
            .expect("engine should have been able to be created successfully");

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

    // FIXME:
    // #[test]
    // fn test_cmd_execution_of_subshell_cmds() {
    //     let engine = check("(true)");
    //     assert!(engine.execution_successful);

    //     let engine = check("(false)");
    //     assert!(!engine.execution_successful);

    //     // MANUAL: check if pwds get printed correctly
    //     let engine = check("(mkdir testdir && cd testdir && pwd) && pwd");
    //     assert!(engine.execution_successful);

    //     // cleanup
    //     let engine = check("rm -r testdir");
    //     assert!(engine.execution_successful);

    //     // MANUAL: check if this exit does not exit the main shell
    //     let engine = check("(mkdir testdir && cd testdir && exit) && pwd");
    //     assert!(engine.execution_successful);

    //     // cleanup
    //     let engine = check("rm -r testdir");
    //     assert!(engine.execution_successful);
    // }

    #[test]
    fn test_cmd_execution_of_piped_cmds() {
        let engine = check(" ls -la | grep c | sort | uniq");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_redirect_output_ops() {
        let engine = check("ls > files2");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_redirect_input_ops() {
        let engine = check("ls > files2");
        assert!(engine.execution_successful);

        let engine = check("rm files2");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_redirect_append_ops() {
        let engine = check("echo foo >> files2");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_redirect_read_write_ops() {
        let engine = check("echo foo <> files2");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_redirect_squirrel_output() {
        let engine = check("ls /tmp/ doesnotexist 2&>1");
        assert!(!engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_redirect_squirrel_input() {
        let engine = check("echo foo <&2");
        assert!(engine.execution_successful);
    }

    #[test]
    fn test_cmd_execution_of_bg_processes() {
        let engine = check("ping google.com &");
        assert!(engine.execution_successful);
    }
}
