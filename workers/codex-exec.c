#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

extern char **environ;
static volatile sig_atomic_t codex_pid;

static void forward_signal(int signal) {
    int saved_errno = errno;
    if (codex_pid > 0) {
        kill((pid_t)codex_pid, signal);
    }
    errno = saved_errno;
}

static void record_spawn(struct timespec start, struct timespec end) {
    const char *path = getenv("PROXY_CODEX_SPAWN_TIMING");
    if (path == NULL || path[0] != '/') {
        return;
    }
    long long microseconds = (long long)(end.tv_sec - start.tv_sec) * 1000000 +
        (end.tv_nsec - start.tv_nsec) / 1000;
    int file = open(path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_NONBLOCK, 0600);
    if (file >= 0) {
        dprintf(file, "%lld\n", microseconds);
        close(file);
    }
}

int main(int argc, char **argv) {
    const char *binary = getenv("PROXY_CODEX_BINARY");
    if (argc < 2 || strcmp(argv[1], "exec") != 0 || binary == NULL || binary[0] != '/') {
        return 64;
    }
    char **args = calloc((size_t)argc + 3, sizeof(*args));
    if (args == NULL) {
        return 71;
    }
    args[0] = (char *)binary;
    args[1] = "exec";
    args[2] = "--ignore-user-config";
    args[3] = "--ignore-rules";
    for (int i = 2; i < argc; ++i) {
        args[i + 2] = argv[i];
    }

    sigset_t signals, previous;
    sigemptyset(&signals);
    sigaddset(&signals, SIGINT);
    sigaddset(&signals, SIGTERM);
    struct sigaction handler = {.sa_handler = forward_signal};
    sigemptyset(&handler.sa_mask);
    if (sigprocmask(SIG_BLOCK, &signals, &previous) != 0 ||
        sigaction(SIGINT, &handler, NULL) != 0 || sigaction(SIGTERM, &handler, NULL) != 0) {
        return 71;
    }
    posix_spawnattr_t attributes;
    if (posix_spawnattr_init(&attributes) != 0 ||
        posix_spawnattr_setsigmask(&attributes, &previous) != 0 ||
        posix_spawnattr_setflags(&attributes, POSIX_SPAWN_SETSIGMASK) != 0) {
        return 71;
    }
    pid_t child;
    struct timespec start, end;
    int timed = clock_gettime(CLOCK_MONOTONIC, &start) == 0;
    int result = posix_spawn(&child, binary, NULL, &attributes, args, environ);
    if (result != 0) {
        return 71;
    }
    codex_pid = child;
    sigprocmask(SIG_SETMASK, &previous, NULL);
    if (timed && clock_gettime(CLOCK_MONOTONIC, &end) == 0) {
        record_spawn(start, end);
    }
    posix_spawnattr_destroy(&attributes);
    free(args);
    int status;
    while (waitpid(child, &status, 0) < 0) {
        if (errno != EINTR) {
            return 71;
        }
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    return WIFSIGNALED(status) ? 128 + WTERMSIG(status) : 71;
}
