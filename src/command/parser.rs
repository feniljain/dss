use std::{fmt::Display, path::PathBuf, str::FromStr};

use crate::errors::ShellError;

use super::{
    token::{Keyword, Operator, Token, TokenType, Word},
    Command,
};

#[derive(Debug)]
pub struct Parser<'a> {
    tokens: &'a Vec<Token>,
    idx: usize,
}

#[derive(Debug, Clone)]
pub enum OpType {
    RedirectOutput(Option<i32>),
    RedirectInput(Option<i32>),
    RedirectAppendOutput(Option<i32>),
    RedirectReadWrite(Option<i32>),
    // RedirectSquirrelOutput(Option<i32>),
    RedirectSquirrelOutput {
        // 2nd argument
        // None here means "minus"
        // because otherwise it will
        // be an error
        //
        // read this as source
        source: Option<i32>,
        // 1st argument
        // Here None means nothing
        // is present and
        // to use default fd of 1
        //
        // read this as target
        target: Option<i32>,
    },
    RedirectSquirrelInput {
        // same docs as output one
        source: Option<i32>,
        target: Option<i32>,
    },
    OrIf,
    Pipe,
    AndIf,
    Semicolon,
}

#[derive(Debug)]
pub enum ExecuteMode {
    Normal,
    Subshell(Vec<Token>),
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a Vec<Token>) -> Self {
        Self { tokens, idx: 0 }
    }

    // There are two types of parsing modes:
    // - First, breaking on separators( i.e. &&, ||, etc ).
    // These ones return ls && echo as:
    //  - ls &&
    //  - echo
    // - Second, preemptively taking one argument in cases
    // like redirection, cause we can be sure over there that
    // there will only be one path/token after redirection
    // operator, so a command like: ls > file2
    // will be returned in parse_result as ls > file2 together
    pub fn get_command(&mut self) -> anyhow::Result<Option<ParseResult>> {
        if self.idx >= self.tokens.len() {
            // all commands are done
            return Ok(None);
        }

        let mut parse_result = ParseResult::new();

        let mut tokens = vec![];
        let mut first_token = true;
        let mut cmd_path = None;
        let mut negate_exit_status = false;
        let mut capture_only_tokens = false; // This is for subshell mode

        while self.idx < self.tokens.len() {
            let token = self.tokens[self.idx].clone();
            self.idx += 1;

            if capture_only_tokens && !matches!(token.token_type, TokenType::RightParen) {
                tokens.push(token);
                continue;
            }

            match &token.token_type {
                TokenType::Word(Word::Text) => {
                    if first_token {
                        cmd_path = Some(PathBuf::from_str(&token.lexeme).expect(&format!(
                            "Could not construct path buf from token: {}",
                            token.lexeme
                        )));
                        first_token = false;
                    }

                    tokens.push(token);
                }
                TokenType::Word(Word::Keyword(keyword)) => match keyword {
                    Keyword::Exit => {
                        parse_result.exit_term = true;
                    }
                },
                TokenType::Operator(Operator::OrIf) => {
                    parse_result.associated_operator = Some(OpType::OrIf);
                    break;
                }
                TokenType::Operator(Operator::AndIf) => {
                    parse_result.associated_operator = Some(OpType::AndIf);
                    break;
                }
                TokenType::Operator(Operator::Semicolon) => {
                    parse_result.associated_operator = Some(OpType::Semicolon);
                    break;
                }
                TokenType::Operator(Operator::Exclamation) => {
                    if !first_token {
                        return Err(
                            ShellError::ParseError("! found in invalid place".into()).into()
                        );
                        // we don't turn first_token to false here cause
                        // in next loop cycle we want it to be parsed
                        // as pathbuf
                    }
                    negate_exit_status = true;
                }
                // FIXME: use macro to remove this repeated code
                // for redirect operators below
                TokenType::Operator(Operator::LeftPointyBracket) => {
                    if let Some(last_token) = tokens.last() {
                        if let Ok(fd) = last_token.to_string().parse::<i32>() {
                            tokens.pop();
                            parse_result.associated_operator =
                                Some(OpType::RedirectInput(Some(fd)));
                        } else {
                            parse_result.associated_operator = Some(OpType::RedirectInput(None));
                        }
                    }

                    let cmds = self.handle_pointy_bracket_redirection_cmd_gen(
                        tokens,
                        cmd_path.expect("expected command path to exist"),
                        negate_exit_status,
                    );

                    for cmd in cmds.into_iter() {
                        parse_result.cmds.push(cmd);
                    }

                    return Ok(Some(parse_result));
                }
                TokenType::Operator(Operator::RightPointyBracket) => {
                    if let Some(last_token) = tokens.last() {
                        if let Ok(fd) = last_token.to_string().parse::<i32>() {
                            tokens.pop();
                            parse_result.associated_operator =
                                Some(OpType::RedirectOutput(Some(fd)));
                        } else {
                            parse_result.associated_operator = Some(OpType::RedirectOutput(None));
                        }
                    }

                    let cmds = self.handle_pointy_bracket_redirection_cmd_gen(
                        tokens,
                        cmd_path.expect("expected command path to exist"),
                        negate_exit_status,
                    );

                    for cmd in cmds.into_iter() {
                        parse_result.cmds.push(cmd);
                    }

                    return Ok(Some(parse_result));
                }
                TokenType::Operator(Operator::DoubleRightPointyBracket) => {
                    if let Some(last_token) = tokens.last() {
                        if let Ok(fd) = last_token.to_string().parse::<i32>() {
                            tokens.pop();
                            parse_result.associated_operator =
                                Some(OpType::RedirectAppendOutput(Some(fd)));
                        } else {
                            parse_result.associated_operator =
                                Some(OpType::RedirectAppendOutput(None));
                        }
                    }

                    let cmds = self.handle_pointy_bracket_redirection_cmd_gen(
                        tokens,
                        cmd_path.expect("expected command path to exist"),
                        negate_exit_status,
                    );

                    for cmd in cmds.into_iter() {
                        parse_result.cmds.push(cmd);
                    }

                    return Ok(Some(parse_result));
                }
                TokenType::Operator(Operator::DiamondPointyBrackets) => {
                    if let Some(last_token) = tokens.last() {
                        if let Ok(fd) = last_token.to_string().parse::<i32>() {
                            tokens.pop();
                            parse_result.associated_operator =
                                Some(OpType::RedirectReadWrite(Some(fd)));
                        } else {
                            parse_result.associated_operator =
                                Some(OpType::RedirectReadWrite(None));
                        }
                    }

                    let cmds = self.handle_pointy_bracket_redirection_cmd_gen(
                        tokens,
                        cmd_path.expect("expected command path to exist"),
                        negate_exit_status,
                    );

                    for cmd in cmds.into_iter() {
                        parse_result.cmds.push(cmd);
                    }

                    return Ok(Some(parse_result));
                }
                TokenType::Operator(Operator::SquirrelOutput) => {
                    if let Some(last_token) = tokens.last() {
                        let target_fd_opt = if let Ok(fd) = last_token.to_string().parse::<i32>() {
                            tokens.pop();
                            Some(fd)
                        } else {
                            None
                        };

                        let fd_or_minus_not_found_err = Err(ShellError::ParseError(
                            "expected file descriptor or minus after squirrel redirection operator"
                                .into(),
                        )
                        .into());

                        let maybe_fd_or_minus_token = self.tokens[self.idx].clone();
                        self.idx += 1;

                        let t = maybe_fd_or_minus_token;
                        let fd_or_minus_token = if t.to_string() == "-" {
                            None
                        } else if let Ok(fd) = t.to_string().parse::<i32>() {
                            Some(fd)
                        } else {
                            return fd_or_minus_not_found_err;
                        };

                        parse_result.associated_operator = Some(OpType::RedirectSquirrelOutput {
                            source: fd_or_minus_token,
                            target: target_fd_opt,
                        });
                    }

                    let cmd = make_command(
                        tokens,
                        cmd_path.expect("expected command path to exist"),
                        negate_exit_status,
                    );
                    parse_result.cmds.push(cmd);

                    return Ok(Some(parse_result));
                }
                TokenType::Operator(Operator::SquirrelInput) => {
                    if let Some(last_token) = tokens.last() {
                        let target_fd_opt = if let Ok(fd) = last_token.to_string().parse::<i32>() {
                            tokens.pop();
                            Some(fd)
                        } else {
                            None
                        };

                        let fd_or_minus_not_found_err = Err(ShellError::ParseError(
                            "expected file descriptor or minus after squirrel redirection operator"
                                .into(),
                        )
                        .into());

                        let maybe_fd_or_minus_token = self.tokens[self.idx].clone();
                        self.idx += 1;

                        let t = maybe_fd_or_minus_token;
                        let fd_or_minus_token = if t.to_string() == "-" {
                            None
                        } else if let Ok(fd) = t.to_string().parse::<i32>() {
                            Some(fd)
                        } else {
                            return fd_or_minus_not_found_err;
                        };

                        parse_result.associated_operator = Some(OpType::RedirectSquirrelInput {
                            source: fd_or_minus_token,
                            target: target_fd_opt,
                        });
                    }

                    let cmd = make_command(
                        tokens,
                        cmd_path.expect("expected command path to exist"),
                        negate_exit_status,
                    );
                    parse_result.cmds.push(cmd);

                    return Ok(Some(parse_result));
                }
                TokenType::LeftParen => {
                    capture_only_tokens = true;
                }
                TokenType::RightParen => {
                    parse_result.execute_mode = ExecuteMode::Subshell(tokens.clone());
                    capture_only_tokens = false;
                }
                TokenType::Operator(Operator::Or) => {
                    parse_result.associated_operator = Some(OpType::Pipe);
                    break;
                }
                TokenType::Operator(Operator::And) => unreachable!(),
                TokenType::Backslash => {}
            }
        }

        if matches!(parse_result.execute_mode, ExecuteMode::Subshell(_)) {
            return Ok(Some(parse_result));
        }

        match cmd_path {
            Some(cmd_path) => {
                let cmd = make_command(tokens, cmd_path, negate_exit_status);

                parse_result.cmds.push(cmd);

                return Ok(Some(parse_result));
            }
            None => {
                if !parse_result.exit_term {
                    return Err(ShellError::InternalError("could not find cmd_path".into()).into());
                }

                return Ok(Some(parse_result));
            }
        }
    }

    fn handle_pointy_bracket_redirection_cmd_gen(
        &mut self,
        tokens: Vec<Token>,
        cmd_path: PathBuf,
        negate_exit_status: bool,
    ) -> Vec<Command> {
        // Construct command before redirect operator
        let cmd = make_command(tokens, cmd_path, negate_exit_status);

        let file_path_cmd = self.make_file_path_cmd();

        return vec![cmd, file_path_cmd];
    }

    fn make_file_path_cmd(&mut self) -> Command {
        // Construct command after redirect operator
        let file_path_token = self.tokens[self.idx].clone();
        self.idx += 1;

        let file_path = PathBuf::from_str(&file_path_token.lexeme).expect(&format!(
            "Could not construct path buf from token: {}",
            file_path_token.lexeme
        ));

        let file_path_cmd = make_command(vec![file_path_token], file_path, false);
        return file_path_cmd;
    }
}

fn make_command(tokens: Vec<Token>, cmd_path: PathBuf, negate_exit_status: bool) -> Command {
    let mut is_unqualified_path = true;
    if cmd_path.starts_with("./") || cmd_path.starts_with("../") || cmd_path.starts_with("/") {
        is_unqualified_path = false;
    }

    return Command {
        tokens,
        path: cmd_path,
        negate_exit_status,
        is_unqualified_path,
    };
}

#[derive(Debug)]
pub struct ParseResult {
    // cmds is only needed because of subshell commands
    // this would otherwise only be 1 element otherwise
    pub cmds: Vec<Command>,
    pub execute_mode: ExecuteMode,
    pub exit_term: bool,
    pub associated_operator: Option<OpType>,
}

impl ParseResult {
    fn new() -> Self {
        Self {
            cmds: vec![],
            execute_mode: ExecuteMode::Normal,
            exit_term: false,
            associated_operator: None,
        }
    }
}

impl Display for OpType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            OpType::AndIf => "&&".into(),
            OpType::OrIf => "||".into(),
            OpType::Semicolon => ";".into(),
            // OpType::And => "&".into(),
            OpType::Pipe => "|".into(),
            // OpType::Exclamation => "!".into(),
            OpType::RedirectOutput(fd_opt) => match fd_opt {
                Some(fd) => format!("{}, <", fd),
                None => "<".into(),
            },
            OpType::RedirectInput(fd_opt) => match fd_opt {
                Some(fd) => format!("{}, >", fd),
                None => ">".into(),
            },
            OpType::RedirectAppendOutput(fd_opt) => match fd_opt {
                Some(fd) => format!("{}, >>", fd),
                None => ">".into(),
            },
            OpType::RedirectReadWrite(fd_opt) => match fd_opt {
                Some(fd) => format!("{}, <>", fd),
                None => ">".into(),
            },
            OpType::RedirectSquirrelOutput { source, target } => {
                let target_fd_str = match target {
                    Some(fd) => format!("{}", fd),
                    None => "-".into(),
                };

                let source_fd_str = match source {
                    Some(fd) => format!("{}", fd),
                    None => "".into(),
                };

                format!("{}&>{}", target_fd_str, source_fd_str)
            }
            OpType::RedirectSquirrelInput { source, target } => {
                let target_fd_str = match target {
                    Some(fd) => format!("{}", fd),
                    None => "-".into(),
                };

                let source_fd_str = match source {
                    Some(fd) => format!("{}", fd),
                    None => "".into(),
                };

                format!("{}&>{}", target_fd_str, source_fd_str)
            }
        };

        write!(f, "{}", variant_str)
    }
}

#[cfg(test)]
mod tests {
    use crate::command::{lexer::Lexer, token::Token};

    use super::{ParseResult, Parser};

    fn check(tokens: &Vec<Token>) -> anyhow::Result<Vec<ParseResult>> {
        let mut parser = Parser::new(tokens);
        let mut results = vec![];
        while let Some(parse_result) = parser.get_command()? {
            results.push(parse_result);
        }

        Ok(results)
    }

    fn get_tokens(input_str: &str) -> anyhow::Result<Lexer> {
        let mut lexer = Lexer::new();
        lexer.scan(input_str)?;
        Ok(lexer)
    }

    #[test]
    fn test_simple_cmd_parsing() {
        let lexer = get_tokens("ls\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_with_args_parsing() {
        let lexer = get_tokens("ls -la\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_with_unqualified_path() {
        let lexer = get_tokens("./ls -la\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_with_semicolon_separator() {
        let lexer = get_tokens("ls -la ; echo foo\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_with_or_if_separator() {
        let lexer = get_tokens("ls -la || echo foo\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_with_and_if_separator() {
        let lexer = get_tokens("ls -la && ./echo foo\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_with_multiple_separators() {
        let lexer =
            get_tokens("false && echo foo || echo bar\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_exit_term() {
        let lexer = get_tokens("ls -la && exit\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_subshell() {
        let lexer = get_tokens("(ls && exit)\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);

        let lexer = get_tokens("(ls && exit) && ls\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_pipe_ops() {
        let lexer = get_tokens("echo foo | cat | cat\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_ops_with_fd() {
        let lexer = get_tokens("ls -6 2> file.txt\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_ops_without_fd() {
        let lexer = get_tokens("ls -6> file.txt\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_append_ops_without_fd() {
        let lexer = get_tokens("ls -la >> file.txt\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_append_ops_with_fd() {
        let lexer = get_tokens("ls -la 2>> file.txt\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_read_write_ops_without_fd() {
        let lexer = get_tokens("ls -la <> file.txt\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_read_write_ops_with_fd() {
        let lexer = get_tokens("ls -la 2<> file.txt\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_squirrel_output_ops_with_fd() {
        let lexer =
            get_tokens("ls /tmp/ doesnotexist 2&>1\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_squirrel_output_ops_without_fd() {
        let lexer =
            get_tokens("ls /tmp/ doesnotexist &>1\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_squirrel_output_ops_with_minus() {
        let lexer =
            get_tokens("ls /tmp/ doesnotexist &>1\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_squirrel_input_ops_with_fd() {
        let lexer = get_tokens("ls 0<&1\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_of_redirection_squirrel_input_ops_without_fd() {
        let lexer = get_tokens("ls <&1\n").expect("lexer failed, check lexer tests");
        let results = check(&lexer.tokens).expect("parser failed :(");
        insta::assert_debug_snapshot!(results);
    }

    #[test]
    fn test_cmd_parsing_for_bg_process_invocation() {
        // let lexer = get_tokens("ls -la <& file.txt\n").expect("lexer failed, check lexer tests");
        // let results = check(&lexer.tokens).expect("parser failed :(");
        // insta::assert_debug_snapshot!(results);
    }
}
