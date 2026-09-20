#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char *argv[]) {
    char repo_path[512];
    const char *home = getenv("HOME");
    if (!home) home = "/data/data/com.termux/files/home";
    snprintf(repo_path, sizeof(repo_path), "%s/.ilang_repository.txt", home);

    FILE *f = fopen(repo_path, "r");
    if (!f) {
        printf("No repository notes found.\n");
        return 0;
    }

    char *filter = (argc >= 2) ? argv[1] : NULL;
    char line[1024];
    int found = 0;

    while (fgets(line, sizeof(line), f)) {
        if (!filter || strstr(line, filter) != NULL) {
            printf("%s", line);
            found = 1;
        }
    }
    fclose(f);
    return 0;
}
