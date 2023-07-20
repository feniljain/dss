mod command;
mod engine;
mod errors;
mod frontend;

use engine::Engine;

// FIXME: Handle error properly everywhere using ShellError
// FIXME: Remove all unnecessary clones
// FIXME: Refine APIs exposed by Engine and Command

fn main() -> anyhow::Result<()> {
    // FIXME: calculate this using process described here: https://www.gnu.org/software/libc/manual/html_node/Initializing-the-Shell.html
    let is_interactive = true;
    let mut engine = Engine::new(is_interactive)?;

    engine.fire_on()?;

    Ok(())
}
