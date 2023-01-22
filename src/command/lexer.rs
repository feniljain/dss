use std::str::Chars;

use crate::errors::{LexError, ShellError};

use super::token::{Keyword, Operator, Token, TokenType, Word};

pub struct Lexer<'a> {
    pub tokens: Vec<Token>,
    itr: Chars<'a>,
    ctx: LexingContext,
}

struct LexingContext {
    line: usize,
    // offset from new line
    offset: usize,
    last_token_type: Option<TokenType>,
    word: String,
}

impl<'a> Lexer<'a> {
    pub fn new(input_str: &'a str) -> Self {
        Self {
            tokens: vec![],
            itr: input_str.chars(),
            ctx: LexingContext {
                line: 0,
                offset: 0,
                last_token_type: None,
                word: String::new(),
            },
        }
    }

    // Tokenization Spec:
    // - URL: https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_03
    // - SECTION: 2.3 Token Recognition
    pub fn scan(&mut self) -> anyhow::Result<&Vec<Token>> {
        // This clone becomes necessary cause otherwise,
        // if we take mutable borrow of this iterator
        // Rust thinks that add_token mutates `self`,
        // causing two mutable borrows, tho we know in
        // reality we don't
        //
        // partial borrows only work in same function,
        // not across function boundaries
        let mut itr = self.itr.clone();

        while let Some(ch) = itr.next() {
            match ch {
                '\n' => {
                    // FIXME:
                    /*
                     * 1. If the end of input is recognized, the current token (if any)
                     * shall be delimited.
                     */
                    self.delimit_word_and_add_token();

                    self.ctx.line += 1;
                    self.ctx.offset = 0;
                }
                /* 2. If the previous character was used as part of an operator and the
                 * current character is not quoted and can be used with the previous chars
                 * to form an operator, it shall be used as part of that (operator) token.
                 */
                '&' => {
                    if let Some(TokenType::Operator(op)) = &self.ctx.last_token_type.clone() {
                        if op == &Operator::And {
                            self.tokens.pop();
                            self.add_token("&&", TokenType::Operator(Operator::AndIf));
                        }
                    } else {
                        self.delimit_word_and_add_token();
                        self.ctx.last_token_type = Some(TokenType::Operator(Operator::And));
                        /* 3. If the previous character was used as part of an operator and
                         * the current char cannot be used with the previous chars to
                         * form an operator, the operator containing the previous char
                         * shall be delimited.
                         * */
                        self.add_token("&", TokenType::Operator(Operator::And));
                    }
                }
                '|' => {
                    if let Some(TokenType::Operator(op)) = &self.ctx.last_token_type {
                        if op == &Operator::Or {
                            self.tokens.pop();
                            self.add_token("||", TokenType::Operator(Operator::OrIf));
                        }
                    } else {
                        self.delimit_word_and_add_token();
                        self.ctx.last_token_type = Some(TokenType::Operator(Operator::Or));
                        self.add_token("|", TokenType::Operator(Operator::Or));
                    }
                }
                ';' => {
                    self.delimit_word_and_add_token();
                    self.add_token(";", TokenType::Operator(Operator::Semicolon));
                }

                '!' => self.add_token("!", TokenType::Operator(Operator::Exclamation)),
                '(' => self.add_token("(", TokenType::LeftParen),
                ')' => {
                    self.delimit_word_and_add_token();
                    self.add_token(")", TokenType::RightParen);
                }

                ' ' => self.delimit_word_and_add_token(),
                _ => {
                    if is_valid_name_char(ch) {
                        self.ctx.word.push(ch);
                    } else {
                        return Err(ShellError::LexError(LexError::SyntaxError {
                            message: "unexpected character".to_string(),
                            line: self.ctx.line,
                            range: (self.ctx.offset, self.ctx.offset + 1),
                        })
                        .into());
                    }
                }
            }
            self.ctx.offset += 1;
        }

        Ok(&self.tokens)
    }

    fn delimit_word_and_add_token(&mut self) {
        let token_type = match self.ctx.word.as_str() {
            "exit" => TokenType::Word(Word::Keyword(Keyword::Exit)),
            _ => TokenType::Word(Word::Text),
        };

        self.ctx.last_token_type = Some(token_type.clone());
        self.add_token(self.ctx.word.clone(), token_type);
        self.ctx.word = String::new();
    }

    fn add_token<T: Into<String>>(&mut self, lexeme: T, token_type: TokenType) {
        let lexeme: String = lexeme.into();
        let len = lexeme.len();

        /*
         * If it is indicated that a token is delimited, and no characters have been included
         * in a token, processing shall continue until an actual token is delimited.
         */
        if lexeme == "" {
            return;
        }

        let mut start_offset = self.ctx.offset;
        let mut end_offset = self.ctx.offset;

        match &token_type {
            TokenType::Word(_) => {
                // For words, we either delimit them on space or newline
                // so the offsets received are of the space or newline char
                //
                // ls
                //   ^
                //   offset point received
                //
                // This is the reason we remove `len` from offset
                // for starting point
                //
                // And we just remove 1 for ending point
                start_offset -= len;
                end_offset -= 1;
            }
            _ => {
                // For other tokens, we evaluated them
                // as soon we find them, we do not wait to
                // delimit like words, so receive offset at
                //
                // &&
                //  ^
                //  offset point received
                //
                //  This is the reason we the space till
                //  len - 1
                start_offset -= len - 1;
            }
        }

        let token = Token {
            lexeme,
            token_type,
            line: self.ctx.line,
            range: (start_offset, end_offset),
        };
        self.tokens.push(token);
    }
}

fn is_valid_name_char(ch: char) -> bool {
    is_alpha_numeric(ch) || is_valid_name_special_char(ch)
}

fn is_valid_name_special_char(ch: char) -> bool {
    ch == '_' || ch == '-' || ch == '.' || ch == '/'
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

    fn check<'a>(input_str: &'a str) -> Lexer<'a> {
        let mut lexer = Lexer::new(input_str);

        lexer.scan().expect("lexing should have succeeded");

        lexer
    }

    // Do not keep insta::assert_debug_snapshot!(lexer.tokens)
    // as common code in check because it forms snapshots with name
    // `check-{i}`; 1 <= 0 <= n
    //
    // we instead want test function names as the snapshot names

    #[test]
    fn test_simple_cmd_lexing() {
        let lexer = check("ls\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_cmd_with_args_lexing() {
        let lexer = check("ls -la\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_cmd_lexing_of_semicolon_separator() {
        let lexer = check("ls -la ; echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);

        let lexer = check("ls -la; echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_cmd_lexing_of_logical_or() {
        let lexer = check("ls -la || echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);

        let lexer = check("ls -la|| echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_cmd_lexing_of_logical_and() {
        let lexer = check("ls -la && echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);

        let lexer = check("ls -la &&echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_cmd_lexing_with_multiple_separators() {
        let lexer = check("false && echo foo || echo bar\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_cmd_lexing_of_negate_exit_status() {
        let lexer = check("! ls -la && echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_lexing_of_subshell_cmds() {
        let lexer = check("(! ls -la)&& echo foo\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_lexing_of_cmd_with_keyword() {
        let lexer = check("ls -la&& exit\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_lexing_of_cmd_cd_dot_dot() {
        let lexer = check("cd ..\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }

    #[test]
    fn test_lexing_of_cmd_with_unqualified_path() {
        let lexer = check("./ls\n");
        insta::assert_debug_snapshot!(lexer.tokens);
    }
}
