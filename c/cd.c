#include <readline/readline.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

void cwd();

int main(int argc, char *argv[]) {
  if (argc != 2) {
    printf("invalid args");
    return 1;
  }

  int result = chdir(argv[1]);
  if (result != 0) {
    perror("chdir() result");
  }

  cwd();

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
