//! Integer expression evaluator for the `calc` command.
//! Supports + - * / %, parentheses, unary minus and hexadecimal (0x..) numbers.

use core::iter::Peekable;
use core::str::Chars;

/// Deepest nesting of parentheses and unary minus signs. The parser recurses once per level on
/// the small kernel stack, so a long run of `(` must be an error rather than a stack overflow.
const MAX_DEPTH: usize = 64;

pub fn evaluate(expression: &str) -> Result<i64, &'static str> {
    let mut parser = Parser { characters: expression.chars().peekable(), depth: 0 };
    let value = parser.expression()?;
    parser.skip_spaces();
    match parser.characters.peek() {
        None => Ok(value),
        Some(_) => Err("unexpected character"),
    }
}

struct Parser<'a> {
    characters: Peekable<Chars<'a>>,
    /// Current nesting of `(` and unary `-`.
    depth: usize,
}

impl Parser<'_> {
    fn skip_spaces(&mut self) {
        while self.characters.next_if(|c| c.is_whitespace()).is_some() {}
    }

    fn next_operator(&mut self, operators: &[char]) -> Option<char> {
        self.skip_spaces();
        self.characters.next_if(|c| operators.contains(c))
    }

    fn expression(&mut self) -> Result<i64, &'static str> {
        let mut value = self.term()?;
        while let Some(operator) = self.next_operator(&['+', '-']) {
            let right = self.term()?;
            value = match operator {
                '+' => value.checked_add(right),
                _ => value.checked_sub(right),
            }
            .ok_or("overflow")?;
        }
        Ok(value)
    }

    fn term(&mut self) -> Result<i64, &'static str> {
        let mut value = self.unary()?;
        while let Some(operator) = self.next_operator(&['*', '/', '%']) {
            let right = self.unary()?;
            if operator != '*' && right == 0 {
                return Err("division by zero");
            }
            value = match operator {
                '*' => value.checked_mul(right),
                '/' => value.checked_div(right),
                _ => value.checked_rem(right),
            }
            .ok_or("overflow")?;
        }
        Ok(value)
    }

    /// Runs `f` one nesting level deeper, failing beyond `MAX_DEPTH`.
    fn nested(&mut self, f: impl FnOnce(&mut Self) -> Result<i64, &'static str>) -> Result<i64, &'static str> {
        if self.depth >= MAX_DEPTH {
            return Err("expression nested too deeply");
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    fn unary(&mut self) -> Result<i64, &'static str> {
        if self.next_operator(&['-']).is_some() {
            return self.nested(|parser| parser.unary()?.checked_neg().ok_or("overflow"));
        }
        self.next_operator(&['+']);
        self.primary()
    }

    fn primary(&mut self) -> Result<i64, &'static str> {
        self.skip_spaces();
        if self.characters.next_if_eq(&'(').is_some() {
            let value = self.nested(|parser| parser.expression())?;
            self.skip_spaces();
            return match self.characters.next() {
                Some(')') => Ok(value),
                _ => Err("missing ')'"),
            };
        }
        self.number()
    }

    fn number(&mut self) -> Result<i64, &'static str> {
        let mut radix = 10;
        if self.characters.next_if_eq(&'0').is_some() {
            if self.characters.next_if(|c| *c == 'x' || *c == 'X').is_some() {
                radix = 16;
            } else if !self.characters.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Ok(0);
            }
        }

        let mut value: i64 = 0;
        let mut digits = 0;
        while let Some(digit) = self.characters.peek().and_then(|c| c.to_digit(radix)) {
            self.characters.next();
            value = value.checked_mul(radix as i64).and_then(|v| v.checked_add(digit as i64)).ok_or("overflow")?;
            digits += 1;
        }
        if digits == 0 && radix == 10 {
            return Err("expected a number");
        }
        if digits == 0 {
            return Err("expected hex digits after 0x");
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn evaluates() {
        assert_eq!(evaluate("(3 + 4) * 0x10"), Ok(112));
        assert_eq!(evaluate("-(-2) - -3"), Ok(5));
        assert_eq!(evaluate("7 % 0"), Err("division by zero"));
    }

    #[test]
    fn deep_nesting_is_an_error() {
        let nested = |depth: usize| {
            let mut text = String::new();
            (0..depth).for_each(|_| text.push('('));
            text.push('1');
            (0..depth).for_each(|_| text.push(')'));
            text
        };
        assert_eq!(evaluate(&nested(MAX_DEPTH)), Ok(1));
        assert_eq!(evaluate(&nested(MAX_DEPTH + 1)), Err("expression nested too deeply"));
        assert_eq!(evaluate(&nested(100_000)), Err("expression nested too deeply"));
        let minus: String = core::iter::repeat_n('-', 100_000).chain(core::iter::once('1')).collect();
        assert_eq!(evaluate(&minus), Err("expression nested too deeply"));
        let open: String = core::iter::repeat_n('(', 100_000).collect();
        assert_eq!(evaluate(&open), Err("expression nested too deeply"));
    }
}
