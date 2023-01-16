mod command;
mod engine;
mod errors;
mod writer;

use signal_hook::consts;
use writer::{Color, write_to_shell, write_to_shell_colored};
use std::{sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
}, io, ops::ControlFlow};

use command::{Command, Separator};
use engine::Engine;

// FIXME: Handle error properly everywhere using ShellError
// FIXME: Remove all unnecessary clones
// FIXME: Refine APIs exposed by Engine and Command

fn main() -> anyhow::Result<()> {
    write_to_shell("Welcome to Dead Simple Shell!\n")?;

    // FIXME: Move all variables here to engine/command struct

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(consts::SIGINT, Arc::clone(&term))?;

    let mut engine = Engine::new();

    while !term.load(Ordering::Relaxed) {
        if !engine.execution_successful {
            write_to_shell_colored("$ ", Color::Red)?;
        } else {
            write_to_shell_colored("$ ", Color::Green)?;
        }

        let mut input_str = String::new();

        io::stdin().read_line(&mut input_str)?;

        input_str = input_str.to_string();

        if input_str.trim() == "" {
            continue;
        }

        if input_str == "exit" {
            break;
        }

        let (commands, separators) = Command::parse_input(input_str)?;
        if separators.len() == 0 {
            assert!(commands.len() == 1);

            if commands[0].args_with_cmd[0] == "exit" {
                break;
            }

            engine.execute_command_by_forking(commands[0].clone())?;
        } else {
            let mut execution_result: Option<(bool, &Separator)> = None;
            let n_cmds = commands.len();
            let mut break_term_loop = false;
            commands.iter().enumerate().for_each(|(i, command)| {
                if command.args_with_cmd[0] == "exit" {
                    break_term_loop = true;
                    ControlFlow::Break::<bool>(true);
                    return;
                }

                // FIXME: Do not ignore result of execution here
                match execution_result {
                    Some((last_execution_result, separator)) => {
                        match separator {
                            Separator::Semicolon => {
                                let _ = engine.execute(command.clone());
                            }
                            Separator::LogicalOr => {
                                if !last_execution_result {
                                    let _ = engine.execute(command.clone());
                                }
                            }
                            Separator::LogicalAnd => {
                                if last_execution_result {
                                    let _ = engine.execute(command.clone());
                                }
                            }
                        }

                        if i < n_cmds - 1 {
                            execution_result = Some((engine.execution_successful, &separators[i]));
                        }
                    }
                    None => {
                        let _ = engine.execute(command.clone());
                        execution_result = Some((engine.execution_successful, &separators[i]));
                    }
                }
            });

            if break_term_loop {
                break;
            }
        }
    }

    Ok(())
}
