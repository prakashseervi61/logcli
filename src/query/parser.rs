use super::ast::{like_regex, literal_compare, Expr, Literal, Op, Path, SetRef};
use super::lexer::{lex, Token, TokenKind};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let tokens = lex(input).map_err(|error| ParseError {
        offset: error.offset,
        message: error.message,
    })?;
    let mut parser = Parser {
        input,
        tokens,
        position: 0,
    };
    parser.parse_query()
}

struct Parser<'a> {
    input: &'a str,
    tokens: Vec<Token>,
    position: usize,
}

enum Operand {
    Path(Path),
    Lit(Literal),
}

#[derive(Clone, Copy)]
enum OpKind {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
    NotContains,
    Regex,
    NotRegex,
    Like,
    NotLike,
}

impl OpKind {
    fn swapped(self) -> Option<OpKind> {
        match self {
            OpKind::Eq => Some(OpKind::Eq),
            OpKind::Ne => Some(OpKind::Ne),
            OpKind::Gt => Some(OpKind::Lt),
            OpKind::Ge => Some(OpKind::Le),
            OpKind::Lt => Some(OpKind::Gt),
            OpKind::Le => Some(OpKind::Ge),
            OpKind::Contains
            | OpKind::NotContains
            | OpKind::Regex
            | OpKind::NotRegex
            | OpKind::Like
            | OpKind::NotLike => None,
        }
    }

    fn negated(self) -> Option<OpKind> {
        match self {
            OpKind::Contains => Some(OpKind::NotContains),
            OpKind::Regex => Some(OpKind::NotRegex),
            OpKind::Like => Some(OpKind::NotLike),
            _ => None,
        }
    }
}

impl Parser<'_> {
    fn parse_query(&mut self) -> Result<Expr, ParseError> {
        if self.at_eof() {
            return Err(self.error_at(0, "empty query"));
        }

        let expr = self.parse_or()?;

        if !self.at_eof() {
            let token = self.peek().clone();
            return Err(self.unexpected(&token));
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_and()?;

        loop {
            if !self.peek_word("or") && !matches!(self.peek().kind, TokenKind::OrOr) {
                break;
            }

            let token = self.next();
            if self.at_eof() {
                return Err(self.error_at(
                    token.end,
                    format!("expected an expression after '{}'", self.token_text(&token)),
                ));
            }

            let right = self.parse_and()?;
            expr = Expr::Or(Box::new(expr), Box::new(right));
        }

        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_not()?;

        loop {
            if !self.peek_word("and") && !matches!(self.peek().kind, TokenKind::AndAnd) {
                break;
            }

            let token = self.next();
            if self.at_eof() {
                return Err(self.error_at(
                    token.end,
                    format!("expected an expression after '{}'", self.token_text(&token)),
                ));
            }

            let right = self.parse_not()?;
            expr = Expr::And(Box::new(expr), Box::new(right));
        }

        Ok(expr)
    }

    fn parse_not(&mut self) -> Result<Expr, ParseError> {
        if self.peek_word("not") || matches!(self.peek().kind, TokenKind::Bang) {
            let token = self.next();
            if self.at_eof() {
                return Err(self.error_at(
                    token.end,
                    format!("expected an expression after '{}'", self.token_text(&token)),
                ));
            }

            let inner = self.parse_not()?;
            return Ok(Expr::Not(Box::new(inner)));
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().clone();

        match &token.kind {
            TokenKind::LParen => {
                self.next();
                if self.at_eof() {
                    let column = self.column(token.start);
                    return Err(self.error_at(
                        self.peek().start,
                        format!("expected ')' to close '(' at column {column}"),
                    ));
                }
                let expr = self.parse_or()?;
                if matches!(self.peek().kind, TokenKind::RParen) {
                    self.next();
                    Ok(expr)
                } else {
                    let column = self.column(token.start);
                    Err(self.error_at(
                        self.peek().start,
                        format!("expected ')' to close '(' at column {column}"),
                    ))
                }
            }
            TokenKind::Word(word)
                if word.eq_ignore_ascii_case("exists") || word.eq_ignore_ascii_case("exist") =>
            {
                self.parse_exists()
            }
            TokenKind::Word(word) if word == "true" => {
                self.next();
                Ok(Expr::Bool(true))
            }
            TokenKind::Word(word) if word == "false" => {
                self.next();
                Ok(Expr::Bool(false))
            }
            TokenKind::Eof => Err(self.error_at(token.start, "unexpected end of query")),
            _ => self.parse_comparison(),
        }
    }

    fn parse_exists(&mut self) -> Result<Expr, ParseError> {
        let keyword = self.next();
        let keyword_text = self.token_text(&keyword);

        if !matches!(self.peek().kind, TokenKind::LParen) {
            return Err(self.error_at(
                self.peek().start,
                format!("expected '(' after '{keyword_text}'"),
            ));
        }

        let open = self.next();
        let argument = self.next();

        let path = match &argument.kind {
            TokenKind::Word(word) => Path::parse(word, false, false),
            TokenKind::Str(text) => {
                let absent = self.eat_adjacent_question(argument.end);
                Path::parse(text, true, absent)
            }
            _ => return Err(self.error_at(argument.start, "exists() takes exactly one field path")),
        }
        .map_err(|message| self.error_at(argument.start, message))?;

        if !matches!(self.peek().kind, TokenKind::RParen) {
            let column = self.column(open.start);
            return Err(self.error_at(
                self.peek().start,
                format!("expected ')' to close '(' at column {column}"),
            ));
        }
        self.next();

        Ok(Expr::Exists { path })
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let left_token = self.peek().clone();
        let left = self.parse_left_operand()?;

        if self.peek_word("in") || (self.peek_word("not") && self.word_at(1, "in")) {
            let path = match left {
                Operand::Path(path) => path,
                Operand::Lit(_) => {
                    return Err(
                        self.error_at(left_token.start, "'in' requires a field path on the left")
                    )
                }
            };

            let negated = if self.peek_word("not") {
                self.next();
                self.next();
                true
            } else {
                self.next();
                false
            };

            let set = self.parse_set()?;
            return Ok(Expr::In { path, set, negated });
        }

        let Some((kind, _, op_text)) = self.parse_operator()? else {
            return Err(match &left {
                Operand::Path(path) => self.error_at(
                    left_token.start,
                    format!("expected a comparison, found field path '{}'", path.raw),
                ),
                Operand::Lit(_) => self.error_at(left_token.start, "expected a comparison"),
            });
        };

        let left_is_literal = matches!(left, Operand::Lit(_));
        let right_token = self.peek().clone();
        let right = self.parse_right_operand(left_is_literal, &op_text)?;

        match (left, right) {
            (Operand::Path(path), Operand::Lit(value)) => {
                let op = build_op(kind, &value, right_token.start)?;
                Ok(Expr::Compare { path, op, value })
            }
            (Operand::Lit(value), Operand::Path(path)) => {
                let Some(swapped) = kind.swapped() else {
                    return Err(self.error_at(
                        left_token.start,
                        format!("left operand of '{op_text}' must be a field path"),
                    ));
                };
                let op = build_op(swapped, &value, left_token.start)?;
                Ok(Expr::Compare { path, op, value })
            }
            (Operand::Lit(left_value), Operand::Lit(right_value)) => {
                let op = build_op(kind, &right_value, right_token.start)?;
                Ok(Expr::Bool(literal_compare(&left_value, &right_value, &op)))
            }
            (Operand::Path(_), Operand::Path(_)) => {
                Err(self.error_at(right_token.start, "comparisons require a literal value"))
            }
        }
    }

    fn parse_left_operand(&mut self) -> Result<Operand, ParseError> {
        let token = self.next();

        match token.kind {
            TokenKind::Str(text) => {
                let absent = self.eat_adjacent_question(token.end);
                let path = Path::parse(&text, true, absent)
                    .map_err(|message| self.error_at(token.start, message))?;
                Ok(Operand::Path(path))
            }
            TokenKind::Num(number) => Ok(Operand::Lit(Literal::Num(number))),
            TokenKind::Word(word) => {
                let literal = match word.as_str() {
                    "true" => Some(Literal::Bool(true)),
                    "false" => Some(Literal::Bool(false)),
                    "null" => Some(Literal::Null),
                    _ => None,
                };

                if let Some(value) = literal {
                    return Ok(Operand::Lit(value));
                }

                let path = Path::parse(&word, false, false)
                    .map_err(|message| self.error_at(token.start, message))?;
                Ok(Operand::Path(path))
            }
            _ => Err(self.error_at(token.start, "expected a comparison")),
        }
    }

    fn parse_right_operand(
        &mut self,
        left_is_literal: bool,
        op_text: &str,
    ) -> Result<Operand, ParseError> {
        let token = self.next();

        match token.kind {
            TokenKind::Str(text) => Ok(Operand::Lit(Literal::Str(text))),
            TokenKind::Num(number) => Ok(Operand::Lit(Literal::Num(number))),
            TokenKind::Word(word) => match word.as_str() {
                "true" => Ok(Operand::Lit(Literal::Bool(true))),
                "false" => Ok(Operand::Lit(Literal::Bool(false))),
                "null" => Ok(Operand::Lit(Literal::Null)),
                _ if left_is_literal => {
                    let path = Path::parse(&word, false, false)
                        .map_err(|message| self.error_at(token.start, message))?;
                    Ok(Operand::Path(path))
                }
                _ => Ok(Operand::Lit(Literal::Str(word))),
            },
            _ => Err(self.error_at(token.start, format!("expected a value after '{op_text}'"))),
        }
    }

    fn parse_operator(&mut self) -> Result<Option<(OpKind, usize, String)>, ParseError> {
        let token = self.peek().clone();

        match &token.kind {
            TokenKind::Sym(symbol) => {
                let kind = kind_from_symbol(symbol).expect("lexer only emits known operators");
                self.next();
                Ok(Some((kind, token.start, (*symbol).to_string())))
            }
            TokenKind::Word(word) => {
                let lower = word.to_ascii_lowercase();

                if lower == "not" {
                    let next = self.peek_at(1);
                    let next_word = match &next.kind {
                        TokenKind::Word(next_word) => next_word.to_ascii_lowercase(),
                        _ => String::new(),
                    };

                    if matches!(next_word.as_str(), "contain" | "match" | "like") {
                        let Some(negated) = kind_from_word(&next_word).and_then(OpKind::negated)
                        else {
                            return Err(self.error_at(token.start, "invalid operator"));
                        };
                        self.next();
                        self.next();
                        return Ok(Some((negated, token.start, format!("not {next_word}"))));
                    }

                    return Err(
                        self.error_at(token.start, format!("expected an operator, found '{word}'"))
                    );
                }

                if let Some(kind) = kind_from_word(&lower) {
                    self.next();
                    Ok(Some((kind, token.start, lower)))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn parse_set(&mut self) -> Result<SetRef, ParseError> {
        let token = self.peek().clone();

        match &token.kind {
            TokenKind::LParen => self.parse_inline_set(),
            TokenKind::Word(word) if word.starts_with('@') => {
                let word = word.clone();
                self.parse_file_set(token, &word)
            }
            _ => Err(self.error_at(token.start, "expected '(' or '@file' after 'in'")),
        }
    }

    fn parse_inline_set(&mut self) -> Result<SetRef, ParseError> {
        self.next();

        if matches!(self.peek().kind, TokenKind::RParen) {
            return Err(self.error_at(
                self.peek().start,
                "expected at least one value in value list",
            ));
        }

        let mut values = Vec::new();

        loop {
            let token = self.peek().clone();
            let value = match token.kind {
                TokenKind::Str(text) => Literal::Str(text),
                TokenKind::Num(number) => Literal::Num(number),
                TokenKind::Word(word) => word_literal(word),
                _ => {
                    return Err(self.error_at(token.start, "expected a value in value list"));
                }
            };
            self.next();
            values.push(value);

            match self.peek().kind {
                TokenKind::Comma => {
                    self.next();
                }
                TokenKind::RParen => {
                    self.next();
                    break;
                }
                _ => {
                    return Err(
                        self.error_at(self.peek().start, "expected ',' or ')' in value list")
                    );
                }
            }
        }

        Ok(SetRef::Inline(values))
    }

    fn parse_file_set(&mut self, token: Token, word: &str) -> Result<SetRef, ParseError> {
        self.next();

        let rest = &word[1..];
        let filename = if rest.is_empty() {
            let next = self.next();
            match next.kind {
                TokenKind::Str(text) => text,
                TokenKind::Word(word) => word,
                _ => {
                    return Err(self.error_at(next.start, "expected a filename after '@'"));
                }
            }
        } else {
            rest.to_string()
        };

        if filename == "-" {
            Ok(SetRef::Stdin {
                offset: token.start,
                query: self.input.to_string(),
            })
        } else {
            Ok(SetRef::File { path: filename })
        }
    }

    fn eat_adjacent_question(&mut self, end: usize) -> bool {
        if self.peek().start == end && self.peek_word("?") {
            self.next();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn peek_at(&self, offset: usize) -> &Token {
        &self.tokens[(self.position + offset).min(self.tokens.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let token = self.peek().clone();
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn peek_word(&self, keyword: &str) -> bool {
        self.word_at(0, keyword)
    }

    fn word_at(&self, offset: usize, keyword: &str) -> bool {
        matches!(
            &self.peek_at(offset).kind,
            TokenKind::Word(word) if word.eq_ignore_ascii_case(keyword)
        )
    }

    fn token_text(&self, token: &Token) -> String {
        match token.kind {
            TokenKind::Eof => "end of query".to_string(),
            _ => self.input[token.start..token.end].to_string(),
        }
    }

    fn unexpected(&self, token: &Token) -> ParseError {
        self.error_at(
            token.start,
            format!("unexpected token '{}'", self.token_text(token)),
        )
    }

    fn column(&self, offset: usize) -> usize {
        self.input[..offset.min(self.input.len())].chars().count() + 1
    }

    fn error_at(&self, offset: usize, message: impl Into<String>) -> ParseError {
        ParseError {
            offset,
            message: message.into(),
        }
    }
}

fn kind_from_symbol(symbol: &str) -> Option<OpKind> {
    match symbol {
        "=" | "==" => Some(OpKind::Eq),
        "!=" => Some(OpKind::Ne),
        ">" => Some(OpKind::Gt),
        ">=" => Some(OpKind::Ge),
        "<" => Some(OpKind::Lt),
        "<=" => Some(OpKind::Le),
        "~=" => Some(OpKind::Contains),
        "!~=" => Some(OpKind::NotContains),
        "~~=" => Some(OpKind::Regex),
        "!~~=" => Some(OpKind::NotRegex),
        _ => None,
    }
}

fn kind_from_word(word: &str) -> Option<OpKind> {
    match word {
        "eq" => Some(OpKind::Eq),
        "ne" => Some(OpKind::Ne),
        "gt" => Some(OpKind::Gt),
        "ge" => Some(OpKind::Ge),
        "lt" => Some(OpKind::Lt),
        "le" => Some(OpKind::Le),
        "contain" => Some(OpKind::Contains),
        "match" => Some(OpKind::Regex),
        "like" => Some(OpKind::Like),
        _ => None,
    }
}

fn word_literal(word: String) -> Literal {
    match word.as_str() {
        "true" => Literal::Bool(true),
        "false" => Literal::Bool(false),
        "null" => Literal::Null,
        _ => Literal::Str(word),
    }
}

fn build_op(kind: OpKind, value: &Literal, offset: usize) -> Result<Op, ParseError> {
    let regex = |pattern: &str| {
        Regex::new(pattern).map_err(|error| ParseError {
            offset,
            message: format!("invalid regex: {error}"),
        })
    };

    match kind {
        OpKind::Eq => Ok(Op::Eq),
        OpKind::Ne => Ok(Op::Ne),
        OpKind::Gt => Ok(Op::Gt),
        OpKind::Ge => Ok(Op::Ge),
        OpKind::Lt => Ok(Op::Lt),
        OpKind::Le => Ok(Op::Le),
        OpKind::Contains => Ok(Op::Contains),
        OpKind::NotContains => Ok(Op::NotContains),
        OpKind::Regex => Ok(Op::Regex(regex(&value.text())?)),
        OpKind::NotRegex => Ok(Op::NotRegex(regex(&value.text())?)),
        OpKind::Like => Ok(Op::Like(like_regex(&value.text()))),
        OpKind::NotLike => Ok(Op::NotLike(like_regex(&value.text()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(input: &str) -> ParseError {
        parse(input).unwrap_err()
    }

    #[test]
    fn parses_precedence() {
        assert!(matches!(parse("a = 1 or b = 2").unwrap(), Expr::Or(_, _)));
        assert!(matches!(parse("a = 1 and b = 2").unwrap(), Expr::And(_, _)));
        assert!(matches!(
            parse("not a = 1").unwrap(),
            Expr::Not(inner) if matches!(*inner, Expr::Compare { .. })
        ));
    }

    #[test]
    fn parses_in_sets_and_files() {
        assert!(matches!(
            parse("status in (200, 201)").unwrap(),
            Expr::In {
                set: SetRef::Inline(_),
                negated: false,
                ..
            }
        ));
        assert!(matches!(
            parse("user not in @blocked.txt").unwrap(),
            Expr::In {
                set: SetRef::File { .. },
                negated: true,
                ..
            }
        ));
        assert!(matches!(
            parse("ip in @-").unwrap(),
            Expr::In {
                set: SetRef::Stdin { .. },
                ..
            }
        ));
    }

    #[test]
    fn reports_missing_value_at_eof() {
        let error = error("status =");
        assert_eq!(error.offset, 8);
        assert_eq!(error.message, "expected a value after '='");
    }

    #[test]
    fn reports_bare_field_paths() {
        let error = error("error");
        assert_eq!(error.offset, 0);
        assert_eq!(
            error.message,
            "expected a comparison, found field path 'error'"
        );
    }

    #[test]
    fn reports_unbalanced_parentheses() {
        let error = error("level > info and (");
        assert_eq!(error.message, "expected ')' to close '(' at column 18");
    }

    #[test]
    fn reports_invalid_regex() {
        let error = error(r#"msg ~~= "(unclosed""#);
        assert!(error.message.starts_with("invalid regex:"));
    }
}
