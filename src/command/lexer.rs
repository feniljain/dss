// Tokenization Spec:
// - URL: https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_03
// I hoped to implement this :sadge:

use std::{iter::Peekable, str::Chars};

use crate::errors::{LexError, ShellError};

use super::token::{Keyword, Operator, Token, TokenType, Word};

pub struct Lexer {
    // TODO: Remove tokens field
    pub tokens: Vec<Token>,
    // ctx: LexingContext<'a>,
}

struct LexingContext<'a> {
    line: usize,
    chars: Peekable<Chars<'a>>,
    // offset from new line
    offset: usize,
    tokens: Vec<Token>,
    // last_token_type: Option<TokenType>,
    word: String,
}

impl Lexer {
    pub fn new() -> Self {
        Self {
            tokens: vec![],
            // ctx: LexingContext {
            //     line: 0,
            //     chars: "c".chars().peekable(),
            //     offset: 0,
            //     word: String::new(),
            //     tokens: &mut vec![],
            // },
        }
    }

    pub fn scan(&mut self, input_str: &str) -> anyhow::Result<Vec<Token>> {
        let mut ctx = LexingContext {
            line: 0,
            chars: input_str.chars().peekable(),
            offset: 0,
            tokens: vec![],
            word: String::new(),
        };

        ctx.scan()?;
        Ok(ctx.tokens)
    }

    pub fn complete_processing(&self, last_token: Token) -> bool {
        // if it's backslash -> not completed processing
        // if it's any operator other than & -> not completed processing

        if matches!(last_token.token_type, TokenType::Backslash) {
            return false;
        }

        if matches!(last_token.token_type, TokenType::Operator(_))
            && !matches!(last_token.token_type, TokenType::Operator(Operator::And))
        {
            return false;
        }

        return true;
    }
}

impl<'a> LexingContext<'a> {
    pub fn scan(&mut self) -> anyhow::Result<()> {
        while let Some(ch) = self.eat() {
            match ch {
                '\n' => {
                    self.line += 1;
                    self.offset = 0;
                    self.word = String::new();
                }
                '&' => {
                    let next_char = self.peek();
                    if next_char == Some(&'&') {
                        self.eat();
                        self.add_token(TokenType::Operator(Operator::AndIf));
                    } else if next_char == Some(&'>') {
                        self.eat();
                        self.add_token(TokenType::Operator(Operator::SquirrelOutput));
                    } else {
                        self.add_token(TokenType::Operator(Operator::And));
                    }
                }
                '|' => {
                    if self.peek() == Some(&'|') {
                        self.eat();
                        self.add_token(TokenType::Operator(Operator::OrIf));
                    } else {
                        self.add_token(TokenType::Operator(Operator::Or));
                    }
                }
                ';' => {
                    self.add_token(TokenType::Semicolon);
                }
                '!' => self.add_token(TokenType::Operator(Operator::Exclamation)),
                '(' => self.add_token(TokenType::LeftParen),
                ')' => {
                    self.add_token(TokenType::RightParen);
                }
                '\\' => {
                    self.add_token(TokenType::Backslash);
                }
                '<' => {
                    let next_char = self.peek();
                    if next_char == Some(&'>') {
                        self.eat();
                       self.add_token(TokenType::Operator(Operator::DiamondPointyBrackets));
                    } else if next_char == Some(&'&') {
                        self.eat();
                        self.add_token(TokenType::Operator(Operator::SquirrelInput));
                    } else {
                        self.add_token(TokenType::Operator(Operator::LeftPointyBracket));
                    }
                }
                '>' => {
                    let next_char = self.peek();
                    if next_char == Some(&'>') {
                        self.eat();
                        self.add_token(TokenType::Operator(Operator::DoubleRightPointyBracket));
                    } else {
                        self.add_token(TokenType::Operator(Operator::RightPointyBracket));
                    }
                }
                ' ' => {
                    // We want to clear the word cause otherwise it will
                    // contain space as a char
                    self.word = String::new();
                }
                ch if is_valid_name_char(ch) => {
                    self.eat_while(is_valid_name_char);
                    let token_type = match self.word.as_str() {
                        "exit" => TokenType::Word(Word::Keyword(Keyword::Exit)),
                        _ => TokenType::Word(Word::Text),
                    };
                    self.add_token(token_type);
                }
                _ => {
                    return Err(ShellError::LexError(LexError::SyntaxError {
                        message: "unexpected character".to_string(),
                        line: self.line,
                        range: (self.offset, self.offset + 1),
                    })
                    .into())
                }
            }
        }

        Ok(())
    }

    fn eat_while(&mut self, predicate: impl Fn(char) -> bool) {
        loop {
            let Some(ch) = self.chars.peek() else {
                break;
            };

            if !predicate(*ch) {
                break;
            }

            self.eat();
        }
    }

    fn eat(&mut self) -> Option<char> {
        let Some(ch) = self.chars.next() else {
            return None;
        };

        self.word.push(ch);
        self.offset += 1;
        return Some(ch);
    }

    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    fn add_token(&mut self, token_type: TokenType) {
        // let lexeme: String = lexeme.into();
        let lexeme = &self.word;
        let len = lexeme.len();

        /*
         * If it is indicated that a token is delimited, and no characters have been included
         * in a token, processing shall continue until an actual token is delimited.
         */
        if lexeme == "" {
            return;
        }

        // let mut start_offset = self.offset;
        // let mut end_offset = self.offset;
        let start_offset = self.offset - len;
        let end_offset = self.offset - 1;

        //match &token_type {
        //    TokenType::Word(_) => {}
        //    // TokenType::LeftPointyBracket(fd_opt) | TokenType::RightPointyBracket(fd_opt) => {
        //    //     if let Some(fd) = fd_opt {
        //    //         start_offset -= fd.to_string().len();
        //    //     }
        //    // }
        //    _ => {
        //        // For other tokens, we evaluated them
        //        // as soon we find them, we do not wait to
        //        // delimit like words, so receive offset at
        //        //
        //        // &&
        //        //  ^
        //        //  offset point received
        //        //
        //        //  This is the reason we take the space till
        //        //  len - 1
        //        start_offset -= len - 1;
        //    }
        //}

        let token = Token {
            lexeme: lexeme.to_string(),
            token_type,
            line: self.line,
            range: (start_offset, end_offset),
        };
        self.tokens.push(token);
        self.word = String::new();
    }
}

fn is_valid_name_char(ch: char) -> bool {
    is_alpha_numeric(ch) || is_valid_name_special_char(ch)
}

fn is_valid_name_special_char(ch: char) -> bool {
    ch == '_'
        || ch == '-'
        || ch == '.'
        || ch == '/'
        || ch == '"'
        || ch == '$'
        || ch == '{'
        || ch == '}'
}

fn is_alpha_numeric(ch: char) -> bool {
    return is_alpha(ch) || is_digit(ch);
}

fn is_digit(ch: char) -> bool {
    return ch >= '0' && ch <= '9';
}

fn is_alpha(ch: char) -> bool {
    return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || ch == '_';
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(input_str: &str) -> Vec<Token> {
        let mut lexer = Lexer::new();

        let tokens = lexer.scan(input_str).expect("lexing should have succeeded");

        tokens
    }

    // Do not keep insta::assert_debug_snapshot!(lexer.tokens)
    // as common code in check because it forms snapshots with name
    // `check-{i}`; 1 <= 0 <= n
    //
    // we instead want test function names as the snapshot names

    #[test]
    fn test_simple_cmd_lexing() {
        let tokens = check("ls\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_cmd_with_args_lexing() {
        let tokens = check("ls -la\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_cmd_lexing_of_semicolon_separator() {
        let tokens = check("ls -la ; echo foo\n");
        insta::assert_debug_snapshot!(tokens);

        let tokens = check("ls -la; echo foo\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_cmd_lexing_of_logical_or() {
        let tokens = check("ls -la || echo foo\n");
        insta::assert_debug_snapshot!(tokens);

        let tokens = check("ls -la|| echo foo\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_cmd_lexing_of_logical_and() {
        let tokens = check("ls -la && echo foo\n");
        insta::assert_debug_snapshot!(tokens);

        let tokens = check("ls -la &&echo foo\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_cmd_lexing_with_multiple_separators() {
        let tokens = check("false && echo foo || echo bar\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_cmd_lexing_of_negate_exit_status() {
        let tokens = check("! ls -la && echo foo\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_subshell_cmds() {
        let tokens = check("(! ls -la)&& echo foo\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_cmd_with_keyword() {
        let tokens = check("ls -la&& exit\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_cmd_cd_dot_dot() {
        let tokens = check("cd ..\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_cmd_with_unqualified_path() {
        let tokens = check("./ls\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_backslash() {
        let tokens = check("echo \\\nfoo\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_pipe_op() {
        let tokens = check("echo foo | cat | cat\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_pipe_op_with_redirection_with_fd() {
        let tokens = check("ls -6 2> file.txt\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_redirection_without_fd() {
        let tokens = check("ls > file.txt\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_redirection_op_append_with_fd() {
        let tokens = check("ls 6>> file.txt\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_redirection_op_append_without_fd() {
        let tokens = check("ls >> file.txt\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_redirection_op_diamond_without_fd() {
        let tokens = check("ls <> file.txt\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_redirection_op_diamond_with_fd() {
        let tokens = check("ls 2<> file.txt\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_squirrel_output_op_with_fd() {
        let tokens = check("ls /tmp/ doesnotexist 2&>1\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_squirrel_output_op_without_fd() {
        let tokens = check("ls /tmp/ doesnotexist &>1\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_squirrel_input_op_with_fd() {
        let tokens = check("0<&1\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_squirrel_input_op_without_fd() {
        let tokens = check("<&1\n");
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn test_lexing_of_bg_process_with_ampersand() {
        let tokens = check("ping google.com &\n");
        insta::assert_debug_snapshot!(tokens);
    }
}
