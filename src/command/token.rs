use std::fmt::Display;

#[derive(Debug, Clone)]
pub struct Token {
    pub lexeme: String,
    pub token_type: TokenType,
    pub line: usize,
    // (start, end)
    pub range: (usize, usize),
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.lexeme)
    }
}

#[derive(Debug, Clone)]
pub enum TokenType {
    Word(Word),
    Operators(Operators),
    LeftParen,
    RightParen,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            TokenType::Word(word) => word.to_string(),
            TokenType::Operators(op) => op.to_string(),
            TokenType::LeftParen => "(".into(),
            TokenType::RightParen => ")".into(),
        };

        write!(f, "{}", variant_str)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Word {
    Text,
    Keyword(Keyword),
}

impl Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            Word::Text => "text".into(),
            Word::Keyword(keyword) => keyword.to_string(),
        };

        write!(f, "{}", variant_str)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Keyword {
    Exit,
}

impl Display for Keyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            Keyword::Exit => "exit",
        };

        write!(f, "{}", variant_str)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operators {
    And, // "&"
    AndIf, // "&&"
    Or,  // "|"
    OrIf,  // "||"
    Semicolon,  // ";"
    Exclamation,  // "!"
}

impl Display for Operators {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            Operators::AndIf => "&&",
            Operators::OrIf => "||",
            Operators::Semicolon => ";",
            Operators::And => "&",
            Operators::Or => "|",
            Operators::Exclamation => "!",
        };

        write!(f, "{}", variant_str)
    }
}
