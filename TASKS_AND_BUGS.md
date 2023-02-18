# Tasks:

- [X] correct command split by space
- [X] handle empty commands
- [X] add / ./ handling
- [X] correct path parsing and argument parsing according to the `man execve`
- [ ] add Ctrl-C + Ctrl-D handling
- [X] parsing all paths
- [X] trying all paths robustly
- [X] proper handling for command not found
- [X] include handling of `!` while parsing and also while checking exit status
- [X] use exit status of wait: The Unix convention is that a zero exit status represents success, and any non-zero exit status represents failure.
- [X] implement your own `cd` in C
- [X] implement `cd` builtin in your own shell
- [X] print error messages according to errno
- [X] for invalid path command ( e.g. ./a.sh ) give no such file or directory error
- [X] add support for `;`, `||` and `&&` in commands
- [X] write a proper lexer and parser for commands
- [X] add lexer tests
- [X] add parser tests
- [X] add tests for execution status
- [X] after stage 1 refactor code to have a separate engine and cmd parsing
- [X] add support for subshell commands
- [X] add support for multiline commands
- [X] pass stage 1 tests
- [X] add support for pipe operators
- [X] add support for redirect output operator
- [X] add support for redirect input operator
- [ ] add support for redirect >> operator
- [ ] add support for redirect <> operator
- [ ] add support for redirect >& operator
- [ ] add support for redirect <& operator
- [ ] add support for fd in redirect operations

## Bonus Tasks:

- [X] add color depending on exit status
- [ ] add last segment of current folder like my own zsh with some color
- [ ] Implement readline like https://github.com/kkawakam/rustyline
- [ ] Handle Ctrl+P input, maintain last command executed, and fill input with it

# Bugs

- [X] builtin command execution successful handling
- [ ] builtin command execution error case handling and printing
- [ ] correct signal handling by referencing https://github.com/kkawakam/rustyline
- [ ] correct handling of "" and {} in commands like `grep "c"` or `awk {print $9}`
- [X] correct handling of commands like "ls 2> err", for some reason wrong cmd exec in our shell is written on stdout itself
- [ ] correct subshell command mode entirely, understand it properly and implement again
