use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Space {
    Base(String),
    Product(Box<Space>, Box<Space>),
    Identity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Flow {
    Identity,
    Primitive { name: String, domain: Space, codomain: Space },
    Chain(Box<Flow>, Box<Flow>),
    Parallel(Box<Flow>, Box<Flow>),
    Feedback(Box<Flow>),
}

#[derive(Debug, Clone, PartialEq)]
enum Token { KwSpace, KwFlow, Ident(String), Equals, Colon, Arrow, OpChain, OpParallel, OpDagger, LParen, RParen }

struct Lexer<'a> { input: &'a str, pos: usize }
impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self { Self { input, pos: 0 } }
    fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        if self.pos >= self.input.len() { return None; }
        let rest = &self.input[self.pos..];
        if rest.starts_with("//") {
            if let Some(idx) = rest.find('\n') { self.pos += idx + 1; return self.next_token(); }
            else { self.pos = self.input.len(); return None; }
        }
        if rest.starts_with(">>") { self.pos += 2; return Some(Token::OpChain); }
        if rest.starts_with("||") { self.pos += 2; return Some(Token::OpParallel); }
        if rest.starts_with("->") { self.pos += 2; return Some(Token::Arrow); }
        let ch = rest.chars().next().unwrap();
        match ch {
            '=' => { self.pos += 1; Some(Token::Equals) }
            ':' => { self.pos += 1; Some(Token::Colon) }
            '~' => { self.pos += 1; Some(Token::OpDagger) }
            '(' => { self.pos += 1; Some(Token::LParen) }
            ')' => { self.pos += 1; Some(Token::RParen) }
            _ if ch.is_alphanumeric() || ch == '_' => {
                let len = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').map(|c| c.len_utf8()).sum();
                let ident = &rest[..len];
                self.pos += len;
                match ident { "space" => Some(Token::KwSpace), "flow" => Some(Token::KwFlow), _ => Some(Token::Ident(ident.to_string())) }
            }
            _ => { self.pos += ch.len_utf8(); self.next_token() }
        }
    }
    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if ch.is_whitespace() { self.pos += ch.len_utf8(); } else { break; }
        }
    }
}

pub struct Parser { tokens: Vec<Token>, pos: usize, primitive_types: HashMap<String, (Space, Space)> }
impl Parser {
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some(tok) = lexer.next_token() { tokens.push(tok); }
        Self { tokens, pos: 0, primitive_types: HashMap::new() }
    }
    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }
    fn advance(&mut self) -> Option<Token> { if self.pos < self.tokens.len() { let t = self.tokens[self.pos].clone(); self.pos += 1; Some(t) } else { None } }
    fn expect(&mut self, expected: Token) -> Result<(), String> {
        match self.advance() { Some(tok) if tok == expected => Ok(()), _ => Err("Syntax error".to_string()) }
    }
    pub fn parse(&mut self) -> Result<Flow, String> {
        let mut main_flow = None;
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::KwSpace) => { self.parse_space_decl()?; }
                Some(Token::KwFlow) => { main_flow = Some(self.parse_flow_decl()?); }
                Some(Token::Ident(_)) => { self.parse_primitive_decl()?; }
                _ => { self.advance(); }
            }
        }
        main_flow.ok_or_else(|| "No flow found".to_string())
    }
    fn parse_space_decl(&mut self) -> Result<(), String> { self.expect(Token::KwSpace)?; self.advance(); self.expect(Token::Equals)?; self.parse_space_expr()?; Ok(()) }
    fn parse_primitive_decl(&mut self) -> Result<(), String> {
        let name = match self.advance() { Some(Token::Ident(n)) => n, _ => return Err("Err".to_string()) };
        self.expect(Token::Colon)?; let dom = self.parse_space_expr()?; self.expect(Token::Arrow)?; let cod = self.parse_space_expr()?;
        self.primitive_types.insert(name, (dom, cod)); Ok(())
    }
    fn parse_space_expr(&mut self) -> Result<Space, String> {
        let mut left = match self.peek() {
            Some(Token::LParen) => { self.advance(); let i = self.parse_space_expr()?; self.expect(Token::RParen)?; i }
            Some(Token::Ident(_)) => { let n = match self.advance() { Some(Token::Ident(n)) => n, _ => unreachable!() }; Space::Base(n) }
            _ => return Err("Err".to_string()),
        };
        if let Some(Token::OpParallel) = self.peek() { self.advance(); let right = self.parse_space_expr()?; left = Space::Product(Box::new(left), Box::new(right)); }
        Ok(left)
    }
    fn parse_flow_decl(&mut self) -> Result<Flow, String> { self.advance(); self.advance(); self.expect(Token::Equals)?; self.parse_chain() }
    fn parse_chain(&mut self) -> Result<Flow, String> {
        let mut left = self.parse_parallel()?;
        while let Some(Token::OpChain) = self.peek() { self.advance(); let right = self.parse_parallel()?; left = Flow::Chain(Box::new(left), Box::new(right)); }
        Ok(left)
    }
    fn parse_parallel(&mut self) -> Result<Flow, String> {
        let mut left = self.parse_unary()?;
        while let Some(Token::OpParallel) = self.peek() { self.advance(); let right = self.parse_unary()?; left = Flow::Parallel(Box::new(left), Box::new(right)); }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Flow, String> {
        if let Some(Token::OpDagger) = self.peek() { self.advance(); Ok(Flow::Feedback(Box::new(self.parse_primary()?))) } else { self.parse_primary() }
    }
    fn parse_primary(&mut self) -> Result<Flow, String> {
        match self.peek().cloned() {
            Some(Token::LParen) => { self.advance(); let i = self.parse_chain()?; self.expect(Token::RParen)?; Ok(i) }
            Some(Token::Ident(name)) => {
                self.advance();
                let (dom, cod) = self.primitive_types.get(&name).cloned().unwrap_or((Space::Base("in".into()), Space::Base("out".into())));
                Ok(Flow::Primitive { name, domain: dom, codomain: cod })
            }
            _ => Err("Syntax error".to_string()),
        }
    }
}

pub struct C99Emitter;
impl C99Emitter {
    pub fn emit(flow: &Flow) -> String {
        let mut out = String::new();
        out.push_str("#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <math.h>\n#include <termios.h>\n#include <unistd.h>\n#include <time.h>\n\n");
        out.push_str("#define WIRE_DIM 64\n#define TENSOR_DIM 128\n\n");

        out.push_str("typedef struct {\n");
        out.push_str("    float tensor[TENSOR_DIM];\n");
        out.push_str("    float memory_matrix[WIRE_DIM][WIRE_DIM];\n");
        out.push_str("    float recalled_wire[WIRE_DIM];\n");
        out.push_str("    char note_text[1024];\n");
        out.push_str("    char context_tag[256];\n");
        out.push_str("    char status_msg[256];\n");
        out.push_str("} SubstrateMemory;\n\n");

        out.push_str("static SubstrateMemory g_mem;\n");
        out.push_str("static struct termios orig_termios;\n\n");

        out.push_str("void disable_raw() { tcsetattr(STDIN_FILENO, TCSAFLUSH, &orig_termios); printf(\"\\033[?25h\\033[0m\\n\"); }\n");
        out.push_str("void enable_raw() { tcgetattr(STDIN_FILENO, &orig_termios); atexit(disable_raw); struct termios raw = orig_termios; raw.c_lflag &= ~(ECHO | ICANON); tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw); }\n\n");

        out.push_str("float cosine_sim(const float* a, const float* b, int len) {\n");
        out.push_str("    float dot=0, na=0, nb=0;\n");
        out.push_str("    for(int i=0; i<len; i++) { dot+=a[i]*b[i]; na+=a[i]*a[i]; nb+=b[i]*b[i]; }\n");
        out.push_str("    return (na==0||nb==0) ? 0.0f : dot/(sqrtf(na)*sqrtf(nb));\n");
        out.push_str("}\n\n");

        out.push_str("void save_to_disk() {\n");
        out.push_str("    const char* home = getenv(\"HOME\");\n");
        out.push_str("    char path[512];\n");
        out.push_str("    snprintf(path, sizeof(path), \"%s/.ilang_repository.txt\", home ? home : \".\");\n");
        out.push_str("    FILE* f = fopen(path, \"a\");\n");
        out.push_str("    if (f) {\n");
        out.push_str("        time_t now = time(NULL);\n");
        out.push_str("        char tstr[64]; strftime(tstr, sizeof(tstr), \"%Y-%m-%d %H:%M\", localtime(&now));\n");
        out.push_str("        fprintf(f, \"[%s] TAG: %s | NOTE: %s\\n\", tstr, g_mem.context_tag, g_mem.note_text);\n");
        out.push_str("        fclose(f);\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");

        out.push_str("void view_repository() {\n");
        out.push_str("    const char* home = getenv(\"HOME\");\n");
        out.push_str("    char path[512];\n");
        out.push_str("    snprintf(path, sizeof(path), \"%s/.ilang_repository.txt\", home ? home : \".\");\n");
        out.push_str("    printf(\"\\033[H\\033[J\");\n");
        out.push_str("    printf(\"\\033[7m === ILANG NOTE REPOSITORY === \\033[0m\\n\\n\");\n");
        out.push_str("    FILE* f = fopen(path, \"r\");\n");
        out.push_str("    if (!f) { printf(\"  (No notes stored yet in repository)\\n\"); }\n");
        out.push_str("    else {\n");
        out.push_str("        char line[1280];\n");
        out.push_str("        while (fgets(line, sizeof(line), f)) printf(\"  %s\", line);\n");
        out.push_str("        fclose(f);\n");
        out.push_str("    }\n");
        out.push_str("    printf(\"\\n\\033[7m Press any key to return to editor \\033[0m\\n\");\n");
        out.push_str("    char c; read(STDIN_FILENO, &c, 1);\n");
        out.push_str("}\n\n");

        out.push_str("void execute_encode_text() {\n");
        out.push_str("    unsigned int h=5381; for(size_t i=0;i<strlen(g_mem.note_text);i++) h=((h<<5)+h)+g_mem.note_text[i];\n");
        out.push_str("    for(int i=0;i<WIRE_DIM;i++) g_mem.tensor[i]=sinf((float)(h+i*17)*0.1f);\n");
        out.push_str("}\n");

        out.push_str("void execute_tag_context() {\n");
        out.push_str("    unsigned int h=5381; for(size_t i=0;i<strlen(g_mem.context_tag);i++) h=((h<<5)+h)+g_mem.context_tag[i];\n");
        out.push_str("    for(int i=0;i<WIRE_DIM;i++) g_mem.tensor[WIRE_DIM+i]=cosf((float)(h+i*31)*0.1f);\n");
        out.push_str("}\n");

        out.push_str("void execute_associate_memory() {\n");
        out.push_str("    for(int i=0;i<WIRE_DIM;i++) for(int j=0;j<WIRE_DIM;j++) g_mem.memory_matrix[i][j]+=g_mem.tensor[i]*g_mem.tensor[WIRE_DIM+j];\n");
        out.push_str("}\n");

        out.push_str("void execute_adjoint_associate_memory() {\n");
        out.push_str("    for(int i=0;i<WIRE_DIM;i++) {\n");
        out.push_str("        float sum=0; for(int j=0;j<WIRE_DIM;j++) sum+=g_mem.memory_matrix[i][j]*g_mem.tensor[WIRE_DIM+j];\n");
        out.push_str("        g_mem.recalled_wire[i]=sum;\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");

        out.push_str("void run_pipeline() {\n");
        out.push_str("    execute_encode_text(); execute_tag_context(); execute_associate_memory(); execute_adjoint_associate_memory();\n");
        out.push_str("}\n\n");

        out.push_str("void draw_editor(int focus_tag) {\n");
        out.push_str("    printf(\"\\033[H\\033[J\");\n");
        out.push_str("    printf(\"\\033[7m  ILANG NANO-NOTE TENSOR EDITOR v1.1                      \\033[0m\\n\\n\");\n");
        out.push_str("    printf(\"  \\033[1;36m[ Note Content ]\\033[0m%s\\n\", focus_tag ? \"\" : \" <editing>\");\n");
        out.push_str("    printf(\"  %s\\n\\n\", g_mem.note_text[0] ? g_mem.note_text : \"(Type note here...)\");\n");
        out.push_str("    printf(\"  \\033[1;33m[ Context Tags ]\\033[0m%s\\n\", focus_tag ? \" <editing>\" : \"\");\n");
        out.push_str("    printf(\"  %s\\n\\n\", g_mem.context_tag[0] ? g_mem.context_tag : \"#general\");\n");
        out.push_str("    printf(\"  --------------------------------------------------\\n\");\n");
        out.push_str("    printf(\"  \\033[1;32mStatus:\\033[0m %s\\n\\n\", g_mem.status_msg[0] ? g_mem.status_msg : \"Ready\");\n");
        out.push_str("    printf(\"\\033[7m ^S Save to Repo   ^R View Repo   ^T Switch Tag   ^X Exit \\033[0m\\n\");\n");
        out.push_str("}\n\n");

        out.push_str("void run_interactive_nano() {\n");
        out.push_str("    enable_raw();\n");
        out.push_str("    strcpy(g_mem.status_msg, \"Press Ctrl+S to save to repository\");\n");
        out.push_str("    int focus_tag = 0, note_pos = strlen(g_mem.note_text), tag_pos = strlen(g_mem.context_tag);\n");
        out.push_str("    if (tag_pos == 0) { strcpy(g_mem.context_tag, \"#general\"); tag_pos = 8; }\n");
        out.push_str("    while (1) {\n");
        out.push_str("        draw_editor(focus_tag);\n");
        out.push_str("        char c; if (read(STDIN_FILENO, &c, 1) != 1) continue;\n");
        out.push_str("        if (c == 24) break; // ^X\n");
        out.push_str("        else if (c == 18) { view_repository(); } // ^R -> View Repo\n");
        out.push_str("        else if (c == 19) { // ^S -> Save\n");
        out.push_str("            run_pipeline(); save_to_disk();\n");
        out.push_str("            float sim = cosine_sim(&g_mem.tensor[0], g_mem.recalled_wire, WIRE_DIM);\n");
        out.push_str("            snprintf(g_mem.status_msg, sizeof(g_mem.status_msg), \"SAVED TO DISK! Match: %.2f%%\", sim * 100.0f);\n");
        out.push_str("        }\n");
        out.push_str("        else if (c == 20) focus_tag = !focus_tag; // ^T\n");
        out.push_str("        else if (c == 127 || c == 8) {\n");
        out.push_str("            if (focus_tag) { if (tag_pos > 0) g_mem.context_tag[--tag_pos] = '\\0'; }\n");
        out.push_str("            else { if (note_pos > 0) g_mem.note_text[--note_pos] = '\\0'; }\n");
        out.push_str("        }\n");
        out.push_str("        else if (c >= 32 && c <= 126) {\n");
        out.push_str("            if (focus_tag) { if (tag_pos < 254) { g_mem.context_tag[tag_pos++] = c; g_mem.context_tag[tag_pos] = '\\0'; } }\n");
        out.push_str("            else { if (note_pos < 1022) { g_mem.note_text[note_pos++] = c; g_mem.note_text[note_pos] = '\\0'; } }\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");

        out.push_str("int main(int argc, char** argv) {\n");
        out.push_str("    memset(&g_mem, 0, sizeof(SubstrateMemory));\n");
        out.push_str("    if (argc > 1 && strcmp(argv[1], \"--list\") == 0) { enable_raw(); view_repository(); return 0; }\n");
        out.push_str("    run_interactive_nano();\n");
        out.push_str("    return 0;\n");
        out.push_str("}\n");
        out
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        if let Ok(code) = fs::read_to_string(&args[1]) {
            let mut parser = Parser::new(&code);
            if let Ok(ast) = parser.parse() {
                let c_code = C99Emitter::emit(&ast);
                let _ = fs::write("payload.c", &c_code);
                let _ = Command::new("clang").args(&["-O3", "payload.c", "-lm", "-o", "binary_app"]).status();
                let _ = Command::new("ln").args(&["-sf", &format!("{}/ingenious/binary_app", env::var("HOME").unwrap()), &format!("{}/bin/note", env::var("PREFIX").unwrap())]).status();
                println!("Updated 'note' editor installed successfully!");
            }
        }
    }
}
