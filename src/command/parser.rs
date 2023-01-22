use std::{path::PathBuf, str::FromStr};

use crate::errors::ShellError;

use super::{
    token::{Keyword, Operator, Token, TokenType, Word},
    Command,
};

#[derive(Debug)]
pub struct Parser<'a> {
    tokens: &'a Vec<Token>,
    tokens_len: usize,
    idx: usize,
}

#[derive(Debug)]
pub enum ExecuteMode {
    Normal,
    Subshell(Vec<Token>),
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a Vec<Token>, tokens_len: usize) -> Self {
        Self {
            tokens,
            tokens_len,
            idx: 0,
        }
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

        while self.idx < self.tokens_len {
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
                    parse_result.operator_for_next_exec = Some(Operator::OrIf);
                    break;
                }
                TokenType::Operator(Operator::AndIf) => {
                    parse_result.operator_for_next_exec = Some(Operator::AndIf);
                    break;
                }
                TokenType::Operator(Operator::Semicolon) => {
                    parse_result.operator_for_next_exec = Some(Operator::Semicolon);
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
                TokenType::LeftParen => {
                    capture_only_tokens = true;
                }
                TokenType::RightParen => {
                    parse_result.execute_mode = ExecuteMode::Subshell(tokens.clone());
                    break;
                }
                TokenType::Operator(Operator::Or) => unreachable!(),
                TokenType::Operator(Operator::And) => unreachable!(),
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
    pub operator_for_next_exec: Option<Operator>,
}

impl ParseResult {
    fn new() -> Self {
        Self {
            cmds: vec![],
            execute_mode: ExecuteMode::Normal,
            exit_term: false,
            operator_for_next_exec: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::command::{lexer::Lexer, token::Token};

    use super::{ParseResult, Parser};

    fn check(tokens: &Vec<Token>) -> anyhow::Result<Vec<ParseResult>> {
        let mut parser = Parser::new(tokens, tokens.len());
        let mut results = vec![];
        while let Some(parse_result) = parser.get_command()? {
            results.push(parse_result);
        }

        Ok(results)
    }

    fn get_tokens(input_str: &str) -> anyhow::Result<Lexer> {
        let mut lexer = Lexer::new(input_str);
        lexer.scan()?;
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
    }
}
