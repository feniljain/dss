#include <stdio.h>
#include <fcntl.h>

int main() {
    int fd = open(_PATH_TTY, O_RDWR);
    printf("fd: %d", fd);
    return 0;
}
