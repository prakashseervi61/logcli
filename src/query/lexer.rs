#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Word(String),
    Str(String),
    Num(f64),
    LParen,
    RParen,
    Comma,
    Sym(&'static str),
    Bang,
    AndAnd,
    OrOr,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub offset: usize,
    pub message: String,
}

pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let (offset, character) = chars[index];

        if character.is_whitespace() {
            index += 1;
            continue;
        }

        match character {
            '(' => {
                tokens.push(token(TokenKind::LParen, offset, offset + 1));
                index += 1;
            }
            ')' => {
                tokens.push(token(TokenKind::RParen, offset, offset + 1));
                index += 1;
            }
            ',' => {
                tokens.push(token(TokenKind::Comma, offset, offset + 1));
                index += 1;
            }
            '"' => {
                let (kind, next) = lex_string(input, &chars, index)?;
                tokens.push(token(kind, offset, chars[next - 1].0 + 1));
                index = next;
            }
            '&' => {
                if input[offset..].starts_with("&&") {
                    tokens.push(token(TokenKind::AndAnd, offset, offset + 2));
                    index += 2;
                } else {
                    return Err(unexpected(offset, "&"));
                }
            }
            '|' => {
                if input[offset..].starts_with("||") {
                    tokens.push(token(TokenKind::OrOr, offset, offset + 2));
                    index += 2;
                } else {
                    return Err(unexpected(offset, "|"));
                }
            }
            '!' => {
                let rest = &input[offset..];
                if rest.starts_with("!~~=") {
                    tokens.push(token(TokenKind::Sym("!~~="), offset, offset + 4));
                    index += 4;
                } else if rest.starts_with("!~=") {
                    tokens.push(token(TokenKind::Sym("!~="), offset, offset + 3));
                    index += 3;
                } else if rest.starts_with("!=") {
                    tokens.push(token(TokenKind::Sym("!="), offset, offset + 2));
                    index += 2;
                } else {
                    tokens.push(token(TokenKind::Bang, offset, offset + 1));
                    index += 1;
                }
            }
            '~' => {
                let rest = &input[offset..];
                if rest.starts_with("~~=") {
                    tokens.push(token(TokenKind::Sym("~~="), offset, offset + 3));
                    index += 3;
                } else if rest.starts_with("~=") {
                    tokens.push(token(TokenKind::Sym("~="), offset, offset + 2));
                    index += 2;
                } else {
                    return Err(unexpected(offset, "~"));
                }
            }
            '=' => {
                let rest = &input[offset..];
                if rest.starts_with("=>") || rest.starts_with("=<") {
                    let symbol = &rest[..2];
                    return Err(LexError {
                        offset,
                        message: format!("unexpected token '{symbol}'{}", suggest_operator(symbol)),
                    });
                }
                if rest.starts_with("==") {
                    tokens.push(token(TokenKind::Sym("=="), offset, offset + 2));
                    index += 2;
                } else {
                    tokens.push(token(TokenKind::Sym("="), offset, offset + 1));
                    index += 1;
                }
            }
            '>' => {
                if input[offset..].starts_with(">=") {
                    tokens.push(token(TokenKind::Sym(">="), offset, offset + 2));
                    index += 2;
                } else {
                    tokens.push(token(TokenKind::Sym(">"), offset, offset + 1));
                    index += 1;
                }
            }
            '<' => {
                if input[offset..].starts_with("<=") {
                    tokens.push(token(TokenKind::Sym("<="), offset, offset + 2));
                    index += 2;
                } else {
                    tokens.push(token(TokenKind::Sym("<"), offset, offset + 1));
                    index += 1;
                }
            }
            _ if is_word_char(character) => {
                while index < chars.len() && is_word_char(chars[index].1) {
                    index += 1;
                }
                let end = chars[index - 1].0 + chars[index - 1].1.len_utf8();
                let text = &input[offset..end];
                tokens.push(token(word_kind(text), offset, end));
            }
            _ => return Err(unexpected(offset, &character.to_string())),
        }
    }

    tokens.push(token(TokenKind::Eof, input.len(), input.len()));
    Ok(tokens)
}

fn token(kind: TokenKind, start: usize, end: usize) -> Token {
    Token { kind, start, end }
}

fn word_kind(text: &str) -> TokenKind {
    if let Ok(number) = text.parse::<f64>() {
        if number.is_finite() {
            return TokenKind::Num(number);
        }
    }
    TokenKind::Word(text.to_string())
}

fn is_word_char(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(
            character,
            '_' | '@' | '.' | '-' | '/' | ':' | '?' | '*' | '[' | ']'
        )
}

fn lex_string(
    input: &str,
    chars: &[(usize, char)],
    start_index: usize,
) -> Result<(TokenKind, usize), LexError> {
    let start_offset = chars[start_index].0;
    let mut escaped = false;
    let mut index = start_index + 1;

    while index < chars.len() {
        let character = chars[index].1;
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            let end_offset = chars[index].0 + 1;
            let raw = &input[start_offset..end_offset];
            let decoded: String = serde_json::from_str(raw).map_err(|error| LexError {
                offset: start_offset,
                message: format!("invalid string literal: {error}"),
            })?;
            return Ok((TokenKind::Str(decoded), index + 1));
        }
        index += 1;
    }

    Err(LexError {
        offset: start_offset,
        message: "unterminated string literal".to_string(),
    })
}

fn unexpected(offset: usize, symbol: &str) -> LexError {
    LexError {
        offset,
        message: format!("unexpected token '{symbol}'"),
    }
}

fn suggest_operator(symbol: &str) -> String {
    const OPERATORS: &[&str] = &[
        "=", "==", "!=", ">", ">=", "<", "<=", "~=", "!~=", "~~=", "!~~=", "eq", "ne", "gt", "ge",
        "lt", "le", "contain", "match", "like",
    ];

    let Some(best) = OPERATORS
        .iter()
        .min_by_key(|candidate| edit_distance(symbol, candidate))
    else {
        return String::new();
    };

    format!(" (did you mean '{best}'?)")
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();

    for (row, left_char) in left.iter().enumerate() {
        let mut current = vec![row + 1];
        for (column, right_char) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(left_char != right_char);
            let insertion = current[column] + 1;
            let deletion = previous[column + 1] + 1;
            current.push(substitution.min(insertion).min(deletion));
        }
        previous = current;
    }

    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        lex(input)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_operators_longest_first() {
        assert_eq!(
            kinds("a !~~= b"),
            vec![
                TokenKind::Word("a".into()),
                TokenKind::Sym("!~~="),
                TokenKind::Word("b".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn keeps_brackets_and_question_marks_in_words() {
        assert_eq!(
            kinds("span.[].name? like /api/*"),
            vec![
                TokenKind::Word("span.[].name?".into()),
                TokenKind::Word("like".into()),
                TokenKind::Word("/api/*".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn decodes_json_strings() {
        assert_eq!(
            kinds(r#""hello\nworld""#),
            vec![TokenKind::Str("hello\nworld".into()), TokenKind::Eof]
        );
    }

    #[test]
    fn reports_unterminated_strings() {
        let error = lex(r#""oops"#).unwrap_err();
        assert_eq!(error.offset, 0);
        assert_eq!(error.message, "unterminated string literal");
    }
}
