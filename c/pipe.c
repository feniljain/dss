#include <unistd.h>
#include <stdio.h>

struct command {
  const char **argv;
};

// we have to write to `out`. `out` -> stdout
// we have to read from `in`. `in` -> stdin
int spawn_proc(int in, int out, struct command *cmd) {
  pid_t pid;

  printf("set_stdin_to: %d, set_stdout_to: %d\n", in, out);
  if ((pid = fork()) == 0) {
    if (in != 0) {
      dup2(in, 0);
      close(in);
    }

    if (out != 1) {
      dup2(out, 1);
      close(out);
    }

    return execvp(cmd->argv[0], (char *const *)cmd->argv);
  }

  return pid;
}

int fork_pipes(int n, struct command *cmd) {
  int i;
  pid_t pid;
  int in, fd[2];

  /* The first process should get its input from the original file descriptor 0.*/
  in = 0;

  /* Note the loop bound, we spawn here all, but the last stage of the pipeline.*/
  for (i = 0; i < n - 1; ++i) {
    pipe(fd);
    printf("pipe results: (%d %d)\n", fd[0], fd[1]);

    /* f[1] is the write end of the pipe, we carry `in` from the prev
     * iteration. */
    spawn_proc(in, fd[1], cmd + i);

    /* No need for the write end of the pipe, the child will write here.  */
    close(fd[1]);

    /* Keep the read end of the pipe, the next child will read from there.  */
    in = fd[0];
  }

  /* Last stage of the pipeline - set stdin be the read end of the previous pipe
     and output to the original file descriptor 1. */
  if (in != 0)
    dup2(in, 0);

  /* Execute the last stage with the current process. */
  return execvp(cmd[i].argv[0], (char *const *)cmd[i].argv);
}

int main() {
  const char *ls[] = {"ls", "-la", 0};
  const char *awk[] = {"awk", "{print $9}", 0};
  const char *sort[] = {"sort", 0};
  const char *uniq[] = {"uniq", 0};

  struct command cmd[] = {{ls}, {awk}, {sort}, {uniq}};
  /* struct command cmd[] = {{ls}, {awk}}; */

  return fork_pipes(4, cmd);
}
