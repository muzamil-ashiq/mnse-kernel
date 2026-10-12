//! Parser for K Language.
//!
//! Transforms token stream into KProgram AST.

use crate::ast::*;
use crate::lexer::Token;
use mnse_types::{ActionPayload, LookDirection};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_program(&mut self) -> anyhow::Result<KProgram> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            // Skip optional semicolons
            if self.peek() == &Token::Semicolon {
                self.advance();
                continue;
            }
            statements.push(self.parse_statement()?);
        }

        Ok(KProgram { statements })
    }

    fn parse_statement(&mut self) -> anyhow::Result<KStatement> {
        let token = self.peek().clone();

        match token {
            Token::KwLet => self.parse_let(),
            Token::KwIf => self.parse_if(),
            Token::KwTry => self.parse_try_catch(),
            Token::KwBehavior => self.parse_behavior(),
            Token::KwCare => {
                self.advance();
                Ok(KStatement::Care)
            }
            Token::Ident(ref s) if s == "help" => {
                self.advance();
                Ok(KStatement::Help)
            }
            Token::KwAdapter => {
                self.advance();
                let action = match self.peek() {
                    Token::KwOn => { self.advance(); "on".to_string() }
                    Token::KwOff => { self.advance(); "off".to_string() }
                    Token::KwEnable => { self.advance(); "enable".to_string() }
                    Token::KwDisable => { self.advance(); "disable".to_string() }
                    Token::KwList => { self.advance(); "list".to_string() }
                    Token::Ident(s) => {
                        let a = s.clone();
                        self.advance();
                        a
                    }
                    _ => "list".to_string(),
                };

                let name = if let Token::Ident(s) = self.peek() {
                    let n = s.clone();
                    self.advance();
                    Some(n)
                } else {
                    None
                };

                Ok(KStatement::Adapter { action, name })
            }
            Token::KwSummary => {
                self.advance();
                let range = if let Token::Ident(s) = self.peek() {
                    let r = s.clone();
                    self.advance();
                    r
                } else if let Token::Str(s) = self.peek() {
                    let r = s.clone();
                    self.advance();
                    r
                } else {
                    "today".to_string()
                };
                Ok(KStatement::Summary { range })
            }
            Token::KwSpeak => {
                self.advance();
                let text = self.expect_str_or_ident()?;
                Ok(KStatement::Action(ActionPayload::Speak {
                    text,
                    voice: None,
                    speed: None,
                }))
            }
            Token::KwLook => {
                self.advance();
                let dir_str = self.expect_str_or_ident()?;
                let dir = match dir_str.to_lowercase().as_str() {
                    "left" => LookDirection::Left,
                    "right" => LookDirection::Right,
                    "down" => LookDirection::Down,
                    "forward" => LookDirection::Forward,
                    "trackface" => LookDirection::TrackFace,
                    _ => LookDirection::Up,
                };
                Ok(KStatement::Action(ActionPayload::LookAt { direction: dir }))
            }
            Token::KwFollow => {
                self.advance();
                let target = self.expect_str_or_ident()?;
                Ok(KStatement::Action(ActionPayload::Follow { target }))
            }
            _ => {
                let expr = self.parse_expr()?;
                Ok(KStatement::Expr(expr))
            }
        }
    }

    fn parse_let(&mut self) -> anyhow::Result<KStatement> {
        self.advance(); // consume 'let'
        let name = match self.advance().clone() {
            Token::Var(v) => v,
            Token::Ident(i) => i,
            t => return Err(anyhow::anyhow!("Expected variable name after let, found {:?}", t)),
        };

        self.expect_token(Token::Assign)?;
        let val = self.parse_expr()?;
        Ok(KStatement::Let { name, val })
    }

    fn parse_if(&mut self) -> anyhow::Result<KStatement> {
        self.advance(); // consume 'if'
        let cond = self.parse_expr()?;
        let then_branch = self.parse_block()?;

        let else_branch = if self.peek() == &Token::KwElse {
            self.advance();
            if self.peek() == &Token::KwIf {
                vec![self.parse_if()?]
            } else {
                self.parse_block()?
            }
        } else {
            Vec::new()
        };

        Ok(KStatement::If {
            cond,
            then_branch,
            else_branch: if else_branch.is_empty() { None } else { Some(else_branch) },
        })
    }

    fn parse_try_catch(&mut self) -> anyhow::Result<KStatement> {
        self.advance(); // consume 'try'
        let try_body = self.parse_block()?;

        if self.peek() == &Token::KwCatch {
            self.advance();
            let catch_var = if self.peek() == &Token::LParen {
                self.advance();
                let v = match self.advance().clone() {
                    Token::Var(var) => var,
                    Token::Ident(id) => id,
                    t => return Err(anyhow::anyhow!("Expected catch variable, found {:?}", t)),
                };
                self.expect_token(Token::RParen)?;
                v
            } else {
                "err".to_string()
            };

            let catch_body = self.parse_block()?;
            Ok(KStatement::TryCatch {
                try_body,
                catch_var,
                catch_body,
            })
        } else {
            Ok(KStatement::TryCatch {
                try_body,
                catch_var: "err".to_string(),
                catch_body: Vec::new(),
            })
        }
    }

    fn parse_behavior(&mut self) -> anyhow::Result<KStatement> {
        self.advance(); // consume 'behavior'
        let name = self.expect_ident()?;
        self.expect_token(Token::LBrace)?;

        let mut triggers = Vec::new();
        while self.peek() != &Token::RBrace && !self.is_at_end() {
            if self.peek() == &Token::KwSense || self.peek() == &Token::KwWatch {
                self.advance();
                let event_type_name = self.expect_ident_or_str()?;
                self.expect_token(Token::Arrow)?;
                let body = self.parse_block()?;
                triggers.push(BehaviorTrigger {
                    event_type_name,
                    body,
                });
            } else {
                self.advance();
            }
        }

        self.expect_token(Token::RBrace)?;
        Ok(KStatement::Behavior { name, triggers })
    }

    fn parse_expr(&mut self) -> anyhow::Result<KExpr> {
        let left = self.parse_primary_expr()?;

        // Check pipeline operator '|'
        if self.peek() == &Token::Pipe {
            let mut pipeline = vec![left];
            while self.peek() == &Token::Pipe {
                self.advance(); // consume '|'
                pipeline.push(self.parse_pipeline_stage()?);
            }
            return Ok(KExpr::Pipeline(pipeline));
        }

        // Check binary comparison operators (==, !=, >, <, etc.)
        match self.peek() {
            Token::EqEq | Token::Neq | Token::Gt | Token::Lt | Token::Gte | Token::Lte => {
                let op = format!("{:?}", self.advance());
                let right = self.parse_primary_expr()?;
                return Ok(KExpr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            }
            _ => {}
        }

        Ok(left)
    }

    fn parse_pipeline_stage(&mut self) -> anyhow::Result<KExpr> {
        match self.peek() {
            Token::KwWhere => {
                self.advance();
                let pred = self.parse_expr()?;
                Ok(KExpr::Where(Box::new(pred)))
            }
            Token::KwSelect => {
                self.advance();
                let mut fields = Vec::new();
                while !self.is_at_end() && self.peek() != &Token::Pipe && self.peek() != &Token::RBrace {
                    let field = self.expect_ident_or_str()?;
                    fields.push(field);
                    if self.peek() == &Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                Ok(KExpr::Select(fields))
            }
            _ => self.parse_expr(),
        }
    }

    fn parse_primary_expr(&mut self) -> anyhow::Result<KExpr> {
        let token = self.peek().clone();

        match token {
            Token::KwCurrent => {
                self.advance();
                let target = if self.peek() == &Token::Dot {
                    self.advance();
                    self.expect_ident()?
                } else if let Token::Ident(i) = self.peek() {
                    let t = i.clone();
                    self.advance();
                    t
                } else {
                    "state".to_string()
                };
                Ok(KExpr::Current(target))
            }

            Token::KwRecent => {
                self.advance();
                let target = if self.peek() == &Token::Dot {
                    self.advance();
                    self.expect_ident()?
                } else if let Token::Ident(i) = self.peek() {
                    let t = i.clone();
                    self.advance();
                    t
                } else {
                    "errors".to_string()
                };
                Ok(KExpr::Recent(target))
            }

            Token::KwHistory => {
                self.advance();
                let range = if self.peek() == &Token::Dot {
                    self.advance();
                    self.expect_ident()?
                } else if let Token::Ident(i) = self.peek() {
                    let r = i.clone();
                    self.advance();
                    r
                } else if let Token::Str(s) = self.peek() {
                    let r = s.clone();
                    self.advance();
                    r
                } else {
                    "today".to_string()
                };
                Ok(KExpr::History(range))
            }

            Token::KwCause | Token::KwCauses => {
                self.advance();
                let target = if self.peek() == &Token::Ident("of".into()) {
                    self.advance();
                    self.expect_ident_or_str()?
                } else {
                    "latest.error".to_string()
                };
                Ok(KExpr::Causal {
                    op: "cause".to_string(),
                    target,
                })
            }

            Token::KwRelated => {
                self.advance();
                let query = self.expect_str_or_ident()?;
                Ok(KExpr::Causal {
                    op: "related".to_string(),
                    target: query,
                })
            }

            Token::Var(v) => {
                self.advance();
                Ok(KExpr::Var(v))
            }

            Token::Str(s) => {
                self.advance();
                Ok(KExpr::Literal(serde_json::Value::String(s)))
            }

            Token::Num(n) => {
                self.advance();
                Ok(KExpr::Literal(serde_json::json!(n)))
            }

            Token::Ident(s) => {
                self.advance();
                if s.starts_with("current.") {
                    Ok(KExpr::Current(s[8..].to_string()))
                } else if s.starts_with("recent.") {
                    Ok(KExpr::Recent(s[7..].to_string()))
                } else if s.starts_with("history.") {
                    Ok(KExpr::History(s[8..].to_string()))
                } else if s == "state" || s == "snapshot" {
                    Ok(KExpr::Current("state".to_string()))
                } else if s == "app" {
                    Ok(KExpr::Current("app".to_string()))
                } else if s == "errors" {
                    Ok(KExpr::Recent("errors".to_string()))
                } else if s == "commands" {
                    Ok(KExpr::Recent("commands".to_string()))
                } else if s == "files" {
                    // Check if next token is a range (e.g. 'today', 'yesterday')
                    if let Token::Ident(range) = self.peek() {
                        if range == "today" || range == "yesterday" || range.starts_with("202") {
                            let r = range.clone();
                            self.advance();
                            return Ok(KExpr::History(r));
                        }
                    }
                    Ok(KExpr::Recent("files".to_string()))
                } else if s == "help" {
                    Ok(KExpr::Help)
                } else if s == "true" {
                    Ok(KExpr::Literal(serde_json::Value::Bool(true)))
                } else if s == "false" {
                    Ok(KExpr::Literal(serde_json::Value::Bool(false)))
                } else if s == "null" {
                    Ok(KExpr::Literal(serde_json::Value::Null))
                } else {
                    anyhow::bail!(
                        "Unknown query or command: '{}'. Valid queries: current.state, recent.errors, recent.files, recent.commands, history today, summary today, adapter list, care, help.",
                        s
                    )
                }
            }

            _ => Err(anyhow::anyhow!("Unexpected expression token: {:?}", token)),
        }
    }

    fn parse_block(&mut self) -> anyhow::Result<Vec<KStatement>> {
        self.expect_token(Token::LBrace)?;
        let mut body = Vec::new();
        while self.peek() != &Token::RBrace && !self.is_at_end() {
            if self.peek() == &Token::Semicolon {
                self.advance();
                continue;
            }
            body.push(self.parse_statement()?);
        }
        self.expect_token(Token::RBrace)?;
        Ok(body)
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.pos += 1;
        }
        self.tokens.get(self.pos - 1).unwrap_or(&Token::Eof)
    }

    fn is_at_end(&self) -> bool {
        self.peek() == &Token::Eof
    }

    fn expect_token(&mut self, expected: Token) -> anyhow::Result<()> {
        let tok = self.advance();
        if std::mem::discriminant(tok) == std::mem::discriminant(&expected) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Expected token {:?}, found {:?}", expected, tok))
        }
    }

    fn expect_ident(&mut self) -> anyhow::Result<String> {
        match self.advance().clone() {
            Token::Ident(s) => Ok(s),
            t => Err(anyhow::anyhow!("Expected identifier, found {:?}", t)),
        }
    }

    fn expect_str_or_ident(&mut self) -> anyhow::Result<String> {
        match self.advance().clone() {
            Token::Str(s) | Token::Ident(s) => Ok(s),
            t => Err(anyhow::anyhow!("Expected string or identifier, found {:?}", t)),
        }
    }

    fn expect_ident_or_str(&mut self) -> anyhow::Result<String> {
        self.expect_str_or_ident()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    #[test]
    fn test_parse_queries() {
        let mut lexer = Lexer::new("current.app");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program().unwrap();

        assert_eq!(
            prog.statements[0],
            KStatement::Expr(KExpr::Current("app".into()))
        );
    }

    #[test]
    fn test_parse_adapter_statements() {
        let mut lexer = Lexer::new("adapter on os; adapter off shell; adapter list");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program().unwrap();

        assert_eq!(
            prog.statements[0],
            KStatement::Adapter {
                action: "on".into(),
                name: Some("os".into()),
            }
        );
        assert_eq!(
            prog.statements[1],
            KStatement::Adapter {
                action: "off".into(),
                name: Some("shell".into()),
            }
        );
        assert_eq!(
            prog.statements[2],
            KStatement::Adapter {
                action: "list".into(),
                name: None,
            }
        );
    }

    #[test]
    fn test_parse_unknown_query_fails_fast() {
        let mut lexer = Lexer::new("metrics");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let res = parser.parse_program();
        assert!(res.is_err(), "k metrics must return an error");
        assert!(res.unwrap_err().to_string().contains("Unknown query or command: 'metrics'"));
    }

    #[test]
    fn test_parse_help_and_summary_and_history() {
        let mut lexer = Lexer::new("help");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program().unwrap();
        assert_eq!(prog.statements[0], KStatement::Help);

        let mut lexer2 = Lexer::new("summary today");
        let tokens2 = lexer2.tokenize().unwrap();
        let mut parser2 = Parser::new(tokens2);
        let prog2 = parser2.parse_program().unwrap();
        assert_eq!(prog2.statements[0], KStatement::Summary { range: "today".into() });

        let mut lexer3 = Lexer::new("history today");
        let tokens3 = lexer3.tokenize().unwrap();
        let mut parser3 = Parser::new(tokens3);
        let prog3 = parser3.parse_program().unwrap();
        assert_eq!(prog3.statements[0], KStatement::Expr(KExpr::History("today".into())));
    }
}
