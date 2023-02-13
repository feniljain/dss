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

#[derive(Debug)]
pub enum OpType {
    RedirectOutput(Option<i32>),
    RedirectInput(Option<i32>),
    OrIf,
    Or,
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
                // FIXME: Add support for `>>`, `<>`
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
                    break;
                }
                TokenType::Operator(Operator::RightPointyBracket) => {
                    if let Some(last_token) = tokens.last() {
                        if let Ok(fd) = last_token.to_string().parse::<i32>() {
                            tokens.pop();
                            parse_result.associated_operator =
                                Some(OpType::RedirectOutput(Some(fd)));
                        } else {
                            parse_result.associated_operator =
                                Some(OpType::RedirectOutput(None));
                        }
                    }
                    break;
                }
                TokenType::LeftParen => {
                    capture_only_tokens = true;
                }
                TokenType::RightParen => {
                    parse_result.execute_mode = ExecuteMode::Subshell(tokens.clone());
                    capture_only_tokens = false;
                }
                TokenType::Operator(Operator::Or) => {
                    parse_result.associated_operator = Some(OpType::Or);
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
                let mut is_unqualified_path = true;
                if cmd_path.starts_with("./")
                    || cmd_path.starts_with("../")
                    || cmd_path.starts_with("/")
                {
                    is_unqualified_path = false;
                }

                let cmd = Command {
                    tokens,
                    path: cmd_path,
                    negate_exit_status,
                    is_unqualified_path,
                };

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
            OpType::Or => "|".into(),
            // OpType::Exclamation => "!".into(),
            OpType::RedirectOutput(fd_opt) => match fd_opt {
                Some(fd) => format!("{}, <", fd),
                None => "<".into(),
            },
            OpType::RedirectInput(fd_opt) => match fd_opt {
                Some(fd) => format!("{}, >", fd),
                None => ">".into(),
            },
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
}
