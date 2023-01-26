use super::{Color, write_to_shell_colored};

#[derive(Debug)]
pub struct Prompt {
    letter: String,
    multiline_mode: bool,
    // color: Color,
}

impl Prompt {

    pub fn new() -> Self {
        Self {
            letter: "$ ".into(),
            multiline_mode: false,
        }
    }

    pub fn render(&self, execution_successful: bool) -> anyhow::Result<()> {
        let color = if self.multiline_mode {
            Color::White
        } else if execution_successful {
            Color::Green
        } else {
            Color::Red
        };

        write_to_shell_colored(&self.letter, color)?;
        Ok(())
    }

    pub fn activate_multiline_prompt(&mut self) {
        self.letter = "> ".into();
        self.multiline_mode = true;
    }

    pub fn deactivate_multiline_prompt(&mut self) {
        self.letter = "$ ".into();
        self.multiline_mode = false;
    }
}
