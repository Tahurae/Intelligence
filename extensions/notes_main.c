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

    char line[1024];
    int found = 0;

    // If no argument is provided, print everything
    if (argc < 2 || strlen(argv[1]) == 0) {
        while (fgets(line, sizeof(line), f)) {
            printf("%s", line);
        }
        fclose(f);
        return 0;
    }

    // Otherwise, strictly filter by the provided query/tag
    char *filter = argv[1];
    while (fgets(line, sizeof(line), f)) {
        if (strstr(line, filter) != NULL) {
            printf("%s", line);
            found = 1;
        }
    }
    fclose(f);

    if (!found) {
        printf("No notes matching '%s' found.\n", filter);
    }
    return 0;
}
