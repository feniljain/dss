use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("dss: command not found: {0}\n")]
    CommandNotFound(String),
    #[error("dss: parse error: could not parse: {0}\n")]
    ParseError(String),
}
