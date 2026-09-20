use crate::language::{default_primitives, Flow, Program, Space, typecheck};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    SpaceKw,
    FlowKw,
    Ident(String),
    Number(usize),
    Equals,
    Colon,
    Arrow,
    Chain,
    Parallel,
    Feedback,
    LParen,
    RParen,
    LAngle,
    RAngle,
    Comma,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parser {
    tokens: Vec<Token>,
    index: usize,
    spaces: HashMap<String, Space>,
}

impl Parser {
    fn new(source: &str) -> Result<Self, String> {
        Ok(Self {
            tokens: tokenize(source)?,
            index: 0,
            spaces: HashMap::new(),
        })
    }

    fn peek(&self) -> &Token { &self.tokens[self.index] }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        self.index += 1;
        token
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        let actual = self.advance();
        if actual == expected {
            Ok(())
        } else {
            Err(format!("expected {:?}, found {:?}", expected, actual))
        }
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            token => Err(format!("expected identifier, found {:?}", token)),
        }
    }

    fn parse_space_atom(&mut self) -> Result<Space, String> {
        let name = self.parse_ident()?;
        let mut dims = Vec::new();
        if matches!(self.peek(), Token::LAngle) {
            self.advance();
            while !matches!(self.peek(), Token::RAngle) {
                match self.advance() {
                    Token::Number(value) => dims.push(value),
                    token => return Err(format!("invalid dimension token {:?}", token)),
                }
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RAngle)?;
        }

        match name.as_str() {
            "Tensor" | "Array" => {
                let dim = dims.first().copied().unwrap_or(1);
                Ok(Space::Tensor { element: "f32".into(), shape: vec![dim] })
            }
            "Quantum" | "Qubit" => {
                let dim = dims.first().copied().unwrap_or(1);
                Ok(Space::Quantum { qubits: dim })
            }
            "Organoid" | "MEA" => {
                let dim = dims.first().copied().unwrap_or(1);
                Ok(Space::Organoid { pins: dim })
            }
            "Logic" | "Bits" => {
                let dim = dims.first().copied().unwrap_or(1);
                Ok(Space::Logic { bits: dim })
            }
            "Scalar" => Ok(Space::Scalar("f64".into())),
            "Unit" => Ok(Space::Unit),
            other if self.spaces.contains_key(other) => Ok(self.spaces[other].clone()),
            other => Err(format!("unknown type `{other}`")),
        }
    }

    fn parse_space(&mut self) -> Result<Space, String> {
        let first = self.parse_space_atom()?;
        if matches!(self.peek(), Token::Parallel) {
            let mut parts = vec![first];
            while matches!(self.peek(), Token::Parallel) {
                self.advance();
                parts.push(self.parse_space_atom()?);
            }
            Ok(Space::Product(parts))
        } else {
            Ok(first)
        }
    }

    fn parse_flow_atom(&mut self) -> Result<Flow, String> {
        match self.peek() {
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(Flow::Primitive { name, domain: Space::Unit, codomain: Space::Unit })
            }
            Token::Feedback => {
                self.advance();
                Ok(Flow::Feedback(Box::new(self.parse_flow_atom()?)))
            }
            Token::LParen => {
                self.advance();
                let flow = self.parse_flow()?;
                self.expect(Token::RParen)?;
                Ok(flow)
            }
            Token::Ident(_) => unreachable!(),
            token => Err(format!("unexpected token while parsing flow: {:?}", token)),
        }
    }

    fn parse_flow(&mut self) -> Result<Flow, String> {
        let mut left = self.parse_flow_atom()?;
        while matches!(self.peek(), Token::Chain) {
            self.advance();
            let right = self.parse_flow_atom()?;
            left = Flow::Chain(Box::new(left), Box::new(right));
        }
        while matches!(self.peek(), Token::Parallel) {
            self.advance();
            let right = self.parse_flow_atom()?;
            left = Flow::Parallel(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_program(&mut self) -> Result<Program, String> {
        let mut spaces = HashMap::new();
        let mut flows = HashMap::new();
        while !matches!(self.peek(), Token::Eof) {
            match self.peek() {
                Token::SpaceKw => {
                    self.advance();
                    let name = self.parse_ident()?;
                    self.expect(Token::Equals)?;
                    let dimension = self.parse_space()?;
                    spaces.insert(name, dimension);
                    self.spaces = spaces.clone();
                }
                Token::FlowKw => {
                    self.advance();
                    let name = self.parse_ident()?;
                    let mut flow = if matches!(self.peek(), Token::Colon) {
                        self.advance();
                        let domain = self.parse_space()?;
                        self.expect(Token::Arrow)?;
                        let codomain = self.parse_space()?;
                        let body = self.parse_flow()?;
                        typecheck(&body, &default_primitives())?;
                        let domain_match = body.domain() == domain;
                        let codomain_match = body.codomain() == codomain;
                        if !domain_match || !codomain_match {
                            return Err(format!("flow `{name}` annotates {:?} -> {:?}, but body is {:?} -> {:?}", domain, codomain, body.domain(), body.codomain()));
                        }
                        body
                    } else {
                        self.expect(Token::Equals)?;
                        self.parse_flow()?
                    };
                    flows.insert(name, flow);
                }
                _ => {
                    return Err(format!("unexpected top-level token {:?}", self.peek()));
                }
            }
        }
        Ok(Program { spaces, flows })
    }
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            ' ' | '\n' | '\r' | '\t' => {}
            '/' => {
                if chars.peek() == Some(&'/') {
                    chars.next();
                    while chars.peek().is_some() && chars.peek() != Some(&'\n') {
                        chars.next();
                    }
                } else {
                    return Err("unexpected `/` token".to_string());
                }
            }
            '=' => tokens.push(Token::Equals),
            ':' => tokens.push(Token::Colon),
            '~' => tokens.push(Token::Feedback),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '<' => tokens.push(Token::LAngle),
            '>' => tokens.push(Token::RAngle),
            ',' => tokens.push(Token::Comma),
            '|' => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token::Parallel);
                } else {
                    return Err("unexpected `|` token".to_string());
                }
            }
            '>' => {
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(Token::Chain);
                } else {
                    return Err("unexpected `>` token".to_string());
                }
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut word = String::new();
                word.push(c);
                while let Some(next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || *next == '_' {
                        word.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                match word.as_str() {
                    "space" => tokens.push(Token::SpaceKw),
                    "flow" => tokens.push(Token::FlowKw),
                    _ => tokens.push(Token::Ident(word)),
                }
            }
            c if c.is_ascii_digit() => {
                let mut value = c.to_digit(10).unwrap() as usize;
                while let Some(next) = chars.peek() {
                    if next.is_ascii_digit() {
                        value = value * 10 + chars.next().unwrap().to_digit(10).unwrap() as usize;
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Number(value));
            }
            c => return Err(format!("unexpected character `{c}`")),
        }
    }
    tokens.push(Token::Eof);
    Ok(tokens)
}

pub fn parse_program(source: &str, primitives: &HashMap<String, Signature>) -> Result<Program, String> {
    let mut parser = Parser::new(source)?;
    let program = parser.parse_program()?;
    for flow in program.flows.values() {
        typecheck(flow, primitives)?;
    }
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_flow() {
        let primitives = default_primitives();
        let source = "space A = Tensor<128>\nspace B = Tensor<64>\nflow f : A -> B = neural_layer\n";
        let program = parse_program(source, &primitives).unwrap();
        assert!(program.flows.contains_key("f"));
    }
}














































































































