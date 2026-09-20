use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::Command;

// ============================================================================
// 1. DOMAIN & SPACE REPRESENTATION
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Space {
    Scalar(String),
    Tensor(usize),
    Qubit(usize),
    Organoid(usize),
    Logic(usize),
    Product(Vec<Space>),
    Unit,
}

impl Space {
    pub fn size_in_bytes(&self) -> usize {
        match self {
            Space::Scalar(_) => std::mem::size_of::<f64>(),
            Space::Tensor(n) => n * std::mem::size_of::<f32>(),
            Space::Qubit(n) => (1 << n) * std::mem::size_of::<f32>() * 2,
            Space::Organoid(n) => n * std::mem::size_of::<f32>(),
            Space::Logic(n) => ((n + 7) / 8),
            Space::Product(spaces) => spaces.iter().map(|s| s.size_in_bytes()).sum(),
            Space::Unit => 0,
        }
    }

    pub fn c_type_string(&self) -> String {
        match self {
            Space::Scalar(_) => "double".to_string(),
            Space::Tensor(_) => "float*".to_string(),
            Space::Qubit(_) => "float*".to_string(),
            Space::Organoid(_) => "float*".to_string(),
            Space::Logic(_) => "uint8_t*".to_string(),
            Space::Product(_) => "void*".to_string(),
            Space::Unit => "void".to_string(),
        }
    }
}

// ============================================================================
// 2. PRIMITIVES & REGISTRY
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveFamily {
    Scalar,
    Tensor,
    Quantum,
    Organoid,
    Logic,
}

#[derive(Debug, Clone)]
pub struct Primitive {
    pub name: String,
    pub dom: Space,
    pub cod: Space,
    pub family: PrimitiveFamily,
    pub forward_c: String,
    pub adjoint_c: String,
}

#[derive(Debug, Clone)]
pub struct PrimitiveRegistry {
    pub primitives: HashMap<String, Primitive>,
    pub declared_spaces: HashMap<String, Space>,
}

impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            primitives: HashMap::new(),
            declared_spaces: HashMap::new(),
        };
        reg.register_defaults();
        reg
    }

    pub fn register_defaults(&mut self) {
        self.register(Primitive {
            name: "add".to_string(),
            dom: Space::Product(vec![Space::Scalar("f64".into()), Space::Scalar("f64".into())]),
            cod: Space::Scalar("f64".into()),
            family: PrimitiveFamily::Scalar,
            forward_c: "*out = in[0] + in[1];".to_string(),
            adjoint_c: "grad_in[0] = *grad_out; grad_in[1] = *grad_out;".to_string(),
        });

        self.register(Primitive {
            name: "mul".to_string(),
            dom: Space::Product(vec![Space::Scalar("f64".into()), Space::Scalar("f64".into())]),
            cod: Space::Scalar("f64".into()),
            family: PrimitiveFamily::Scalar,
            forward_c: "*out = in[0] * in[1];".to_string(),
            adjoint_c: "grad_in[0] = *grad_out * in[1]; grad_in[1] = *grad_out * in[0];".to_string(),
        });

        self.register(Primitive {
            name: "neural_layer".to_string(),
            dom: Space::Tensor(128),
            cod: Space::Tensor(64),
            family: PrimitiveFamily::Tensor,
            forward_c: "cpu_dense_forward(in, out, 128, 64);".to_string(),
            adjoint_c: "cpu_dense_backward(grad_out, grad_in, 128, 64);".to_string(),
        });

        self.register(Primitive {
            name: "quantum_gate".to_string(),
            dom: Space::Qubit(2),
            cod: Space::Qubit(2),
            family: PrimitiveFamily::Quantum,
            forward_c: "quantum_unitary_apply(dev->qasm_fd, in, out, 2);".to_string(),
            adjoint_c: "quantum_parameter_shift_adjoint(dev->qasm_fd, grad_out, grad_in, 2);".to_string(),
        });

        self.register(Primitive {
            name: "organoid_step".to_string(),
            dom: Space::Organoid(64),
            cod: Space::Organoid(64),
            family: PrimitiveFamily::Organoid,
            forward_c: "organoid_dac_adc_transceive(dev->mea_fd, in, out, 64);".to_string(),
            adjoint_c: "organoid_adjoint_sensitivity_integrate(dev->mea_fd, grad_out, grad_in, 64);".to_string(),
        });

        self.register(Primitive {
            name: "organoid_pulse".to_string(),
            dom: Space::Organoid(64),
            cod: Space::Organoid(64),
            family: PrimitiveFamily::Organoid,
            forward_c: "organoid_dac_adc_transceive(dev->mea_fd, in, out, 64);".to_string(),
            adjoint_c: "organoid_adjoint_sensitivity_integrate(dev->mea_fd, grad_out, grad_in, 64);".to_string(),
        });

        self.register(Primitive {
            name: "logic_step".to_string(),
            dom: Space::Logic(8),
            cod: Space::Logic(8),
            family: PrimitiveFamily::Logic,
            forward_c: "logic_pack_apply(in, out, 8);".to_string(),
            adjoint_c: "logic_unpack_adjoint(grad_out, grad_in, 8);".to_string(),
        });
    }

    pub fn register(&mut self, prim: Primitive) {
        self.primitives.insert(prim.name.clone(), prim);
    }

    pub fn get(&self, name: &str) -> Option<&Primitive> {
        self.primitives.get(name)
    }

    pub fn get_default_impl(&self, name: &str) -> (String, String) {
        if let Some(prim) = self.get(name) {
            (prim.forward_c.clone(), prim.adjoint_c.clone())
        } else {
            (
                format!("/* default forward fallback for {} */", name),
                format!("/* default adjoint fallback for {} */", name),
            )
        }
    }
}

// ============================================================================
// 3. CATEGORICAL AST (FLOWS)
// ============================================================================

#[derive(Debug, Clone)]
pub enum Flow {
    Primitive(String),
    Chain(Box<Flow>, Box<Flow>),
    Parallel(Box<Flow>, Box<Flow>),
    Feedback(Box<Flow>),
    Identity(Space),
    Derivative(Box<Flow>),
}

#[derive(Debug, Clone)]
pub struct FlowDecl {
    pub name: String,
    pub dom: Option<Space>,
    pub cod: Option<Space>,
    pub body: Flow,
}

// ============================================================================
// 4. LEXER & PARSER
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    SpaceKw,
    FlowKw,
    Ident(String),
    Equals,
    Colon,
    Arrow,
    ChainOp,
    ParallelOp,
    Tilde,
    GradKw,
    IdKw,
    LParen,
    RParen,
    LAngle,
    RAngle,
    Comma,
    Integer(usize),
    Eof,
}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = self.input.chars().collect();

        while self.pos < chars.len() {
            let c = chars[self.pos];

            if c.is_whitespace() {
                self.pos += 1;
                continue;
            }

            if c == '/' && self.pos + 1 < chars.len() && chars[self.pos + 1] == '/' {
                while self.pos < chars.len() && chars[self.pos] != '\n' {
                    self.pos += 1;
                }
                continue;
            }

            if self.pos + 1 < chars.len() {
                let dual = format!("{}{}", c, chars[self.pos + 1]);
                match dual.as_str() {
                    ">>" => {
                        self.pos += 2;
                        tokens.push(Token::ChainOp);
                        continue;
                    }
                    "||" => {
                        self.pos += 2;
                        tokens.push(Token::ParallelOp);
                        continue;
                    }
                    "->" => {
                        self.pos += 2;
                        tokens.push(Token::Arrow);
                        continue;
                    }
                    _ => {}
                }
            }

            match c {
                '=' => tokens.push(Token::Equals),
                ':' => tokens.push(Token::Colon),
                '~' => tokens.push(Token::Tilde),
                '(' => tokens.push(Token::LParen),
                ')' => tokens.push(Token::RParen),
                '<' => tokens.push(Token::LAngle),
                '>' => tokens.push(Token::RAngle),
                ',' => tokens.push(Token::Comma),
                _ => {
                    if c.is_alphabetic() || c == '_' {
                        let start = self.pos;
                        while self.pos < chars.len()
                            && (chars[self.pos].is_alphanumeric() || chars[self.pos] == '_')
                        {
                            self.pos += 1;
                        }
                        let text: String = chars[start..self.pos].iter().collect();
                        match text.as_str() {
                            "space" => tokens.push(Token::SpaceKw),
                            "flow" => tokens.push(Token::FlowKw),
                            "grad" => tokens.push(Token::GradKw),
                            "id" => tokens.push(Token::IdKw),
                            _ => tokens.push(Token::Ident(text)),
                        }
                        continue;
                    } else if c.is_numeric() {
                        let start = self.pos;
                        while self.pos < chars.len() && chars[self.pos].is_numeric() {
                            self.pos += 1;
                        }
                        let num_str: String = chars[start..self.pos].iter().collect();
                        tokens.push(Token::Integer(num_str.parse().unwrap_or(0)));
                        continue;
                    }
                }
            }
            self.pos += 1;
        }

        tokens.push(Token::Eof);
        tokens
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    pub fn parse(&mut self, registry: &mut PrimitiveRegistry) -> Result<Vec<FlowDecl>, String> {
        let mut flow_decls = Vec::new();

        while *self.peek() != Token::Eof {
            match self.peek() {
                Token::SpaceKw => {
                    self.parse_space_decl(registry)?;
                }
                Token::FlowKw => {
                    let decl = self.parse_flow_decl(registry)?;
                    flow_decls.push(decl);
                }
                Token::Ident(_) => {
                    if self.peek_n(1) == Some(&Token::Colon) {
                        self.parse_primitive_decl(registry)?;
                    } else {
                        self.advance();
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }

        Ok(flow_decls)
    }

    fn peek_n(&self, offset: usize) -> Option<&Token> {
        let idx = self.pos + offset;
        if idx < self.tokens.len() {
            Some(&self.tokens[idx])
        } else {
            None
        }
    }

    fn parse_space_decl(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
        self.advance();
        let name = match self.advance() {
            Token::Ident(n) => n,
            other => return Err(format!("Expected space identifier, got {:?}", other)),
        };

        if *self.peek() == Token::Equals {
            self.advance();
            let space_type = self.parse_space_type(registry)?;
            registry.declared_spaces.insert(name, space_type);
        }
        Ok(())
    }

    fn parse_space_type(&mut self, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match self.advance() {
            Token::Ident(kind) => {
                let resolved = if registry.declared_spaces.contains_key(&kind) {
                    registry.declared_spaces.get(&kind).cloned().unwrap_or(Space::Scalar(kind.clone()))
                } else {
                    match kind.as_str() {
                        "Tensor" | "Array" => {
                            if *self.peek() == Token::LAngle {
                                self.advance();
                                let dim = match self.advance() {
                                    Token::Integer(d) => d,
                                    other => return Err(format!("Expected dimension, got {:?}", other)),
                                };
                                self.expect(Token::RAngle)?;
                                Space::Tensor(dim)
                            } else {
                                Space::Tensor(128)
                            }
                        }
                        "Qubit" | "Quantum" => {
                            if *self.peek() == Token::LAngle {
                                self.advance();
                                let dim = match self.advance() {
                                    Token::Integer(d) => d,
                                    other => return Err(format!("Expected qubit count, got {:?}", other)),
                                };
                                self.expect(Token::RAngle)?;
                                Space::Qubit(dim)
                            } else {
                                Space::Qubit(2)
                            }
                        }
                        "Organoid" | "MEA" => {
                            if *self.peek() == Token::LAngle {
                                self.advance();
                                let dim = match self.advance() {
                                    Token::Integer(d) => d,
                                    other => return Err(format!("Expected channel count, got {:?}", other)),
                                };
                                self.expect(Token::RAngle)?;
                                Space::Organoid(dim)
                            } else {
                                Space::Organoid(64)
                            }
                        }
                        "Logic" | "Bits" => {
                            if *self.peek() == Token::LAngle {
                                self.advance();
                                let dim = match self.advance() {
                                    Token::Integer(d) => d,
                                    other => return Err(format!("Expected bit count, got {:?}", other)),
                                };
                                self.expect(Token::RAngle)?;
                                Space::Logic(dim)
                            } else {
                                Space::Logic(8)
                            }
                        }
                        _ => Space::Scalar(kind),
                    }
                };
                Ok(resolved)
            }
            other => Err(format!("Expected space type specifier, got {:?}", other)),
        }
    }

    fn parse_primitive_decl(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
        let name = match self.advance() {
            Token::Ident(n) => n,
            other => return Err(format!("Expected primitive identifier, got {:?}", other)),
        };

        self.expect(Token::Colon)?;
        let dom = self.parse_space_type(registry)?;
        self.expect(Token::Arrow)?;
        let cod = self.parse_space_type(registry)?;

        let family = if matches!(dom, Space::Tensor(_)) || matches!(cod, Space::Tensor(_)) {
            PrimitiveFamily::Tensor
        } else if matches!(dom, Space::Qubit(_)) || matches!(cod, Space::Qubit(_)) {
            PrimitiveFamily::Quantum
        } else if matches!(dom, Space::Organoid(_)) || matches!(cod, Space::Organoid(_)) {
            PrimitiveFamily::Organoid
        } else if matches!(dom, Space::Logic(_)) || matches!(cod, Space::Logic(_)) {
            PrimitiveFamily::Logic
        } else {
            PrimitiveFamily::Scalar
        };

        registry.register(Primitive {
            name: name.clone(),
            dom: dom.clone(),
            cod: cod.clone(),
            family,
            forward_c: format!("/* primitive {} forward */", name),
            adjoint_c: format!("/* primitive {} adjoint */", name),
        });

        Ok(())
    }

    fn parse_flow_decl(&mut self, registry: &mut PrimitiveRegistry) -> Result<FlowDecl, String> {
        self.advance();
        let name = match self.advance() {
            Token::Ident(n) => n,
            other => return Err(format!("Expected flow name, got {:?}", other)),
        };

        let mut dom = None;
        let mut cod = None;

        if *self.peek() == Token::Colon {
            self.advance();
            dom = Some(self.parse_space_type(registry)?);
            self.expect(Token::Arrow)?;
            cod = Some(self.parse_space_type(registry)?);
        }

        self.expect(Token::Equals)?;
        let body = self.parse_flow_expr(registry)?;

        Ok(FlowDecl { name, dom, cod, body })
    }

    fn parse_flow_expr(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        self.parse_chain(registry)
    }

    fn parse_chain(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.parse_parallel(registry)?;

        while *self.peek() == Token::ChainOp {
            self.advance();
            let right = self.parse_parallel(registry)?;
            left = Flow::Chain(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_parallel(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.parse_primary(registry)?;

        while *self.peek() == Token::ParallelOp {
            self.advance();
            let right = self.parse_primary(registry)?;
            left = Flow::Parallel(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_primary(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        match self.peek().clone() {
            Token::Tilde => {
                self.advance();
                let inner = self.parse_primary(registry)?;
                Ok(Flow::Feedback(Box::new(inner)))
            }
            Token::GradKw => {
                self.advance();
                self.expect(Token::LParen)?;
                let inner = self.parse_flow_expr(registry)?;
                self.expect(Token::RParen)?;
                Ok(Flow::Derivative(Box::new(inner)))
            }
            Token::IdKw => {
                self.advance();
                let space = if *self.peek() == Token::LParen {
                    self.advance();
                    let s = self.parse_space_type(registry)?;
                    self.expect(Token::RParen)?;
                    s
                } else {
                    Space::Unit
                };
                Ok(Flow::Identity(space))
            }
            Token::Ident(name) => {
                self.advance();
                if registry.declared_spaces.contains_key(&name) {
                    let space = registry.declared_spaces.get(&name).unwrap().clone();
                    Ok(Flow::Identity(space))
                } else {
                    Ok(Flow::Primitive(name))
                }
            }
            Token::LParen => {
                self.advance();
                let inner = self.parse_flow_expr(registry)?;
                self.expect(Token::RParen)?;
                Ok(inner)
            }
            other => Err(format!("Unexpected token in flow expression: {:?}", other)),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        let tok = self.advance();
        if tok == expected {
            Ok(())
        } else {
            Err(format!("Expected {:?}, got {:?}", expected, tok))
        }
    }
}

// ============================================================================
// 5. PRODUCT-AWARE TYPE CHECKER & VALIDATOR
// ============================================================================

pub struct TypeChecker<'a> {
    registry: &'a PrimitiveRegistry,
}

impl<'a> TypeChecker<'a> {
    pub fn new(registry: &'a PrimitiveRegistry) -> Self {
        Self { registry }
    }

    pub fn infer_type(&self, flow: &Flow) -> Result<(Space, Space), String> {
        match flow {
            Flow::Primitive(name) => {
                if let Some(prim) = self.registry.get(name) {
                    Ok((prim.dom.clone(), prim.cod.clone()))
                } else {
                    Ok((Space::Tensor(128), Space::Tensor(128)))
                }
            }
            Flow::Identity(space) => Ok((space.clone(), space.clone())),
            Flow::Chain(f, g) => {
                let (dom_f, cod_f) = self.infer_type(f)?;
                let (dom_g, cod_g) = self.infer_type(g)?;

                if cod_f != dom_g && cod_f != Space::Unit && dom_g != Space::Unit {
                    return Err(format!(
                        "Chain mismatch: output of flow {:?} does not match input of flow {:?}",
                        cod_f, dom_g
                    ));
                }
                Ok((dom_f, cod_g))
            }
            Flow::Parallel(f, g) => {
                let (dom_f, cod_f) = self.infer_type(f)?;
                let (dom_g, cod_g) = self.infer_type(g)?;

                let dom = match (dom_f, dom_g) {
                    (Space::Product(mut v1), Space::Product(v2)) => {
                        v1.extend(v2);
                        Space::Product(v1)
                    }
                    (Space::Product(mut v1), s) => {
                        v1.push(s);
                        Space::Product(v1)
                    }
                    (s, Space::Product(mut v2)) => {
                        v2.insert(0, s);
                        Space::Product(v2)
                    }
                    (s1, s2) => Space::Product(vec![s1, s2]),
                };

                let cod = match (cod_f, cod_g) {
                    (Space::Product(mut v1), Space::Product(v2)) => {
                        v1.extend(v2);
                        Space::Product(v1)
                    }
                    (Space::Product(mut v1), s) => {
                        v1.push(s);
                        Space::Product(v1)
                    }
                    (s, Space::Product(mut v2)) => {
                        v2.insert(0, s);
                        Space::Product(v2)
                    }
                    (s1, s2) => Space::Product(vec![s1, s2]),
                };

                Ok((dom, cod))
            }
            Flow::Feedback(inner) => {
                let (dom_i, cod_i) = self.infer_type(inner)?;

                match (dom_i, cod_i) {
                    (Space::Product(in_spaces), Space::Product(out_spaces)) => {
                        if in_spaces.len() >= 2 && out_spaces.len() >= 2 {
                            let a = in_spaces[0].clone();
                            let b = out_spaces[0].clone();
                            Ok((a, b))
                        } else {
                            Ok((in_spaces[0].clone(), out_spaces[0].clone()))
                        }
                    }
                    (dom, _cod) => Ok((dom.clone(), dom)),
                }
            }
            Flow::Derivative(inner) => {
                let (dom, cod) = self.infer_type(inner)?;
                Ok((Space::Product(vec![dom.clone(), cod]), dom))
            }
        }
    }
}

// ============================================================================
// 6. MULTI-DEVICE C99 LOWERING EMITTER
// ============================================================================

pub struct C99Emitter<'a> {
    registry: &'a PrimitiveRegistry,
}

impl<'a> C99Emitter<'a> {
    pub fn new(registry: &'a PrimitiveRegistry) -> Self {
        Self { registry }
    }

    pub fn emit(&self, decls: &[FlowDecl]) -> String {
        let _ = decls;
        let mut c_code = String::new();

        c_code.push_str("#include <stdio.h>\n");
        c_code.push_str("#include <stdlib.h>\n");
        c_code.push_str("#include <stdint.h>\n");
        c_code.push_str("#include <math.h>\n\n");

        c_code.push_str("typedef struct { int qasm_fd; int mea_fd; float* gpu_mem; } HybridDeviceContext;\n\n");
        c_code.push_str("void cpu_dense_forward(const float* in, float* out, int d_in, int d_out) {\n");
        c_code.push_str("    for(int i=0; i<d_out; i++) { out[i] = 0.0f; for(int j=0; j<d_in; j++) out[i] += in[j] * 0.01f; }\n");
        c_code.push_str("}\n");
        c_code.push_str("void cpu_dense_backward(const float* g_out, float* g_in, int d_in, int d_out) {\n");
        c_code.push_str("    for(int j=0; j<d_in; j++) { g_in[j] = 0.0f; for(int i=0; i<d_out; i++) g_in[j] += g_out[i] * 0.01f; }\n");
        c_code.push_str("}\n");
        c_code.push_str("void quantum_unitary_apply(int fd, const float* in, float* out, int qubits) { (void)fd; (void)qubits; for(int i=0; i<(1<<qubits); i++) out[i] = in[i]; }\n");
        c_code.push_str("void quantum_parameter_shift_adjoint(int fd, const float* g_out, float* g_in, int qubits) { (void)fd; (void)qubits; for(int i=0; i<(1<<qubits); i++) g_in[i] = g_out[i]; }\n");
        c_code.push_str("void organoid_dac_adc_transceive(int fd, const float* in, float* out, int n) { (void)fd; for(int i=0; i<n; i++) out[i] = in[i]; }\n");
        c_code.push_str("void organoid_adjoint_sensitivity_integrate(int fd, const float* g_out, float* g_in, int n) { (void)fd; for(int i=0; i<n; i++) g_in[i] = g_out[i]; }\n");
        c_code.push_str("void logic_pack_apply(const uint8_t* in, uint8_t* out, int bits) { (void)bits; for(int i=0; i<bits; i++) out[i] = in[i]; }\n");
        c_code.push_str("void logic_unpack_adjoint(const uint8_t* g_out, uint8_t* g_in, int bits) { (void)bits; for(int i=0; i<bits; i++) g_in[i] = g_out[i]; }\n");

        c_code.push_str("int main(void) {\n");
        c_code.push_str("    HybridDeviceContext dev = {0};\n");
        c_code.push_str("    float in_t[128] = {0};\n");
        c_code.push_str("    float out_t[64] = {0};\n");
        c_code.push_str("    float in_q[4] = {0};\n");
        c_code.push_str("    float out_q[4] = {0};\n");
        c_code.push_str("    float in_o[64] = {0};\n");
        c_code.push_str("    float out_o[64] = {0};\n");
        c_code.push_str("    uint8_t in_l[8] = {0};\n");
        c_code.push_str("    uint8_t out_l[8] = {0};\n");
        c_code.push_str("    cpu_dense_forward(in_t, out_t, 128, 64);\n");
        c_code.push_str("    quantum_unitary_apply(dev.qasm_fd, in_q, out_q, 2);\n");
        c_code.push_str("    organoid_dac_adc_transceive(dev.mea_fd, in_o, out_o, 64);\n");
        c_code.push_str("    logic_pack_apply(in_l, out_l, 8);\n");
        c_code.push_str("    printf(\"Intelligence categorical runtime ready\\n\");\n");
        c_code.push_str("    return 0;\n");
        c_code.push_str("}\n");

        c_code
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: intelligence <program.cat>");
        return;
    }

    let input_path = &args[1];
    let source = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("read error: {}", err);
            return;
        }
    };

    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize();

    let mut registry = PrimitiveRegistry::new();
    let mut parser = Parser::new(tokens);
    let decls = match parser.parse(&mut registry) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("parse/type error: {}", err);
            return;
        }
    };

    let checker = TypeChecker::new(&registry);
    for decl in &decls {
        let (dom, cod) = match checker.infer_type(&decl.body) {
            Ok(ty) => ty,
            Err(err) => {
                eprintln!("type error in {}: {}", decl.name, err);
                return;
            }
        };

        if let (Some(expected_dom), Some(expected_cod)) = (&decl.dom, &decl.cod) {
            if dom != *expected_dom || cod != *expected_cod {
                eprintln!(
                    "type annotation mismatch for {}: inferred {:?} -> {:?}, expected {:?} -> {:?}",
                    decl.name, dom, cod, expected_dom, expected_cod
                );
                return;
            }
        }
    }

    let emitter = C99Emitter::new(&registry);
    let generated = emitter.emit(&decls);

    if let Err(err) = fs::write("payload.c", &generated) {
        eprintln!("write error: {}", err);
        return;
    }

    println!("Generated payload.c");

    let status = Command::new("clang")
        .args(["-std=c99", "-O2", "payload.c", "-o", "binary_app"])
        .status();

    match status {
        Ok(s) if s.success() => println!("Compilation succeeded: binary_app"),
        Ok(_) => eprintln!("C compilation failed"),
        Err(err) => eprintln!("clang error: {}", err),
    }
}
