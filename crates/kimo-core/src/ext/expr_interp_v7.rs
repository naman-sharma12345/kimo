//! Free-form expression interpreter (FEATURES.md L210/L220 gap): a real small expression language
//! -- arithmetic, comparisons, a ternary, and a function set -- that can read this property's own
//! keyframed value at any time (`value`, `valueAtTime(t)`), another clip's property or effect param at
//! any time (`link("clip:prop", t)` -- AE's pickwhip/valueAtTime linkage, in one primitive), and this
//! clip's position among its track's clips ordered by start time (`index`, this project's closest
//! honest analogue to AE's layer index -- there is no stacked-layer model here to index into).
//!
//! Architecture mirrors link_property.rs exactly (same author's existing live-linkage precedent):
//! set_expression_script parses, validates by evaluating once against the CURRENT project, bakes one
//! linear key per frame directly into the clip via set_keyframes (dispatch_nested), and registers a
//! standing ExprScript record (Project.expr_scripts) so render and sample_property re-bake it fresh at
//! every render. `link()` targets (another clip's property) stay fully live -- an edit to a linked
//! driver after the fact is picked up automatically, same guarantee link_property gives. `value` and
//! `valueAtTime` are different: they read a FROZEN snapshot of this property's own keyframes taken at
//! set_expression_script time (ExprScript::base), not the live project -- because this script bakes
//! INTO that same property, so a live self-read would feed each resolve's output into the next
//! resolve's input (see ExprScript::base's doc comment for the confirmed failure mode). resolve_live
//! composes resolve_links then resolve_expr_scripts at every render/sample_property call site, so an
//! expression script's link() can read an already-resolved link.
use super::kit::*;
use crate::model::Project;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ExprScript {
    pub clip_id: String, pub property: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub effect_id: Option<String>,
    pub script: String, pub fps: f64,
    /// Frozen snapshot of the property's own keyframes from the moment this script was (re)registered --
    /// what `value`/`valueAtTime` read. Frozen, not live: baking into this SAME property on every resolve
    /// means the live project's own copy of this property is the script's OUTPUT, not its source, so
    /// reading `value` against the live project would feed each resolve's output into the next resolve's
    /// input (confirmed experimentally: a `valueAtTime(time + 1)` self-reference drifted further off its
    /// true curve on every subsequent sample_property call before this fix). AE avoids this by never
    /// baking; this project's bake-based link_property precedent does not have a self-reference case to
    /// break on, since a link's driver is normally a different property. Re-running set_expression_script
    /// re-captures a fresh base from whatever the property holds at that moment.
    pub base: crate::keyframe::Prop,
}

// ---------------------------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok { Num(f64), Str(String), Ident(String), Op(char), Le, Ge, Eq, Ne, And, Or, LParen, RParen, Comma, Question, Colon, End }

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let b: Vec<char> = s.chars().collect(); let mut i = 0; let mut out = vec![];
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() { i += 1; continue; }
        if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let start = i; while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.') { i += 1; }
            let txt: String = b[start..i].iter().collect();
            out.push(Tok::Num(txt.parse().map_err(|_| format!("bad number '{txt}' at {start}"))?)); continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i; while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') { i += 1; }
            out.push(Tok::Ident(b[start..i].iter().collect())); continue;
        }
        if c == '"' || c == '\'' {
            let quote = c; i += 1; let start = i;
            while i < b.len() && b[i] != quote { i += 1; }
            if i >= b.len() { return Err(format!("unterminated string starting at {start}")); }
            out.push(Tok::Str(b[start..i].iter().collect())); i += 1; continue;
        }
        match c {
            '+' | '-' | '*' | '/' | '%' | '^' => { out.push(Tok::Op(c)); i += 1; }
            '(' => { out.push(Tok::LParen); i += 1; }
            ')' => { out.push(Tok::RParen); i += 1; }
            ',' => { out.push(Tok::Comma); i += 1; }
            '?' => { out.push(Tok::Question); i += 1; }
            ':' => { out.push(Tok::Colon); i += 1; }
            '<' => { if i + 1 < b.len() && b[i + 1] == '=' { out.push(Tok::Le); i += 2; } else { out.push(Tok::Op('<')); i += 1; } }
            '>' => { if i + 1 < b.len() && b[i + 1] == '=' { out.push(Tok::Ge); i += 2; } else { out.push(Tok::Op('>')); i += 1; } }
            '=' => { if i + 1 < b.len() && b[i + 1] == '=' { out.push(Tok::Eq); i += 2; } else { return Err(format!("unexpected '=' at {i} (did you mean '=='?)")); } }
            '!' => { if i + 1 < b.len() && b[i + 1] == '=' { out.push(Tok::Ne); i += 2; } else { return Err(format!("unexpected '!' at {i}")); } }
            '&' => { if i + 1 < b.len() && b[i + 1] == '&' { out.push(Tok::And); i += 2; } else { return Err(format!("unexpected '&' at {i} (did you mean '&&'?)")); } }
            '|' => { if i + 1 < b.len() && b[i + 1] == '|' { out.push(Tok::Or); i += 2; } else { return Err(format!("unexpected '|' at {i} (did you mean '||'?)")); } }
            _ => return Err(format!("unexpected character '{c}' at {i}")),
        }
    }
    out.push(Tok::End); Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Parser (recursive descent): ternary -> or -> and -> comparison -> additive -> multiplicative -> power -> unary -> primary
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Ast { Num(f64), Str(String), Var(String), Call(String, Vec<Ast>), Unary(char, Box<Ast>), Bin(&'static str, Box<Ast>, Box<Ast>), Ternary(Box<Ast>, Box<Ast>, Box<Ast>) }

struct P { t: Vec<Tok>, i: usize }
impl P {
    fn peek(&self) -> &Tok { &self.t[self.i] }
    fn next(&mut self) -> Tok { let t = self.t[self.i].clone(); self.i += 1; t }
    fn expect(&mut self, want: &Tok, what: &str) -> Result<(), String> { if self.peek() == want { self.i += 1; Ok(()) } else { Err(format!("expected {what}, found {:?}", self.peek())) } }

    fn ternary(&mut self) -> Result<Ast, String> {
        let c = self.or_expr()?;
        if *self.peek() == Tok::Question { self.i += 1; let a = self.ternary()?; self.expect(&Tok::Colon, "':' in ternary")?; let b = self.ternary()?; return Ok(Ast::Ternary(Box::new(c), Box::new(a), Box::new(b))); }
        Ok(c)
    }
    fn or_expr(&mut self) -> Result<Ast, String> { let mut a = self.and_expr()?; while *self.peek() == Tok::Or { self.i += 1; let b = self.and_expr()?; a = Ast::Bin("||", Box::new(a), Box::new(b)); } Ok(a) }
    fn and_expr(&mut self) -> Result<Ast, String> { let mut a = self.cmp()?; while *self.peek() == Tok::And { self.i += 1; let b = self.cmp()?; a = Ast::Bin("&&", Box::new(a), Box::new(b)); } Ok(a) }
    fn cmp(&mut self) -> Result<Ast, String> {
        let mut a = self.add()?;
        loop { let op = match self.peek() { Tok::Op('<') => "<", Tok::Op('>') => ">", Tok::Le => "<=", Tok::Ge => ">=", Tok::Eq => "==", Tok::Ne => "!=", _ => break };
            self.i += 1; let b = self.add()?; a = Ast::Bin(op, Box::new(a), Box::new(b)); }
        Ok(a)
    }
    fn add(&mut self) -> Result<Ast, String> {
        let mut a = self.mul()?;
        loop { let op = match self.peek() { Tok::Op('+') => '+', Tok::Op('-') => '-', _ => break };
            self.i += 1; let b = self.mul()?; a = Ast::Bin(if op == '+' { "+" } else { "-" }, Box::new(a), Box::new(b)); }
        Ok(a)
    }
    fn mul(&mut self) -> Result<Ast, String> {
        let mut a = self.pow()?;
        loop { let op = match self.peek() { Tok::Op('*') => '*', Tok::Op('/') => '/', Tok::Op('%') => '%', _ => break };
            self.i += 1; let b = self.pow()?; a = Ast::Bin(match op { '*' => "*", '/' => "/", _ => "%" }, Box::new(a), Box::new(b)); }
        Ok(a)
    }
    fn pow(&mut self) -> Result<Ast, String> {
        let a = self.unary()?;
        if *self.peek() == Tok::Op('^') { self.i += 1; let b = self.pow()?; return Ok(Ast::Bin("^", Box::new(a), Box::new(b))); } // right-assoc
        Ok(a)
    }
    fn unary(&mut self) -> Result<Ast, String> {
        match self.peek().clone() { Tok::Op('-') => { self.i += 1; Ok(Ast::Unary('-', Box::new(self.unary()?))) } Tok::Op('!') => { self.i += 1; Ok(Ast::Unary('!', Box::new(self.unary()?))) } _ => self.primary() }
    }
    fn primary(&mut self) -> Result<Ast, String> {
        match self.next() {
            Tok::Num(n) => Ok(Ast::Num(n)),
            Tok::Str(s) => Ok(Ast::Str(s)),
            Tok::LParen => { let e = self.ternary()?; self.expect(&Tok::RParen, "')'")?; Ok(e) }
            Tok::Ident(name) => {
                if *self.peek() == Tok::LParen {
                    self.i += 1; let mut args = vec![];
                    if *self.peek() != Tok::RParen { loop { args.push(self.ternary()?); if *self.peek() == Tok::Comma { self.i += 1; } else { break; } } }
                    self.expect(&Tok::RParen, "')' closing a call")?;
                    Ok(Ast::Call(name, args))
                } else { Ok(Ast::Var(name)) }
            }
            other => Err(format!("unexpected token {other:?}")),
        }
    }
}

fn parse(script: &str) -> Result<Ast, String> {
    let toks = tokenize(script)?; let mut p = P { t: toks, i: 0 };
    let ast = p.ternary()?;
    if *p.peek() != Tok::End { return Err(format!("unexpected trailing input at token {}: {:?}", p.i, p.peek())); }
    Ok(ast)
}

// ---------------------------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------------------------

struct Ctx<'a> { project: &'a Project, clip_id: &'a str, clip: &'a crate::model::Clip, base: &'a crate::keyframe::Prop, time: f64 }

/// This project's closest honest analogue to AE's layer index: 1-based position of clip_id among its
/// own track's clips, ordered by start time (ties broken by id) -- there is no AE-style single stacked
/// layer order here, so "index" is scoped to the clip's own track.
fn clip_index(p: &Project, clip_id: &str) -> Result<f64, String> {
    for t in &p.tracks {
        if t.clips.iter().any(|c| c.id == clip_id) {
            let mut order: Vec<(f64, &str)> = t.clips.iter().map(|c| (c.start, c.id.as_str())).collect();
            order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(b.1)));
            let pos = order.iter().position(|(_, id)| *id == clip_id).unwrap();
            return Ok((pos + 1) as f64);
        }
    }
    Err(format!("clip '{clip_id}' is not on any track (needed for `index`)"))
}

fn truthy(v: f64) -> bool { v != 0.0 }
fn b(v: bool) -> f64 { if v { 1.0 } else { 0.0 } }

fn eval_str(e: &Ast) -> Result<String, String> { match e { Ast::Str(s) => Ok(s.clone()), _ => Err("expected a string literal (e.g. a \"clip:property\" reference) here".into()) } }

/// Parse a link() reference: "clip_ref:property" or "clip_ref:effect_id.property". clip_ref may be a raw
/// clip id or an @name (crate::ext::names::resolve_ref).
fn parse_link_ref(project: &Project, reference: &str) -> Result<(String, Option<String>, String), String> {
    let (clip_ref, rest) = reference.split_once(':').ok_or_else(|| format!("link() reference '{reference}' needs a ':' -- \"clip:property\" or \"clip:effect_id.property\""))?;
    let clip_id = crate::ext::names::resolve_ref(project, "clip", clip_ref)?;
    match rest.split_once('.') { Some((eff, prop)) => Ok((clip_id, Some(eff.to_string()), prop.to_string())), None => Ok((clip_id, None, rest.to_string())) }
}

fn eval(e: &Ast, ctx: &Ctx) -> Result<f64, String> {
    match e {
        Ast::Num(n) => Ok(*n),
        Ast::Str(_) => Err("a string cannot be used as a number; strings are only valid as link() reference arguments".into()),
        Ast::Var(name) => match name.as_str() {
            "time" => Ok(ctx.time),
            "value" => Ok(ctx.base.at(crate::render::clip_local_time(ctx.clip, ctx.time))),
            "index" => clip_index(ctx.project, ctx.clip_id),
            "PI" => Ok(std::f64::consts::PI),
            "E" => Ok(std::f64::consts::E),
            other => Err(format!("unknown variable '{other}' (known: time, value, index, PI, E)")),
        },
        Ast::Unary(op, a) => { let v = eval(a, ctx)?; Ok(match op { '-' => -v, '!' => b(!truthy(v)), _ => unreachable!() }) }
        Ast::Bin(op, a, bx) => {
            if *op == "&&" { return Ok(b(truthy(eval(a, ctx)?) && truthy(eval(bx, ctx)?))); }
            if *op == "||" { return Ok(b(truthy(eval(a, ctx)?) || truthy(eval(bx, ctx)?))); }
            let (x, y) = (eval(a, ctx)?, eval(bx, ctx)?);
            Ok(match *op {
                "+" => x + y, "-" => x - y, "*" => x * y,
                "/" => { if y == 0.0 { return Err("division by zero".into()); } x / y }
                "%" => { if y == 0.0 { return Err("modulo by zero".into()); } x.rem_euclid(y) }
                "^" => x.powf(y),
                "<" => b(x < y), ">" => b(x > y), "<=" => b(x <= y), ">=" => b(x >= y), "==" => b((x - y).abs() < 1e-12), "!=" => b((x - y).abs() >= 1e-12),
                _ => unreachable!(),
            })
        }
        Ast::Ternary(c, a, bx) => if truthy(eval(c, ctx)?) { eval(a, ctx) } else { eval(bx, ctx) },
        Ast::Call(name, args) => {
            let n1 = || -> Result<f64, String> { if args.len() != 1 { return Err(format!("{name}() takes 1 argument")); } eval(&args[0], ctx) };
            let n2 = || -> Result<(f64, f64), String> { if args.len() != 2 { return Err(format!("{name}() takes 2 arguments")); } Ok((eval(&args[0], ctx)?, eval(&args[1], ctx)?)) };
            match name.as_str() {
                "sin" => Ok(n1()?.sin()), "cos" => Ok(n1()?.cos()), "tan" => Ok(n1()?.tan()),
                "abs" => Ok(n1()?.abs()), "floor" => Ok(n1()?.floor()), "ceil" => Ok(n1()?.ceil()), "round" => Ok(n1()?.round()),
                "sqrt" => { let v = n1()?; if v < 0.0 { return Err("sqrt() of a negative number".into()); } Ok(v.sqrt()) }
                "min" => { let (x, y) = n2()?; Ok(x.min(y)) } "max" => { let (x, y) = n2()?; Ok(x.max(y)) }
                "pow" => { let (x, y) = n2()?; Ok(x.powf(y)) }
                "mod" => { let (x, y) = n2()?; if y == 0.0 { return Err("mod() by zero".into()); } Ok(x.rem_euclid(y)) }
                "clamp" => { if args.len() != 3 { return Err("clamp() takes 3 arguments: value, min, max".into()); } let v = eval(&args[0], ctx)?; let lo = eval(&args[1], ctx)?; let hi = eval(&args[2], ctx)?; Ok(v.clamp(lo.min(hi), lo.max(hi))) }
                "linear" => { if args.len() != 5 { return Err("linear() takes 5 arguments: t, tMin, tMax, value1, value2".into()); }
                    let t = eval(&args[0], ctx)?; let t_min = eval(&args[1], ctx)?; let t_max = eval(&args[2], ctx)?; let v1 = eval(&args[3], ctx)?; let v2 = eval(&args[4], ctx)?;
                    if (t_max - t_min).abs() < 1e-12 { return Ok(v1); } let f = ((t - t_min) / (t_max - t_min)).clamp(0.0, 1.0); Ok(v1 + f * (v2 - v1)) }
                "valueAtTime" => { let t = n1()?; Ok(ctx.base.at(crate::render::clip_local_time(ctx.clip, t))) }
                "link" => {
                    if args.is_empty() || args.len() > 2 { return Err("link() takes a \"clip:property\" reference and an optional time: link(ref) or link(ref, t)".into()); }
                    let reference = eval_str(&args[0])?;
                    let t = if args.len() == 2 { eval(&args[1], ctx)? } else { ctx.time };
                    let (clip_id, effect_id, property) = parse_link_ref(ctx.project, &reference)?;
                    crate::ext::link_property::eval_property_at(ctx.project, &clip_id, &property, effect_id.as_deref(), t)
                }
                other => Err(format!("unknown function '{other}'")),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Bake + standing registration (mirrors link_property.rs's apply_link / resolve_links exactly)
// ---------------------------------------------------------------------------------------------

/// Recompute one expression script's target keys from `src` (the pre-this-pass snapshot: `value` and
/// `valueAtTime` read THIS property's own keyframes from before this script overwrote them, matching
/// AE's own semantics where `value` is the keyframed value the expression modifies, not the expression's
/// own prior output), writing the baked keys onto the matching clip/property in `out`.
fn apply_script(src: &Project, out: &mut Project, sc: &ExprScript) -> Result<(), String> {
    let ast = parse(&sc.script)?;
    let tc = src.find_clip(&sc.clip_id).ok_or_else(|| format!("no clip '{}'", sc.clip_id))?.clone();
    let fps = if sc.fps.is_finite() && sc.fps > 0.0 { sc.fps } else { 30.0 };
    let n = ((tc.duration * fps).floor() as usize).max(1) + 1;
    if n > 100_000 { return Err("clip too long to bake this expression at this fps".into()); }
    let times: Vec<f64> = (0..n).map(|i| (tc.start + i as f64 / fps).min(tc.start + tc.duration)).collect();
    let mut keys = Vec::with_capacity(n); let mut lt_prev = f64::NEG_INFINITY;
    for t in &times {
        let ctx = Ctx { project: src, clip_id: &sc.clip_id, clip: &tc, base: &sc.base, time: *t };
        let v = eval(&ast, &ctx)?;
        if !v.is_finite() { return Err(format!("the expression gives a non-finite value at {t}s")); }
        let lt = crate::render::clip_local_time(&tc, *t); if lt <= lt_prev + 1e-9 { continue; } lt_prev = lt;
        keys.push(crate::keyframe::Key { t: lt, v, ease: crate::keyframe::Ease::Linear, bez: None, out: None, inn: None });
    }
    let c = out.find_clip_mut(&sc.clip_id).ok_or("target vanished")?;
    let prop = match &sc.effect_id { Some(e) => c.effects.iter_mut().find(|x| x.id == *e).and_then(|x| x.params.get_mut(&sc.property)), None => c.props.get_mut(&sc.property) };
    let prop = prop.ok_or_else(|| format!("clip '{}' has no property '{}'", sc.clip_id, sc.property))?;
    prop.keys = keys; prop.expr.clear();
    Ok(())
}

/// The project as rendering / sample_property should see it: every registered expression script
/// re-baked fresh from its current sources. Zero cost when there are none. Up to `expr_scripts.len()`
/// passes so one script reading another's link() output converges; a script whose target has vanished
/// is skipped, not fatal, same posture as resolve_links toward a vanished link target.
pub fn resolve_expr_scripts(p: &Project) -> std::borrow::Cow<'_, Project> {
    if p.expr_scripts.is_empty() { return std::borrow::Cow::Borrowed(p); }
    let mut out = p.clone();
    for _ in 0..p.expr_scripts.len().max(1) {
        let snapshot = out.clone();
        for sc in &p.expr_scripts { let _ = apply_script(&snapshot, &mut out, sc); }
    }
    std::borrow::Cow::Owned(out)
}

/// Composes resolve_links then resolve_expr_scripts, in that order (a script's link() sees an
/// already-resolved link; a link's driver sees the PREVIOUS render's baked script output, converging
/// over successive renders same as a driver chain does) -- the one function render.rs and curves.rs
/// should call instead of resolve_links alone.
pub fn resolve_live(p_in: &Project) -> std::borrow::Cow<'_, Project> {
    let after_links = crate::ext::link_property::resolve_links(p_in);
    match resolve_expr_scripts(after_links.as_ref()) {
        std::borrow::Cow::Owned(p2) => std::borrow::Cow::Owned(p2),
        std::borrow::Cow::Borrowed(_) => after_links,
    }
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = crate::ext::names::resolve_ref(&ss.project, "clip", str_arg(a, "clip_id")?)?;
    let prop = str_arg(a, "property")?.to_string();
    let eff = a.get("effect_id").and_then(|v| v.as_str()).map(String::from);
    let script = str_arg(a, "script")?.to_string();
    let fps = a.get("fps").and_then(|v| v.as_f64()).unwrap_or(ss.project.fps.max(30.0));
    if !(fps > 0.0 && fps <= 240.0) { return Err("fps must be in (0, 240]".into()); }
    let ast = parse(&script)?; // syntax error surfaces immediately, nothing changed yet

    // the target must exist as a property before anything changes (same existence check link_property uses)
    let (start, dur) = { let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no clip '{cid}'"))?; (c.start, c.duration) };
    let mut tq = json!({"clip_id": cid, "property": prop, "times": [start]}); if let Some(e) = &eff { tq["effect_id"] = json!(e); }
    crate::commands::dispatch_nested(ss, "sample_property", &tq).map_err(|e| format!("target: {e}"))?;

    // Freeze `value`/`valueAtTime`'s source NOW, before anything bakes over this same property -- see the
    // ExprScript::base doc comment for why this must not be re-read live from the project on every resolve.
    let tc = ss.project.find_clip(&cid).unwrap().clone();
    let base_prop: crate::keyframe::Prop = match &eff {
        Some(e) => tc.effects.iter().find(|x| x.id == *e).and_then(|x| x.params.get(&prop)).cloned(),
        None => tc.props.get(&prop).cloned(),
    }.unwrap_or_else(|| crate::ext::curves::static_default(&prop).unwrap_or_else(|| crate::keyframe::Prop::new(0.0)));

    // validation bake against the CURRENT project (before any mutation), so a bad script (unknown
    // function, a link() to a clip that doesn't exist, a division by zero at some frame) fails loud
    // with nothing written -- same fail-fast posture the rest of this mission's TDD discipline expects.
    let n = ((dur * fps).floor() as usize).max(1) + 1;
    if n > 100_000 { return Err("the clip is too long to bake this expression at this fps".into()); }
    let times: Vec<f64> = (0..n).map(|i| (start + i as f64 / fps).min(start + dur)).collect();
    let mut keys = Vec::with_capacity(n); let mut lt_prev = f64::NEG_INFINITY;
    for t in &times {
        let ctx = Ctx { project: &ss.project, clip_id: &cid, clip: &tc, base: &base_prop, time: *t };
        let v = eval(&ast, &ctx)?;
        if !v.is_finite() { return Err(format!("the expression gives a non-finite value at {t}s; nothing was changed")); }
        let lt = crate::render::clip_local_time(&tc, *t); if lt <= lt_prev + 1e-9 { continue; } lt_prev = lt;
        keys.push(json!({"t": lt, "v": v, "ease": "linear"}));
    }
    let mut k = json!({"clip_id": cid, "property": prop, "keys": keys, "mode": "replace_all"}); if let Some(e) = &eff { k["effect_id"] = json!(e); }
    crate::commands::dispatch_nested(ss, "set_keyframes", &k)?;
    let c = ss.project.find_clip_mut(&cid).ok_or("target vanished")?;
    let p = match &eff { Some(e) => c.effects.iter_mut().find(|x| x.id == *e).and_then(|x| x.params.get_mut(&prop)), None => c.props.get_mut(&prop) };
    let dropped = p.map(|p| { let d = p.expr.len(); p.expr.clear(); d }).unwrap_or(0);

    let rec = ExprScript { clip_id: cid.clone(), property: prop.clone(), effect_id: eff.clone(), script: script.clone(), fps, base: base_prop };
    ss.project.expr_scripts.retain(|r| !(r.clip_id == rec.clip_id && r.property == rec.property && r.effect_id == rec.effect_id));
    ss.project.expr_scripts.push(rec);
    Ok(json!({"clip_id": cid, "property": prop, "keys": keys.len(), "range": [start, start + dur], "expressions_replaced": dropped,
        "note": "live expression: re-baked fresh from value/valueAtTime/link/index at every render and sample_property call; re-run set_expression_script only to change the script"}))
}

fn clear_expression_script(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = crate::ext::names::resolve_ref(&ss.project, "clip", str_arg(a, "clip_id")?)?;
    let prop = str_arg(a, "property")?.to_string();
    let eff = a.get("effect_id").and_then(|v| v.as_str()).map(String::from);
    let before = ss.project.expr_scripts.len();
    ss.project.expr_scripts.retain(|r| !(r.clip_id == cid && r.property == prop && r.effect_id == eff));
    Ok(json!({"removed": before - ss.project.expr_scripts.len(), "note": "already-baked keyframes are left in place; this only stops future re-baking"}))
}
fn list_expression_scripts(ss: &mut Session, a: &Value) -> Result<Value, String> {
    match a.get("clip_id").and_then(|v| v.as_str()) {
        Some(c) => { let cid = crate::ext::names::resolve_ref(&ss.project, "clip", c)?; Ok(json!({"scripts": ss.project.expr_scripts.iter().filter(|r| r.clip_id == cid).cloned().collect::<Vec<_>>()})) }
        None => Ok(json!({"scripts": ss.project.expr_scripts})),
    }
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "set_expression_script", description: "Free-form expression interpreter (FEATURES.md L210/L220 gap, After Effects-style). A real small expression language on a property or effect param -- arithmetic (+ - * / % ^), comparisons (< > <= >= == !=), && || !, a ? b : c ternary, and functions sin/cos/tan/abs/sqrt/floor/ceil/round/min/max/pow/mod/clamp(v,lo,hi)/linear(t,tMin,tMax,v1,v2). Variables: time (project timeline seconds), value (this property's own keyframed value at `time`, frozen from the moment this call registers the script -- AE's `value`; re-run this command to pick up a newer base), index (1-based position of this clip among its own track's clips ordered by start time -- this project's analogue to AE's layer index, since there is no stacked-layer model to index into), PI, E. valueAtTime(t) reads that same frozen base at an arbitrary time. link(\"clip:property\", t?) reads ANY other clip's property or effect param (\"clip:effect_id.property\" for an effect param; clip may be an @name) at time t (default `time`) -- the pickwhip/linkage primitive. LIVE like link_property: baked to one linear key per frame now AND re-baked fresh at every render/sample_property call, so an edited link() source is picked up automatically; call again only to change the script.", mutates: true,
        schema: obj(vec![("clip_id", s("target clip")), ("property", s("target property, or effect param with effect_id")), ("effect_id", s("optional: target effect")), ("script", s("the expression, e.g. \"value + sin(time*2)*10\" or \"link(\\\"@other:opacity\\\") * 0.5\"")), ("fps", n("keys per second to bake at; default = project fps (min 30)"))], &["clip_id", "property", "script"]),
        run },
    Command { name: "clear_expression_script", description: "Stop a property's standing expression script from re-baking on future renders. Leaves its already-baked keyframes in place (static from then on) -- does not restore any prior animation.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("property", s("property name")), ("effect_id", s("optional: effect"))], &["clip_id", "property"]), run: clear_expression_script },
    Command { name: "list_expression_scripts", description: "List registered expression scripts, for one clip or the whole project.", mutates: false,
        schema: obj(vec![("clip_id", s("optional: one clip; omit for the whole project"))], &[]), run: list_expression_scripts },
] }
