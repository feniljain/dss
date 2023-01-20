pub mod lexer;
pub mod token;

use std::{ffi::CString, ops::ControlFlow, path::PathBuf, str::FromStr};

use crate::errors::ShellError;

#[derive(Default, Clone, Debug)]
pub struct Command {
    pub args_with_cmd: Vec<String>,
    pub path: PathBuf,
    pub args: Vec<CString>,
    pub negate_exit_status: bool,
    // Unqualified path = A path not starting with "/" or "../" or "./"
    pub is_unqualified_path: bool,
    // is_subshell_cmd: bool,
}

impl Command {
    pub fn parse_input(input_str: String) -> anyhow::Result<(Vec<Command>, Vec<Separator>)> {
        // FIXME: This is an ad-hoc implementation,
        // implement proper tokenizer acc. to spec
        let mut commands = vec![];
        let mut separators = vec![];

        let mut err: Option<anyhow::Error> = None;

        let mut word = String::new();
        let mut command_strs = vec![];
        let mut capture_subshell_input = false;
        let mut subshell_input = String::new();
        input_str.chars().for_each(|ch| {
            if capture_subshell_input && ch != ')' {
                subshell_input.push(ch);
                word = String::new();
                ControlFlow::Continue::<()>(());
                return;
            }

            // FIXME: Implement multiline commands here
            // For single line commands this will be the end
            if ch == '\n' {
                command_strs.push(word.clone());
                let parse_result = Command::parse_cmd_str_vec(command_strs.clone());
                match parse_result {
                    Ok(command) => {
                        commands.push(command);
                        command_strs = vec![];
                    }
                    Err(err_) => {
                        err = Some(err_);
                    }
                }
                ControlFlow::Break::<char>(ch);
                return;
            }

            // FIXME: Here we make an assumption that
            // separators will always be space paadded,
            // correct this assumption
            if ch == ' ' {
                if let Some(separator) = Separator::to_separator(&word) {
                    separators.push(separator);
                    println!(
                        "word: {word:?}, command_strs: {command_strs:?}, input_str: {input_str:?}"
                    );
                    let parse_result = Command::parse_cmd_str_vec(command_strs.clone());
                    match parse_result {
                        Ok(command) => {
                            commands.push(command);
                            command_strs = vec![];
                        }
                        Err(err_) => {
                            err = Some(err_);
                            ControlFlow::Break::<char>(ch);
                        }
                    }
                } else {
                    // if word != "" {
                    println!("word on space, and not being separator: {:?}", word);
                    command_strs.push(word.clone());
                    // }
                }
                word = String::new();
            } else if ch == '(' {
                capture_subshell_input = true;
            } else if ch == ')' {
                capture_subshell_input = false;
                println!("Subshell Input: {:?}", subshell_input);
                // FIXME: Adding a new line here as our parsing code depends on it a lot,
                // possibly fix this dependency
                let result = Command::parse_input(subshell_input.clone() + "\n");
                match result {
                    Ok((mut commands_, mut separators_)) => {
                        commands.append(&mut commands_);
                        separators.append(&mut separators_);
                    }
                    Err(err_) => {
                        err = Some(err_);
                        ControlFlow::Break::<char>(ch);
                    }
                }
                subshell_input = String::new();
            } else {
                word.push(ch);
            }
        });

        // This never turned false,
        // that means we never enocuntered
        // `)`
        if capture_subshell_input {
            return Err(ShellError::ParseError("expected ) at the end".into()).into());
        }

        Ok((commands, separators))
    }

    // fn parse_command(cmd_str: &str) -> Command {
    //     let mut args_with_cmd: Vec<&str> = cmd_str.split_ascii_whitespace().collect();
    //     Self::parse_cmd_str_vec(args_with_cmd)
    // }

    pub fn parse_cmd_str_vec(mut args_with_cmd: Vec<String>) -> anyhow::Result<Command> {
        let mut negate_exit_status = false;

        println!("args to parse_cmd_str_vec: {:?}", args_with_cmd);

        if args_with_cmd[0] == "!" {
            args_with_cmd.remove(0);

            if args_with_cmd.len() == 0 {
                // true will resolve to /usr/bin/true
                args_with_cmd.push("true".to_string())
            }

            negate_exit_status = true;
        }

        // Parsing subshell command
        // let len = args_with_cmd.len();
        // let mut is_subshell_cmd = false;
        // let mut chars = args_with_cmd[0].chars();
        // let first_char = chars.next();
        // let chars = args_with_cmd[len - 1].chars();
        // let last_char = chars.last();
        // if (first_char == Some('(') && last_char != Some(')'))
        //     || (first_char != Some('(') && last_char == Some(')'))
        // {
        //     return Err(ShellError::ParseError("expected correct () pair".to_string()).into());
        // } else if first_char == Some('(') && last_char == Some(')') {
        //     is_subshell_cmd = true;
        //     args_with_cmd[0].remove(0);
        //     let str_len = args_with_cmd[len - 1].len();
        //     args_with_cmd[len - 1].remove(str_len - 1);
        // }

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
            // is_subshell_cmd,
        };

        command.is_unqualified_path = is_unqualified_path;

        return Ok(command);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Separator {
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

    // fn is_separator<T: ToString>(input: T) -> bool {
    //     let input_str = input.to_string();
    //     input_str == ";" || input_str == "||" || input_str == "&&"
    // }

    pub fn to_separator<T: ToString>(input: T) -> Option<Separator> {
        let input_str = input.to_string();

        match input_str.as_str() {
            ";" => Some(Separator::Semicolon),
            "||" => Some(Separator::LogicalOr),
            "&&" => Some(Separator::LogicalAnd),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Separator};

    // Following matklad's `check` pattern, ref:
    // https://matklad.github.io/2021/05/31/how-to-test.html
    fn check(input_str: &str) -> (Vec<Command>, Vec<Separator>) {
        return Command::parse_input(input_str.to_string() + "\n")
            .expect("parsing should have succeeded");
    }

    #[test]
    fn test_base_cmd_parsing() {
        // Testing normal command parsing
        let (commands, separators) = check("ls");

        assert_eq!(commands.len(), 1);
        assert_eq!(separators.len(), 0);
        assert_eq!(commands[0].args_with_cmd[0], "ls".to_string());
    }

    #[test]
    fn test_cmd_parsing_with_args() {
        // Testing command parsing with args
        let (commands, separators) = check("ls -la");

        assert_eq!(commands.len(), 1);
        assert_eq!(separators.len(), 0);
        assert_eq!(commands[0].args_with_cmd[0], "ls".to_string());
        assert_eq!(commands[0].args_with_cmd[1], "-la".to_string());
    }

    #[test]
    fn test_cmd_parsing_of_semicolon_separator() {
        // Testing parsing of `;`
        let (commands, separators) = check("ls -la ; echo foo");

        assert_eq!(commands.len(), 2);
        assert_eq!(separators.len(), 1);
        assert_eq!(commands[0].args_with_cmd[0], "ls".to_string());
        assert_eq!(commands[0].args_with_cmd[1], "-la".to_string());
        assert_eq!(separators[0], Separator::Semicolon);
        assert_eq!(commands[1].args_with_cmd[0], "echo".to_string());
        assert_eq!(commands[1].args_with_cmd[1], "foo".to_string());
    }

    #[test]
    fn test_cmd_parsing_of_semicolon_logical_or() {
        // Testing parsing of `||`
        let (commands, separators) = check("ls -la || echo foo");

        assert_eq!(commands.len(), 2);
        assert_eq!(separators.len(), 1);
        assert_eq!(commands[0].args_with_cmd[0], "ls".to_string());
        assert_eq!(commands[0].args_with_cmd[1], "-la".to_string());
        assert_eq!(separators[0], Separator::LogicalOr);
        assert_eq!(commands[1].args_with_cmd[0], "echo".to_string());
        assert_eq!(commands[1].args_with_cmd[1], "foo".to_string());
    }

    #[test]
    fn test_cmd_parsing_of_semicolon_logical_and() {
        // Testing parsing of `&&`
        let (commands, separators) = check("ls -la && echo foo");

        assert_eq!(commands.len(), 2);
        assert_eq!(separators.len(), 1);
        assert_eq!(commands[0].args_with_cmd[0], "ls".to_string());
        assert_eq!(commands[0].args_with_cmd[1], "-la".to_string());
        assert_eq!(separators[0], Separator::LogicalAnd);
        assert_eq!(commands[1].args_with_cmd[0], "echo".to_string());
        assert_eq!(commands[1].args_with_cmd[1], "foo".to_string());

        let (commands, separators) = check("cd /tmp && pwd");

        assert_eq!(commands.len(), 2);
        assert_eq!(separators.len(), 1);
        assert_eq!(commands[0].args_with_cmd[0], "cd".to_string());
        assert_eq!(commands[0].args_with_cmd[1], "/tmp".to_string());
        assert_eq!(commands[1].args_with_cmd[0], "pwd".to_string());
    }

    #[test]
    fn test_cmd_parsing_with_multiple_separators() {
        // Test multiple separators together
        let (commands, separators) = check("false && echo foo || echo bar");

        assert_eq!(commands.len(), 3);
        assert_eq!(separators.len(), 2);
        assert_eq!(commands[0].args_with_cmd[0], "false".to_string());
        assert_eq!(separators[0], Separator::LogicalAnd);
        assert_eq!(commands[1].args_with_cmd[0], "echo".to_string());
        assert_eq!(commands[1].args_with_cmd[1], "foo".to_string());
        assert_eq!(separators[1], Separator::LogicalOr);
        assert_eq!(commands[2].args_with_cmd[0], "echo".to_string());
        assert_eq!(commands[2].args_with_cmd[1], "bar".to_string());
    }

    #[test]
    fn test_cmd_parsing_of_negate_exit_status() {
        // Testing parsing of `!`
        let (commands, separators) = check("! ls -la && echo foo");

        assert_eq!(commands.len(), 2);
        assert_eq!(separators.len(), 1);
        assert_eq!(commands[0].args_with_cmd[0], "ls".to_string());
        assert_eq!(commands[0].args_with_cmd[1], "-la".to_string());
        assert!(commands[0].negate_exit_status);
        assert_eq!(commands[1].args_with_cmd[0], "echo".to_string());
        assert_eq!(commands[1].args_with_cmd[1], "foo".to_string());
    }

    // #[test]
    // fn test_cmd_parsing_for_subshell_execution() {
    //     // Testing command parsing for subshell execution, i.e. within `()`
    //     let (commands, separators) = check("(cd /tmp && pwd) ; pwd");

    //     println!("commands: {commands:?}");
    //     assert_eq!(commands.len(), 3);
    //     assert_eq!(separators.len(), 2);
    //     assert_eq!(commands[0].args_with_cmd[0], "cd".to_string());
    //     assert_eq!(commands[0].args_with_cmd[1], "/tmp".to_string());
    //     assert_eq!(commands[1].args_with_cmd[0], "pwd".to_string());
    //     assert_eq!(commands[2].args_with_cmd[0], "pwd".to_string());
    // }
}
