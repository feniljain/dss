use nix::{
    sys::wait::waitpid,
    unistd::{execve, fork, ForkResult},
};
use std::{
    ffi::{CStr, CString},
    io::{self, Write},
};

const BUILTIN_COMMANDS: [&str; 1] = ["cd"];

#[derive(Default, Clone, Debug)]
struct Command {
    path: CString,
    args: Vec<CString>,
}

// FIXME: Handle error properly everywhere

fn main() -> io::Result<()> {
    io::stdout().write_all(b"Welcome to Dead Simple Shell!\n")?;

    loop {
        io::stdout().write_all(b"$ ")?;
        // Flushing is important because:
        // https://stackoverflow.com/questions/34993744/why-does-this-read-input-before-printing
        io::stdout().flush().expect("flush failed!");

        let mut cmd_str = String::new();

        // FIXME: Add support for multiline commands
        io::stdin().read_line(&mut cmd_str)?;

        cmd_str = cmd_str.trim().to_string();

        if cmd_str == "exit" {
            break;
        }

        let args_with_cmd: Vec<&str> = cmd_str.split_ascii_whitespace().collect();
        let command = Command {
            path: CString::new(args_with_cmd[0]).expect("Could not construct CString path"),
            args: args_with_cmd[1..]
                .iter()
                .map(|x| {
                    CString::new(x.bytes().collect::<Vec<u8>>())
                        .expect(&format!("Could not construct CString arg: {:?}", x))
                })
                .collect(),
        };

        // if is_builtin_command(&command.path) {
        //     // handle_builtin_command(command);
        // }
        execve::<CString, CString>(&command.path, &[], &[])?;

        // match unsafe { fork() } {
        //     Ok(ForkResult::Parent { child, .. }) => {
        //         waitpid(child, None)
        //             .expect(&format!("Expected to wait for child with pid: {:?}", child));
        //     }
        //     Ok(ForkResult::Child) => {
        //         // FIXME: Optimize this .len() out,
        //         // we just wanna know if there are more
        //         // than 1 elements
        //         let args: &[CString] = if command.args.len() < 1 {
        //             &[]
        //         } else {
        //             &command.args[1..]
        //         };

        //         println!("Path: {:?}", command.path);
        //         println!("Args: {:?}", args);

        //         execve::<CString, CString>(&command.path, args, &[])
        //             .expect("Call to execve failed");
        //         unsafe { libc::_exit(0) };
        //     }
        //     Err(err) => panic!("Fork failed: {err:?}"),
        // }
    }

    Ok(())
}

// fn is_builtin_command(cmd_name: &str) -> bool {
//     BUILTIN_COMMANDS.contains(&cmd_name)
// }

// fn handle_builtin_command(cmd: Command) {
//     if cmd.name == "echo" {
//         for arg in cmd.args {
//             print!("{:?}", arg);
//         }
//     }
// }

// fn trim_double_quotes() {
// }
