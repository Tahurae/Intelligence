#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

int main(int argc, char *argv[]) {
    if (argc < 2) {
        printf("Usage: note \"your note text\" [-t tag]\n");
        return 1;
    }

    char *content = argv[1];
    char *tag = "#bin";
    if (argc >= 4 && strcmp(argv[2], "-t") == 0) {
        tag = argv[3];
    }

    time_t now = time(NULL);
    struct tm *t = localtime(&now);
    char time_str[64];
    strftime(time_str, sizeof(time_str), "%Y-%m-%d %H:%M", t);

    char repo_path[512];
    const char *home = getenv("HOME");
    if (!home) home = "/data/data/com.termux/files/home";
    snprintf(repo_path, sizeof(repo_path), "%s/.ilang_repository.txt", home);

    FILE *f = fopen(repo_path, "a");
    if (!f) {
        perror("Failed to open repository file");
        return 1;
    }

    fprintf(f, "[%s] TAG: %s | NOTE: %s\n", time_str, tag, content);
    fclose(f);

    printf("Saved note to repository (%s)\n", tag);
    return 0;
}
