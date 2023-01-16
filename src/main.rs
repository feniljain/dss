mod command;
mod engine;
mod errors;
mod writer;

use engine::Engine;

// FIXME: Handle error properly everywhere using ShellError
// FIXME: Remove all unnecessary clones
// FIXME: Refine APIs exposed by Engine and Command

fn main() -> anyhow::Result<()> {
    let mut engine = Engine::new();

    engine.fire_on()?;

    Ok(())
}
