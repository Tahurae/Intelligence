#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <termios.h>
#include <unistd.h>
#include <time.h>

#define WIRE_DIM 64
#define TENSOR_DIM 128

typedef struct {
    float tensor[TENSOR_DIM];
    float memory_matrix[WIRE_DIM][WIRE_DIM];
    float recalled_wire[WIRE_DIM];
    char note_text[1024];
    char context_tag[256];
    char status_msg[256];
} SubstrateMemory;

static SubstrateMemory g_mem;
static struct termios orig_termios;

void disable_raw() { tcsetattr(STDIN_FILENO, TCSAFLUSH, &orig_termios); printf("\033[?25h\033[0m\n"); }
void enable_raw() { tcgetattr(STDIN_FILENO, &orig_termios); atexit(disable_raw); struct termios raw = orig_termios; raw.c_lflag &= ~(ECHO | ICANON); tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw); }

float cosine_sim(const float* a, const float* b, int len) {
    float dot=0, na=0, nb=0;
    for(int i=0; i<len; i++) { dot+=a[i]*b[i]; na+=a[i]*a[i]; nb+=b[i]*b[i]; }
    return (na==0||nb==0) ? 0.0f : dot/(sqrtf(na)*sqrtf(nb));
}

void save_to_disk() {
    const char* home = getenv("HOME");
    char path[512];
    snprintf(path, sizeof(path), "%s/.ilang_repository.txt", home ? home : ".");
    FILE* f = fopen(path, "a");
    if (f) {
        time_t now = time(NULL);
        char tstr[64]; strftime(tstr, sizeof(tstr), "%Y-%m-%d %H:%M", localtime(&now));
        fprintf(f, "[%s] TAG: %s | NOTE: %s\n", tstr, g_mem.context_tag, g_mem.note_text);
        fclose(f);
    }
}

void view_repository() {
    const char* home = getenv("HOME");
    char path[512];
    snprintf(path, sizeof(path), "%s/.ilang_repository.txt", home ? home : ".");
    printf("\033[H\033[J");
    printf("\033[7m === ILANG NOTE REPOSITORY === \033[0m\n\n");
    FILE* f = fopen(path, "r");
    if (!f) { printf("  (No notes stored yet in repository)\n"); }
    else {
        char line[1280];
        while (fgets(line, sizeof(line), f)) printf("  %s", line);
        fclose(f);
    }
    printf("\n\033[7m Press any key to return to editor \033[0m\n");
    char c; read(STDIN_FILENO, &c, 1);
}

void execute_encode_text() {
    unsigned int h=5381; for(size_t i=0;i<strlen(g_mem.note_text);i++) h=((h<<5)+h)+g_mem.note_text[i];
    for(int i=0;i<WIRE_DIM;i++) g_mem.tensor[i]=sinf((float)(h+i*17)*0.1f);
}
void execute_tag_context() {
    unsigned int h=5381; for(size_t i=0;i<strlen(g_mem.context_tag);i++) h=((h<<5)+h)+g_mem.context_tag[i];
    for(int i=0;i<WIRE_DIM;i++) g_mem.tensor[WIRE_DIM+i]=cosf((float)(h+i*31)*0.1f);
}
void execute_associate_memory() {
    for(int i=0;i<WIRE_DIM;i++) for(int j=0;j<WIRE_DIM;j++) g_mem.memory_matrix[i][j]+=g_mem.tensor[i]*g_mem.tensor[WIRE_DIM+j];
}
void execute_adjoint_associate_memory() {
    for(int i=0;i<WIRE_DIM;i++) {
        float sum=0; for(int j=0;j<WIRE_DIM;j++) sum+=g_mem.memory_matrix[i][j]*g_mem.tensor[WIRE_DIM+j];
        g_mem.recalled_wire[i]=sum;
    }
}

void run_pipeline() {
    execute_encode_text(); execute_tag_context(); execute_associate_memory(); execute_adjoint_associate_memory();
}

void draw_editor(int focus_tag) {
    printf("\033[H\033[J");
    printf("\033[7m  ILANG NANO-NOTE TENSOR EDITOR v1.1                      \033[0m\n\n");
    printf("  \033[1;36m[ Note Content ]\033[0m%s\n", focus_tag ? "" : " <editing>");
    printf("  %s\n\n", g_mem.note_text[0] ? g_mem.note_text : "(Type note here...)");
    printf("  \033[1;33m[ Context Tags ]\033[0m%s\n", focus_tag ? " <editing>" : "");
    printf("  %s\n\n", g_mem.context_tag[0] ? g_mem.context_tag : "#general");
    printf("  --------------------------------------------------\n");
    printf("  \033[1;32mStatus:\033[0m %s\n\n", g_mem.status_msg[0] ? g_mem.status_msg : "Ready");
    printf("\033[7m ^S Save to Repo   ^R View Repo   ^T Switch Tag   ^X Exit \033[0m\n");
}

void run_interactive_nano() {
    enable_raw();
    strcpy(g_mem.status_msg, "Press Ctrl+S to save to repository");
    int focus_tag = 0, note_pos = strlen(g_mem.note_text), tag_pos = strlen(g_mem.context_tag);
    if (tag_pos == 0) { strcpy(g_mem.context_tag, "#general"); tag_pos = 8; }
    while (1) {
        draw_editor(focus_tag);
        char c; if (read(STDIN_FILENO, &c, 1) != 1) continue;
        if (c == 24) break; // ^X
        else if (c == 18) { view_repository(); } // ^R -> View Repo
        else if (c == 19) { // ^S -> Save
            run_pipeline(); save_to_disk();
            float sim = cosine_sim(&g_mem.tensor[0], g_mem.recalled_wire, WIRE_DIM);
            snprintf(g_mem.status_msg, sizeof(g_mem.status_msg), "SAVED TO DISK! Match: %.2f%%", sim * 100.0f);
        }
        else if (c == 20) focus_tag = !focus_tag; // ^T
        else if (c == 127 || c == 8) {
            if (focus_tag) { if (tag_pos > 0) g_mem.context_tag[--tag_pos] = '\0'; }
            else { if (note_pos > 0) g_mem.note_text[--note_pos] = '\0'; }
        }
        else if (c >= 32 && c <= 126) {
            if (focus_tag) { if (tag_pos < 254) { g_mem.context_tag[tag_pos++] = c; g_mem.context_tag[tag_pos] = '\0'; } }
            else { if (note_pos < 1022) { g_mem.note_text[note_pos++] = c; g_mem.note_text[note_pos] = '\0'; } }
        }
    }
}

int main(int argc, char** argv) {
    memset(&g_mem, 0, sizeof(SubstrateMemory));
    if (argc > 1 && strcmp(argv[1], "--list") == 0) { enable_raw(); view_repository(); return 0; }
    run_interactive_nano();
    return 0;
}
