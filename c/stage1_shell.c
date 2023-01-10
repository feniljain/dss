#include <readline/readline.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

char **get_input(char *);
void cwd();

int main() {
  printf("Welcome to shell\n");

  char **command;
  char *input;
  pid_t child_pid;
  int stat_loc;

  while (1) {
    input = readline("$ ");
    command = get_input(input);

    if (!command[0]) {
      /* Handle empty commands */
      free(input);
      free(command);
      continue;
    }

    child_pid = fork();
    if (child_pid < 0) {
      perror("Fork failed");
      // This exits entire program
      exit(1);
    }

    if (child_pid == 0) {
      if (strcmp(command[0], "cd") == 0) {
        /* cwd(); */
        int result = chdir(command[1]);
        if (result != 0) {
          perror("chdir() result");
        }
        cwd();
      } else {
        /* Never returns if the call is successful */
        if (execvp(command[0], command) < 0) {
          perror(command[0]);
          // This exits child program
          // Explanation:
          // https://indradhanush.github.io/blog/writing-a-unix-shell-part-2/
          exit(1);
        }
        printf("This won't be printed if execvp is successul\n");
      }
      /* printf("Result from chdir: %d\n", result); */
    } else {
      waitpid(child_pid, &stat_loc, WUNTRACED);
    }

    free(input);
    free(command);
  }

  return 0;
}

void cwd() {
  char buf[256];
  if (getcwd(buf, sizeof(buf)) == NULL) {
    perror("getcwd() error");
  } else {
    printf("Current Working Directory: %s\n", buf);
  }
}

char **get_input(char *input) {
  char **command = malloc(8 * sizeof(char *));
  char *separator = " ";
  char *parsed;
  int index = 0;

  parsed = strtok(input, separator);
  while (parsed != NULL) {
    command[index] = parsed;
    index++;

    parsed = strtok(NULL, separator);
  }

  command[index] = NULL;
  return command;
}
