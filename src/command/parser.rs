use super::{token::Token, Command};

pub struct Parser<'a> {
    tokens: &'a Vec<Token>,
}

pub struct ParseResult {
    cmds: Vec<Command>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a Vec<Token>) -> Self {
        Self {
            tokens
        }
    }

    fn get_command(&mut self) -> anyhow::Result<Option<ParseResult>> {
        Ok(None)
    }
}
