//! Splits a command line into words. Pure functions, so they can be tested on the host.
//!
//! * Words are separated by spaces and tabs.
//! * `"..."` and `'...'` quote spaces and special characters; inside double quotes `\"` and `\\`
//!   are escapes, single quotes are literal. Outside quotes `\x` is a literal `x`.
//! * An unquoted `>` or `>>` redirects the output to the file named by the next word; it may be
//!   written with or without spaces (`echo hi>a.txt`).

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub path: String,
    pub append: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandLine {
    pub words: Vec<String>,
    pub redirect: Option<Redirect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnterminatedQuote,
    MissingRedirectTarget,
    MultipleRedirects,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            ParseError::UnterminatedQuote => "unterminated quote",
            ParseError::MissingRedirectTarget => "missing file name after '>'",
            ParseError::MultipleRedirects => "only one output redirection is supported",
        })
    }
}

enum Token {
    Word(String),
    Redirect { append: bool },
}

fn tokenize(line: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    let mut word = String::new();
    let mut in_word = false;

    while let Some(character) = chars.next() {
        match character {
            ' ' | '\t' => {
                if in_word {
                    tokens.push(Token::Word(core::mem::take(&mut word)));
                    in_word = false;
                }
            }
            '>' => {
                if in_word {
                    tokens.push(Token::Word(core::mem::take(&mut word)));
                    in_word = false;
                }
                let append = chars.peek() == Some(&'>');
                if append {
                    chars.next();
                }
                tokens.push(Token::Redirect { append });
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        None => return Err(ParseError::UnterminatedQuote),
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"') | Some('\\')) => word.push(chars.next().unwrap()),
                        Some(other) => word.push(other),
                    }
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        None => return Err(ParseError::UnterminatedQuote),
                        Some('\'') => break,
                        Some(other) => word.push(other),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            other => {
                in_word = true;
                word.push(other);
            }
        }
    }
    if in_word {
        tokens.push(Token::Word(word));
    }
    Ok(tokens)
}

pub fn parse(line: &str) -> Result<CommandLine, ParseError> {
    let mut result = CommandLine::default();
    let mut tokens = tokenize(line)?.into_iter();
    while let Some(token) = tokens.next() {
        match token {
            Token::Word(word) => result.words.push(word),
            Token::Redirect { append } => {
                if result.redirect.is_some() {
                    return Err(ParseError::MultipleRedirects);
                }
                match tokens.next() {
                    Some(Token::Word(path)) if !path.is_empty() => result.redirect = Some(Redirect { path, append }),
                    _ => return Err(ParseError::MissingRedirectTarget),
                }
            }
        }
    }
    Ok(result)
}

/// The word the cursor is in, for tab completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentWord {
    /// 0 for the command name.
    pub index: usize,
    /// Character offset where the word starts (including an opening quote).
    pub start: usize,
    /// The word's text so far with quotes and escapes removed.
    pub text: String,
    /// The quote the word is currently inside of, if any.
    pub open_quote: Option<char>,
    /// The word follows `>` or `>>`.
    pub is_redirect_target: bool,
}

/// Finds the word being typed at the end of `before_cursor`.
pub fn current_word(before_cursor: &str) -> CurrentWord {
    let mut index = 0;
    let mut words_seen = 0;
    let mut start = 0;
    let mut text = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut after_redirect = false;
    let mut is_redirect_target = false;
    let chars: Vec<char> = before_cursor.chars().collect();
    let mut position = 0;

    while position < chars.len() {
        let character = chars[position];
        match (quote, character) {
            (Some(open), c) if c == open => quote = None,
            (Some('"'), '\\') if matches!(chars.get(position + 1), Some('"') | Some('\\')) => {
                position += 1;
                text.push(chars[position]);
            }
            (Some(_), c) => text.push(c),
            (None, ' ' | '\t' | '>') => {
                if in_word {
                    in_word = false;
                    if !after_redirect {
                        words_seen += 1;
                    }
                    after_redirect = false;
                }
                if character == '>' {
                    after_redirect = true;
                }
            }
            (None, c) => {
                if !in_word {
                    in_word = true;
                    start = position;
                    text.clear();
                    index = words_seen;
                    is_redirect_target = after_redirect;
                }
                match c {
                    '"' | '\'' => quote = Some(c),
                    '\\' => {
                        if let Some(&next) = chars.get(position + 1) {
                            position += 1;
                            text.push(next);
                        }
                    }
                    _ => text.push(c),
                }
            }
        }
        position += 1;
    }

    if !in_word {
        // The cursor is after a separator: a new, empty word starts here.
        return CurrentWord {
            index: words_seen,
            start: chars.len(),
            text: String::new(),
            open_quote: None,
            is_redirect_target: after_redirect,
        };
    }
    CurrentWord { index, start, text, open_quote: quote, is_redirect_target }
}

/// Whether `word` must be quoted to reach a command as one argument.
pub fn needs_quotes(word: &str) -> bool {
    word.is_empty() || word.chars().any(|c| matches!(c, ' ' | '\t' | '"' | '\'' | '\\' | '>'))
}

/// `word` in double quotes with `"` and `\` escaped; `close` adds the closing quote.
pub fn double_quote(word: &str, close: bool) -> String {
    let mut quoted = String::from("\"");
    for c in word.chars() {
        if c == '"' || c == '\\' {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    if close {
        quoted.push('"');
    }
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn words(line: &str) -> Vec<String> {
        parse(line).unwrap().words
    }

    #[test]
    fn splits_words() {
        assert_eq!(words("  ls   -l  /docs "), vec!["ls", "-l", "/docs"]);
        assert_eq!(words(""), Vec::<String>::new());
        assert_eq!(words("write \"hello world.txt\" some text"), vec!["write", "hello world.txt", "some", "text"]);
        assert_eq!(words("echo 'a  \"b\"'"), vec!["echo", "a  \"b\""]);
        assert_eq!(words("echo \"a \\\"q\\\" \\\\ \\n\""), vec!["echo", "a \"q\" \\ \\n"]);
        assert_eq!(words("echo a\\ b"), vec!["echo", "a b"]);
        assert_eq!(words("echo \"\""), vec!["echo", ""]);
        assert_eq!(words("echo ab\"cd ef\"gh"), vec!["echo", "abcd efgh"]);
        assert_eq!(words("echo \">\""), vec!["echo", ">"]);
    }

    #[test]
    fn redirects() {
        let line = parse("echo hi > out.txt").unwrap();
        assert_eq!(line.words, vec!["echo", "hi"]);
        assert_eq!(line.redirect, Some(Redirect { path: "out.txt".into(), append: false }));
        let line = parse("echo hi>>\"my log.txt\"").unwrap();
        assert_eq!(line.words, vec!["echo", "hi"]);
        assert_eq!(line.redirect, Some(Redirect { path: "my log.txt".into(), append: true }));
        let line = parse("ls >a extra").unwrap();
        assert_eq!(line.words, vec!["ls", "extra"]);
        assert_eq!(parse("echo >"), Err(ParseError::MissingRedirectTarget));
        assert_eq!(parse("echo > > a"), Err(ParseError::MissingRedirectTarget));
        assert_eq!(parse("echo > a > b"), Err(ParseError::MultipleRedirects));
        assert_eq!(parse("echo \"abc"), Err(ParseError::UnterminatedQuote));
        assert_eq!(parse("echo 'abc"), Err(ParseError::UnterminatedQuote));
    }

    #[test]
    fn finds_current_word() {
        let word = current_word("");
        assert_eq!((word.index, word.start, word.text.as_str()), (0, 0, ""));
        let word = current_word("ls");
        assert_eq!((word.index, word.start, word.text.as_str()), (0, 0, "ls"));
        let word = current_word("cat ");
        assert_eq!((word.index, word.start, word.text.as_str()), (1, 4, ""));
        let word = current_word("cat do");
        assert_eq!((word.index, word.start, word.text.as_str()), (1, 4, "do"));
        let word = current_word("cat \"my fi");
        assert_eq!((word.index, word.start, word.text.as_str(), word.open_quote), (1, 4, "my fi", Some('"')));
        let word = current_word("cp a \"b c\"/d");
        assert_eq!((word.index, word.start, word.text.as_str(), word.open_quote), (2, 5, "b c/d", None));
        let word = current_word("echo hi > fi");
        assert_eq!((word.index, word.text.as_str(), word.is_redirect_target), (2, "fi", true));
        let word = current_word("echo hi >fi");
        assert_eq!((word.text.as_str(), word.is_redirect_target), ("fi", true));
        let word = current_word("echo hi > ");
        assert!(word.is_redirect_target);
        let word = current_word("echo a\\ b");
        assert_eq!((word.index, word.start, word.text.as_str()), (1, 5, "a b"));
    }

    #[test]
    fn quotes() {
        assert_eq!(double_quote("plain.txt", true), "\"plain.txt\"");
        assert!(needs_quotes("my file.txt") && !needs_quotes("plain.txt"));
        assert_eq!(double_quote("a\"b", false), "\"a\\\"b");
        for name in ["my file", "a\"b", "c\\d", "x>y", "it's"] {
            assert_eq!(words(&alloc::format!("cmd {}", double_quote(name, true))), vec!["cmd", name]);
        }
    }
}
