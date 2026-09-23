//! Integer expression evaluator for the `calc` command.
//! Supports + - * / %, parentheses, unary minus and hexadecimal (0x..) numbers.

use core::iter::Peekable;
use core::str::Chars;

pub fn evaluate(expression: &str) -> Result<i64, &'static str> {
    let mut parser = Parser { characters: expression.chars().peekable() };
    let value = parser.expression()?;
    parser.skip_spaces();
    match parser.characters.peek() {
        None => Ok(value),
        Some(_) => Err("unexpected character"),
    }
}

struct Parser<'a> {
    characters: Peekable<Chars<'a>>,
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

    fn unary(&mut self) -> Result<i64, &'static str> {
        if self.next_operator(&['-']).is_some() {
            return self.unary()?.checked_neg().ok_or("overflow");
        }
        self.next_operator(&['+']);
        self.primary()
    }

    fn primary(&mut self) -> Result<i64, &'static str> {
        self.skip_spaces();
        if self.characters.next_if_eq(&'(').is_some() {
            let value = self.expression()?;
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
