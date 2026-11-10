//! U2.3: `{"$calc": "expr"}` - a tiny pure expression evaluator for batch numeric args.
//! Numbers, + - * / % ^, parentheses, min max clamp abs floor ceil round sqrt sin cos mix smoothstep, constant pi, loop variables, and $op.field references to earlier numeric results.
//! No strings, no I/O, no loops. Errors are `calc_error: ... at position N` (character offset in the expression).
use super::kit::*;

pub const MAX_LEN: usize = 400;
pub const MAX_TOKENS: usize = 120;
#[derive(Clone, Debug, PartialEq)]
pub enum K { Num(f64), Id(String), Ref(String), Op(char), Open, Close, Comma }
#[derive(Clone, Debug)]
pub struct T { pub k: K, pub pos: usize, pub end: usize }
fn err(pos: usize, m: &str) -> String { format!("calc_error: {m} at position {pos}") }
pub fn tokenize(s: &str) -> Result<Vec<T>, String> {
    let c: Vec<char> = s.chars().collect(); if c.len() > MAX_LEN { return Err(err(MAX_LEN, &format!("expression longer than the {MAX_LEN}-character cap"))); }
    let mut out = vec![]; let mut i = 0;
    while i < c.len() {
        let ch = c[i]; let st = i;
        if ch.is_whitespace() { i += 1; continue; }
        let k = if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).map_or(false, |d| d.is_ascii_digit())) { while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') { i += 1; } let t: String = c[st..i].iter().collect(); K::Num(t.parse::<f64>().map_err(|_| err(st, &format!("bad number '{t}'")))?) }
        else if ch.is_ascii_alphabetic() || ch == '_' { while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') { i += 1; } K::Id(c[st..i].iter().collect()) }
        else if ch == '$' { i += 1; while i < c.len() && (c[i].is_ascii_alphanumeric() || "_#.[]".contains(c[i])) { i += 1; } let t: String = c[st..i].iter().collect(); if !t.contains('.') && !t.contains('[') { return Err(err(st, "a reference looks like $op.field")); } K::Ref(t) }
        else if "+-*/%^".contains(ch) { i += 1; K::Op(ch) } else if ch == '(' { i += 1; K::Open } else if ch == ')' { i += 1; K::Close } else if ch == ',' { i += 1; K::Comma }
        else { return Err(err(st, &format!("unexpected character '{ch}' (strings and other symbols are not allowed)"))); };
        out.push(T { k, pos: st, end: i }); if out.len() > MAX_TOKENS { return Err(err(st, &format!("expression has more than {MAX_TOKENS} tokens (the op-count cap)"))); }
    }
    Ok(out)
}
/// Reference tokens ($op.field) inside an expression; a tokenizer error yields none (the evaluator reports it).
pub fn refs(expr: &str) -> Vec<String> { tokenize(expr).map(|t| t.into_iter().filter_map(|x| if let K::Ref(r) = x.k { Some(r) } else { None }).collect()).unwrap_or_default() }
/// Rewrites tokens: numeric loop variables become literals, and `ref_map` may rename references. Returns the new expression text.
pub fn rewrite(expr: &str, vars: &dyn Fn(&str) -> Option<f64>, ref_map: &dyn Fn(&str) -> String) -> Result<String, String> {
    let toks = tokenize(expr)?; let c: Vec<char> = expr.chars().collect(); let mut out = String::new(); let mut last = 0;
    for t in toks { out.extend(&c[last..t.pos]); last = t.end;
        match &t.k { K::Id(n) if !is_func(n) && n != "pi" => match vars(n) { Some(v) => out.push_str(&format!("({v:?})")), None => out.extend(&c[t.pos..t.end]) }, K::Ref(r) => out.push_str(&ref_map(r)), _ => out.extend(&c[t.pos..t.end]) } }
    out.extend(&c[last..]); Ok(out)
}
fn is_func(n: &str) -> bool { matches!(n, "min" | "max" | "clamp" | "abs" | "floor" | "ceil" | "round" | "sqrt" | "sin" | "cos" | "mix" | "smoothstep") }
struct P<'a> { t: Vec<T>, i: usize, v: &'a dyn Fn(&str) -> Result<Option<f64>, String>, end: usize }
impl<'a> P<'a> {
    fn pos(&self) -> usize { self.t.get(self.i).map_or(self.end, |t| t.pos) }
    fn peek(&self) -> Option<&K> { self.t.get(self.i).map(|t| &t.k) }
    fn fin(&self, x: f64, pos: usize) -> Result<f64, String> { if x.is_finite() { Ok(x) } else { Err(err(pos, "result is not a finite number")) } }
    fn add(&mut self) -> Result<f64, String> { let mut a = self.mul()?; while let Some(K::Op(o)) = self.peek().cloned() { if o != '+' && o != '-' { break; } let p = self.pos(); self.i += 1; let b = self.mul()?; a = self.fin(if o == '+' { a + b } else { a - b }, p)?; } Ok(a) }
    fn mul(&mut self) -> Result<f64, String> { let mut a = self.unary()?; while let Some(K::Op(o)) = self.peek().cloned() { if o != '*' && o != '/' && o != '%' { break; } let p = self.pos(); self.i += 1; let b = self.unary()?;
        a = match o { '*' => a * b, '/' => { if b == 0.0 { return Err(err(p, "division by zero")); } a / b } _ => { if b == 0.0 { return Err(err(p, "modulo by zero")); } a % b } }; a = self.fin(a, p)?; } Ok(a) }
    fn unary(&mut self) -> Result<f64, String> { if let Some(K::Op(o)) = self.peek().cloned() { if o == '-' || o == '+' { self.i += 1; let v = self.unary()?; return Ok(if o == '-' { -v } else { v }); } } self.pow() }
    fn pow(&mut self) -> Result<f64, String> { let a = self.atom()?; if let Some(K::Op('^')) = self.peek() { let p = self.pos(); self.i += 1; let b = self.unary()?; return self.fin(a.powf(b), p); } Ok(a) }
    fn atom(&mut self) -> Result<f64, String> {
        let Some(t) = self.t.get(self.i).cloned() else { return Err(err(self.end, "expression ends early")) }; self.i += 1;
        match t.k {
            K::Num(n) => Ok(n),
            K::Open => { let v = self.add()?; match self.peek() { Some(K::Close) => { self.i += 1; Ok(v) } _ => Err(err(self.pos(), "expected )")) } }
            K::Ref(r) => match (self.v)(&r).map_err(|m| err(t.pos, &m))? { Some(v) => self.fin(v, t.pos), None => Err(err(t.pos, &format!("{r} has no numeric value"))) },
            K::Id(n) if n == "pi" => Ok(std::f64::consts::PI),
            K::Id(n) if is_func(&n) => { if self.peek() != Some(&K::Open) { return Err(err(self.pos(), &format!("{n} needs ( arguments )"))); } self.i += 1; let mut args = vec![];
                if self.peek() == Some(&K::Close) { self.i += 1; } else { loop { args.push(self.add()?); match self.peek() { Some(K::Comma) => { self.i += 1; } Some(K::Close) => { self.i += 1; break; } _ => return Err(err(self.pos(), "expected , or )")) } } }
                let want: Option<usize> = match n.as_str() { "abs" | "floor" | "ceil" | "round" | "sqrt" | "sin" | "cos" => Some(1), "clamp" | "mix" | "smoothstep" => Some(3), _ => None };
                if let Some(w) = want { if args.len() != w { return Err(err(t.pos, &format!("{n} takes {w} argument(s), got {}", args.len()))); } } else if args.len() < 2 { return Err(err(t.pos, &format!("{n} takes at least 2 arguments"))); }
                let r = match n.as_str() { "min" => args.iter().cloned().fold(f64::INFINITY, f64::min), "max" => args.iter().cloned().fold(f64::NEG_INFINITY, f64::max), "abs" => args[0].abs(), "floor" => args[0].floor(), "ceil" => args[0].ceil(), "round" => args[0].round(), "sqrt" => args[0].sqrt(), "sin" => args[0].sin(), "cos" => args[0].cos(),
                    "clamp" => { if args[1] > args[2] { return Err(err(t.pos, "clamp needs lo <= hi")); } args[0].clamp(args[1], args[2]) } "mix" => args[0] + (args[1] - args[0]) * args[2],
                    _ => { if args[0] == args[1] { return Err(err(t.pos, "smoothstep needs edge0 != edge1")); } let x = ((args[2] - args[0]) / (args[1] - args[0])).clamp(0.0, 1.0); x * x * (3.0 - 2.0 * x) } };
                self.fin(r, t.pos) }
            K::Id(n) => match (self.v)(&n).map_err(|m| err(t.pos, &m))? { Some(v) => self.fin(v, t.pos), None => Err(err(t.pos, &format!("unknown name '{n}' (variables, pi and the listed functions only)"))) },
            _ => Err(err(t.pos, "unexpected token")),
        }
    }
}
/// Evaluate `expr`; `v` resolves names and $refs (None = unknown).
pub fn eval(expr: &str, v: &dyn Fn(&str) -> Result<Option<f64>, String>) -> Result<f64, String> {
    let t = tokenize(expr)?; if t.is_empty() { return Err(err(0, "empty expression")); } let end = expr.chars().count();
    let mut p = P { t, i: 0, v, end }; let r = p.add()?; if p.i < p.t.len() { return Err(err(p.pos(), "unexpected token after the expression")); } Ok(r)
}
pub fn is_calc(v: &Value) -> bool { v.as_object().map_or(false, |o| o.len() == 1 && o.get("$calc").map_or(false, |x| x.is_string())) }
pub fn num_json(x: f64) -> Value { if x.fract() == 0.0 && x.abs() < 1e15 { json!(x as i64) } else { json!(x) } }
pub fn commands() -> Vec<Command> { vec![] }
