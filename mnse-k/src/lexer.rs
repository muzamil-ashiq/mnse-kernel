//! Lexer / Tokenizer for K Language.
//!
//! Handles shebang (`#!k`) stripping, string literals, numbers, variables (`$var`),
//! keywords, and language operators.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    KwLet,
    KwIf,
    KwElse,
    KwTry,
    KwCatch,
    KwSense,
    KwWatch,
    KwBehavior,
    KwCurrent,
    KwRecent,
    KwHistory,
    KwCause,
    KwCauses,
    KwRelated,
    KwWhere,
    KwSelect,
    KwCare,
    KwSummary,
    KwAdapter,
    KwOn,
    KwOff,
    KwEnable,
    KwDisable,
    KwList,
    KwSh,

    // Identifiers & Variables
    Ident(String),
    Var(String),
    Str(String),
    Num(f64),

    // Symbols & Operators
    Pipe,       // |
    Arrow,      // ->
    DotDot,     // ..
    Minus,      // -
    Plus,       // +
    EqEq,       // ==
    Neq,        // !=
    Gt,         // >
    Lt,         // <
    Gte,        // >=
    Lte,        // <=
    Assign,     // =
    LBrace,     // {
    RBrace,     // }
    LBracket,   // [
    RBracket,   // ]
    LParen,     // (
    RParen,     // )
    Colon,      // :
    Dot,        // .
    Comma,      // ,
    Semicolon,  // ;

    Eof,
}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    pending_token: Option<Token>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        // Strip shebang if present on line 1
        let trimmed = if input.starts_with("#!") {
            if let Some(newline_pos) = input.find('\n') {
                &input[newline_pos + 1..]
            } else {
                ""
            }
        } else {
            input
        };

        Self {
            input: trimmed,
            pos: 0,
            pending_token: None,
        }
    }

    pub fn tokenize(&mut self) -> anyhow::Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            if tok == Token::Eof {
                tokens.push(Token::Eof);
                break;
            }
            tokens.push(tok);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> anyhow::Result<Token> {
        if let Some(tok) = self.pending_token.take() {
            return Ok(tok);
        }

        self.skip_whitespace_and_comments();

        if self.pos >= self.input.len() {
            return Ok(Token::Eof);
        }

        let rest = &self.input[self.pos..];
        let ch = rest.chars().next().unwrap();

        // Two-character operators
        if rest.starts_with("->") {
            self.pos += 2;
            return Ok(Token::Arrow);
        }
        if rest.starts_with("==") {
            self.pos += 2;
            return Ok(Token::EqEq);
        }
        if rest.starts_with("!=") {
            self.pos += 2;
            return Ok(Token::Neq);
        }
        if rest.starts_with(">=") {
            self.pos += 2;
            return Ok(Token::Gte);
        }
        if rest.starts_with("<=") {
            self.pos += 2;
            return Ok(Token::Lte);
        }
        if rest.starts_with("..") && !rest.starts_with("../") {
            self.pos += 2;
            return Ok(Token::DotDot);
        }

        // File path literal: /path/to/file, ./file, ../file, ~/file
        if (ch == '/' && !rest.starts_with("//"))
            || rest.starts_with("./")
            || rest.starts_with("../")
            || rest.starts_with("~/")
        {
            let start = self.pos;
            while self.pos < self.input.len() {
                let c = self.input[self.pos..].chars().next().unwrap();
                if c.is_whitespace() || c == ';' || c == '|' || c == ']' || c == ')' || c == '}' || c == ',' {
                    break;
                }
                self.pos += c.len_utf8();
            }
            let path_str = self.input[start..self.pos].to_string();
            return Ok(Token::Str(path_str));
        }

        // Single character symbols
        match ch {
            '|' => { self.pos += 1; return Ok(Token::Pipe); }
            '-' => { self.pos += 1; return Ok(Token::Minus); }
            '+' => { self.pos += 1; return Ok(Token::Plus); }
            '=' => { self.pos += 1; return Ok(Token::Assign); }
            '>' => { self.pos += 1; return Ok(Token::Gt); }
            '<' => { self.pos += 1; return Ok(Token::Lt); }
            '{' => { self.pos += 1; return Ok(Token::LBrace); }
            '}' => { self.pos += 1; return Ok(Token::RBrace); }
            '[' => { self.pos += 1; return Ok(Token::LBracket); }
            ']' => { self.pos += 1; return Ok(Token::RBracket); }
            '(' => { self.pos += 1; return Ok(Token::LParen); }
            ')' => { self.pos += 1; return Ok(Token::RParen); }
            ':' => { self.pos += 1; return Ok(Token::Colon); }
            '.' => { self.pos += 1; return Ok(Token::Dot); }
            ',' => { self.pos += 1; return Ok(Token::Comma); }
            ';' => { self.pos += 1; return Ok(Token::Semicolon); }
            _ => {}
        }

        // Variable: $name or $it
        if ch == '$' {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.input.len() {
                let c = self.input[self.pos..].chars().next().unwrap();
                if c.is_alphanumeric() || c == '_' || c == '.' {
                    self.pos += c.len_utf8();
                } else {
                    break;
                }
            }
            let var_name = self.input[start..self.pos].to_string();
            return Ok(Token::Var(var_name));
        }

        // String literal: "..." or '...'
        if ch == '"' || ch == '\'' {
            let quote = ch;
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.input.len() {
                let c = self.input[self.pos..].chars().next().unwrap();
                if c == quote {
                    let s = self.input[start..self.pos].to_string();
                    self.pos += 1; // skip closing quote
                    return Ok(Token::Str(s));
                }
                self.pos += c.len_utf8();
            }
            return Err(anyhow::anyhow!("Unterminated string literal"));
        }

        // Number literal: 123, 45.6 or Date literal: 2026-10-06
        if ch.is_ascii_digit() {
            let start = self.pos;
            let mut has_dot = false;
            let mut has_dash = false;
            while self.pos < self.input.len() {
                let c = self.input[self.pos..].chars().next().unwrap();
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else if c == '.' && !has_dot && !has_dash {
                    // Check if next char is digit (so it's float, not field access .field)
                    let next_char = self.input[self.pos + 1..].chars().next();
                    if next_char.map_or(false, |n| n.is_ascii_digit()) {
                        has_dot = true;
                        self.pos += 1;
                    } else {
                        break;
                    }
                } else if c == '-' {
                    // Date literal like 2026-10-06
                    let next_char = self.input[self.pos + 1..].chars().next();
                    if next_char.map_or(false, |n| n.is_ascii_digit()) {
                        has_dash = true;
                        self.pos += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            let raw_str = &self.input[start..self.pos];
            if has_dash {
                return Ok(Token::Ident(raw_str.to_string()));
            } else {
                let num: f64 = raw_str.parse()?;
                return Ok(Token::Num(num));
            }
        }

        // Identifiers & Keywords
        if ch.is_alphabetic() || ch == '_' {
            let start = self.pos;
            while self.pos < self.input.len() {
                let c = self.input[self.pos..].chars().next().unwrap();
                if c.is_alphanumeric() || c == '_' || c == '.' {
                    self.pos += c.len_utf8();
                } else {
                    break;
                }
            }
            let word = &self.input[start..self.pos];
            return Ok(match word {
                "let" => Token::KwLet,
                "if" => Token::KwIf,
                "else" => Token::KwElse,
                "try" => Token::KwTry,
                "catch" => Token::KwCatch,
                "sense" => Token::KwSense,
                "watch" => Token::KwWatch,
                "behavior" => Token::KwBehavior,
                "current" => Token::KwCurrent,
                "recent" => Token::KwRecent,
                "history" => Token::KwHistory,
                "cause" => Token::KwCause,
                "causes" => Token::KwCauses,
                "related" => Token::KwRelated,
                "where" => Token::KwWhere,
                "select" => Token::KwSelect,
                "care" => Token::KwCare,
                "summary" => Token::KwSummary,
                "adapter" | "adapters" => Token::KwAdapter,
                "on" => Token::KwOn,
                "off" => Token::KwOff,
                "enable" => Token::KwEnable,
                "disable" => Token::KwDisable,
                "list" => Token::KwList,
                "sh" => {
                    self.skip_whitespace_and_comments();
                    if self.pos < self.input.len() && self.input[self.pos..].starts_with('{') {
                        self.pos += 1; // consume '{'
                        let block_start = self.pos;
                        let mut depth = 1;
                        let mut in_str = false;
                        let mut escape = false;
                        while self.pos < self.input.len() && depth > 0 {
                            let c = self.input[self.pos..].chars().next().unwrap();
                            let c_len = c.len_utf8();
                            if in_str {
                                if escape {
                                    escape = false;
                                } else if c == '\\' {
                                    escape = true;
                                } else if c == '"' {
                                    in_str = false;
                                }
                            } else {
                                if c == '"' {
                                    in_str = true;
                                } else if c == '{' {
                                    depth += 1;
                                } else if c == '}' {
                                    depth -= 1;
                                    if depth == 0 {
                                        let block = self.input[block_start..self.pos].trim().to_string();
                                        self.pos += c_len; // consume closing '}'
                                        self.pending_token = Some(Token::Str(block));
                                        return Ok(Token::KwSh);
                                    }
                                }
                            }
                            self.pos += c_len;
                        }
                        return Err(anyhow::anyhow!("Unterminated shell block: missing closing '}}'"));
                    } else {
                        Token::KwSh
                    }
                }
                _ => Token::Ident(word.to_string()),
            });
        }

        Err(anyhow::anyhow!("Unexpected character: '{}' at position {}", ch, self.pos))
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.input.len() {
            let rest = &self.input[self.pos..];
            let ch = rest.chars().next().unwrap();

            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else if rest.starts_with("//") || rest.starts_with('#') {
                // Skip line comment
                if let Some(newline_pos) = rest.find('\n') {
                    self.pos += newline_pos + 1;
                } else {
                    self.pos = self.input.len();
                }
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_tokens() {
        let mut lexer = Lexer::new("let $x = current.app | where $it == \"zed\"");
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0], Token::KwLet);
        assert_eq!(tokens[1], Token::Var("x".into()));
        assert_eq!(tokens[2], Token::Assign);
        assert_eq!(tokens[3], Token::Ident("current.app".into()));
        assert_eq!(tokens[4], Token::Pipe);
        assert_eq!(tokens[5], Token::KwWhere);
        assert_eq!(tokens[6], Token::Var("it".into()));
        assert_eq!(tokens[7], Token::EqEq);
        assert_eq!(tokens[8], Token::Str("zed".into()));
        assert_eq!(tokens[9], Token::Eof);
    }

    #[test]
    fn test_lexer_sh_block() {
        let mut lexer = Lexer::new("sh { pdftotext -layout $file - }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::KwSh);
        assert_eq!(tokens[1], Token::Str("pdftotext -layout $file -".into()));
        assert_eq!(tokens[2], Token::Eof);
    }

    #[test]
    fn test_lexer_vector_slice_tokens() {
        let mut lexer = Lexer::new("data[0..-1]");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::Ident("data".into()));
        assert_eq!(tokens[1], Token::LBracket);
        assert_eq!(tokens[2], Token::Num(0.0));
        assert_eq!(tokens[3], Token::DotDot);
        assert_eq!(tokens[4], Token::Minus);
        assert_eq!(tokens[5], Token::Num(1.0));
        assert_eq!(tokens[6], Token::RBracket);
    }
}
