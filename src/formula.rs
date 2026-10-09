//! A small evaluator for the subset of ShapeSheet formulas found in master
//! geometry, e.g. `Width*0.5`, `Height-0.25 in`, `MIN(Width,Height)/2`.
//!
//! Anything it does not understand (cell references to other sections,
//! THEMEVAL(), ...) evaluates to `None`, and callers fall back to the cached value.

use std::f64::consts::PI;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(src: &str) -> Option<Vec<Tok>> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                // Exponent, e.g. 1.5E-3
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let mut j = i + 1;
                    if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                        j += 1;
                    }
                    if j < chars.len() && chars[j].is_ascii_digit() {
                        i = j;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                let text: String = chars[start..i].iter().collect();
                toks.push(Tok::Num(text.parse().ok()?));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || "_.!".contains(chars[i])) {
                    i += 1;
                }
                toks.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            '"' => {
                toks.push(Tok::Ident("in".into()));
                i += 1;
            }
            '+' | '-' | '*' | '/' | '^' | '%' => {
                toks.push(Tok::Op(c));
                i += 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            ',' | ';' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            _ => return None,
        }
    }
    Some(toks)
}

/// Conversion factor to internal units (inches / radians) for a unit suffix.
fn unit_factor(unit: &str) -> Option<f64> {
    Some(match unit.to_ascii_lowercase().as_str() {
        "in" | "in." | "inch" | "inches" | "dl" | "il" => 1.0,
        "mm" => 1.0 / 25.4,
        "cm" => 1.0 / 2.54,
        "m" => 39.370_078_740_157_48,
        "pt" => 1.0 / 72.0,
        "p" => 1.0 / 6.0,
        "ft" => 12.0,
        "deg" => PI / 180.0,
        "rad" => 1.0,
        _ => return None,
    })
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    env: &'a dyn Fn(&str) -> Option<f64>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.term()?;
            v = if op == '+' { v + rhs } else { v - rhs };
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while let Some(Tok::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.unary()?;
            v = if op == '*' { v * rhs } else { v / rhs };
        }
        Some(v)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.pos += 1;
                Some(-self.unary()?)
            }
            Some(Tok::Op('+')) => {
                self.pos += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let mut base = self.postfix()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.pos += 1;
            base = base.powf(self.unary()?);
        }
        Some(base)
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut v = self.primary()?;
        while let Some(Tok::Op('%')) = self.peek() {
            self.pos += 1;
            v /= 100.0;
        }
        Some(v)
    }

    fn primary(&mut self) -> Option<f64> {
        match self.next()? {
            Tok::Num(n) => {
                if let Some(Tok::Ident(unit)) = self.peek()
                    && let Some(f) = unit_factor(unit) {
                        self.pos += 1;
                        return Some(n * f);
                    }
                Some(n)
            }
            Tok::LParen => {
                let v = self.expr()?;
                (self.next()? == Tok::RParen).then_some(v)
            }
            Tok::Ident(name) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if self.peek() != Some(&Tok::RParen) {
                        loop {
                            args.push(self.expr()?);
                            match self.next()? {
                                Tok::Comma => continue,
                                Tok::RParen => break,
                                _ => return None,
                            }
                        }
                    } else {
                        self.pos += 1;
                    }
                    call(&name, &args)
                } else {
                    (self.env)(&name)
                }
            }
            _ => None,
        }
    }
}

fn call(name: &str, args: &[f64]) -> Option<f64> {
    let a = |i: usize| args.get(i).copied();
    Some(match name.to_ascii_uppercase().as_str() {
        "GUARD" | "INH" => a(0)?,
        "MIN" => args.iter().copied().reduce(f64::min)?,
        "MAX" => args.iter().copied().reduce(f64::max)?,
        "ABS" => a(0)?.abs(),
        "SQRT" => a(0)?.sqrt(),
        "SIN" => a(0)?.sin(),
        "COS" => a(0)?.cos(),
        "TAN" => a(0)?.tan(),
        "ATAN2" => a(0)?.atan2(a(1)?),
        "PI" => PI,
        "INT" => a(0)?.floor(),
        "ROUND" => {
            let p = 10f64.powi(a(1).unwrap_or(0.0) as i32);
            (a(0)? * p).round() / p
        }
        "IF" => {
            if a(0)? != 0.0 {
                a(1)?
            } else {
                a(2)?
            }
        }
        _ => return None,
    })
}

/// Evaluate `src`, resolving identifiers such as `Width` through `env`.
pub fn eval(src: &str, env: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let src = src.trim().trim_start_matches('=');
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return None;
    }
    let mut p = Parser { toks, pos: 0, env };
    let v = p.expr()?;
    (p.pos == p.toks.len() && v.is_finite()).then_some(v)
}

/// Pull the numeric arguments out of a call such as `POLYLINE(0, 0, 0.5, 1)`
/// or `NURBS(1, 3, 0, 0, ...)`. Arguments that are themselves expressions are
/// evaluated with `env`.
pub fn call_args(src: &str, func: &str, env: &dyn Fn(&str) -> Option<f64>) -> Option<Vec<f64>> {
    let src = src.trim().trim_start_matches('=');
    let open = src.find('(')?;
    if !src[..open].trim().eq_ignore_ascii_case(func) {
        return None;
    }
    let inner = src[open + 1..].trim_end().strip_suffix(')')?;
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(eval(&inner[start..i], env)?);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !inner[start..].trim().is_empty() {
        out.push(eval(&inner[start..], env)?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<f64> {
        match name {
            "Width" => Some(2.0),
            "Height" => Some(1.0),
            _ => None,
        }
    }

    #[test]
    fn evaluates_common_formulas() {
        assert_eq!(eval("Width*0.5", &env), Some(1.0));
        assert_eq!(eval("Height*0", &env), Some(0.0));
        assert_eq!(eval("Width-0.25 in", &env), Some(1.75));
        assert_eq!(eval("MIN(Width,Height)/2", &env), Some(0.5));
        assert_eq!(eval("-Width/4+1", &env), Some(0.5));
        assert!((eval("GUARD(25.4 mm)", &env).unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(eval("50%", &env), Some(0.5));
        assert!((eval("90 deg", &env).unwrap() - PI / 2.0).abs() < 1e-12);
        assert_eq!(eval("Geometry1.X1", &env), None);
        assert_eq!(eval("THEMEVAL()", &env), None);
        assert_eq!(eval("Inh", &env), None);
    }

    #[test]
    fn extracts_call_args() {
        assert_eq!(
            call_args("POLYLINE(0, 0, 0.5, Width*0.5)", "POLYLINE", &env),
            Some(vec![0.0, 0.0, 0.5, 1.0])
        );
        assert_eq!(call_args("NURBS(1,3)", "POLYLINE", &env), None);
    }
}
