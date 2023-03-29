use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("dss: command not found: {0}\n")]
    CommandNotFound(String),
    #[error("dss: parse error: could not parse: {0}\n")]
    ParseError(String),
    #[error("dss: scan error: {0}\n")]
    LexError(LexError),
    #[error("dss: internal error [BUG]: {0}\n")]
    InternalError(String),
    #[error("dss: engine error: {0}\n")]
    EngineError(String),
}

#[derive(Error, Debug)]
pub enum LexError {
    #[error("dss: syntax error: {message} on line: {line} for range: {range:?}\n")]
    SyntaxError {
        message: String,
        line: usize,
        range: (usize, usize),
    },
}
