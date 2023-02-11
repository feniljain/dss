use std::fmt::Display;

#[derive(Debug, Clone)]
pub struct Token {
    pub lexeme: String,
    pub token_type: TokenType,
    pub line: usize,
    pub range: (usize, usize), // (start, end)
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.lexeme)
    }
}

#[derive(Debug, Clone)]
pub enum TokenType {
    Word(Word),
    Operator(Operator),
    LeftParen, // "("
    RightParen, // ")"
    Backslash,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            TokenType::Word(word) => word.to_string(),
            TokenType::Operator(op) => op.to_string(),
            TokenType::LeftParen => "(".into(),
            TokenType::RightParen => ")".into(),
            TokenType::Backslash => "\\".into(),
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
pub enum Operator {
    And, // "&"
    AndIf, // "&&"
    Or,  // "|"
    OrIf,  // "||"
    Semicolon,  // ";"
    Exclamation,  // "!"
    LeftPointyBracket,  // "<"
    RightPointyBracket,  // ">"
}

impl Display for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            Operator::AndIf => "&&",
            Operator::OrIf => "||",
            Operator::Semicolon => ";",
            Operator::And => "&",
            Operator::Or => "|",
            Operator::Exclamation => "!",
            Operator::LeftPointyBracket => "<",
            Operator::RightPointyBracket => ">",
        };

        write!(f, "{}", variant_str)
    }
}
