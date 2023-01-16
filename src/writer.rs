// Here we try to not use println as it can
// panic, more here:
// https://github.com/BurntSushi/advent-of-code/issues/17

use std::io::{self, Write};

use colored::Colorize;
use nix::errno::Errno;

#[derive(Debug)]
pub enum Color {
    Green,
    Red,
}

pub fn write_to_shell(output: &str) -> anyhow::Result<()> {
    io::stdout().write_all(output.as_bytes())?;

    // Flushing is important because:
    // https://stackoverflow.com/questions/34993744/why-does-this-read-input-before-printing
    io::stdout().flush().expect("flush failed!");

    Ok(())
}

pub fn write_to_shell_colored(output: &str, color: Color) -> anyhow::Result<()> {
    //FIXME: Figure out why colored doesn't work with write_all
    // and replace println here
    match color {
        Color::Red => print!("{}", output.red()),
        Color::Green => print!("{}", output.green()),
    }

    io::stdout().flush().expect("flush failed!");

    Ok(())
}

pub fn write_error_to_shell(
    errno: Errno,
    cmd_str: &str,
    is_unqualified_path: bool,
) -> anyhow::Result<()> {
    if is_unqualified_path {
        write_to_shell(&format!("dss: command not found: {}\n", cmd_str))?;
    } else {
        write_to_shell(&format!("dss: {}: {}\n", errno.desc(), cmd_str))?;
    }

    Ok(())
}
