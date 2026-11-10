//! Pure typed float-vector expression AST. No I/O, loops or side effects.
use crate::effects::Frame;
#[derive(Clone, Debug)]
enum Node {
    Num(f32),
    Var(String),
    Unary(Box<Node>),
    Bin(char, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}
#[derive(Clone, Debug)]
pub struct Program {
    root: Node,
    pub ops: usize,
}
struct Parser<'a> {
    s: &'a str,
    pos: usize,
    ops: usize,
    depth: usize,
    cap: usize,
}
const VARS: &[&str] = &[
    "x", "y", "u", "v", "w", "h", "t", "r", "g", "b", "a", "src", "p0", "p1", "p2", "p3", "p4",
    "p5", "p6", "p7",
];
const FUNCS: &[(&str, usize)] = &[
    ("sin", 1),
    ("pow", 2),
    ("mix", 3),
    ("smoothstep", 3),
    ("clamp", 3),
    ("fract", 1),
    ("floor", 1),
    ("noise", 3),
    ("min", 2),
    ("max", 2),
    ("atan2", 2),
    ("sqrt", 1),
    ("vec4", 4),
    ("sample", 2),
    ("aux1", 2),
    ("red", 1),
    ("green", 1),
    ("blue", 1),
    ("alpha", 1),
];
impl Parser<'_> {
    fn ws(&mut self) {
        while self
            .s
            .as_bytes()
            .get(self.pos)
            .map_or(false, |b| b.is_ascii_whitespace())
        {
            self.pos += 1
        }
    }
    fn err(&self, m: &str) -> String {
        format!("pixel_expr compile at byte {}: {m}", self.pos)
    }
    fn expr(&mut self, min: u8) -> Result<Node, String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self.err("depth cap 64 exceeded"));
        }
        self.ws();
        let mut left = if self.s.as_bytes().get(self.pos) == Some(&b'-') {
            self.pos += 1;
            Node::Unary(Box::new(self.expr(3)?))
        } else {
            self.atom()?
        };
        loop {
            self.ws();
            let op = self.s.as_bytes().get(self.pos).copied().unwrap_or(0) as char;
            let prec = match op {
                '+' | '-' => 1,
                '*' | '/' => 2,
                _ => 0,
            };
            if prec == 0 || prec < min {
                break;
            }
            self.pos += 1;
            let right = self.expr(prec + 1)?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
            self.ops += 1;
            self.limit()?;
        }
        self.depth -= 1;
        Ok(left)
    }
    fn limit(&self) -> Result<(), String> {
        if self.ops > self.cap {
            Err(self.err(&format!(
                "expr_max_ops cap {} exceeded (set_limits expr_max_ops)",
                self.cap
            )))
        } else {
            Ok(())
        }
    }
    fn atom(&mut self) -> Result<Node, String> {
        self.ws();
        let start = self.pos;
        let b = self
            .s
            .as_bytes()
            .get(start)
            .copied()
            .ok_or_else(|| self.err("expected expression"))?;
        self.ops += 1;
        self.limit()?;
        if b == b'(' {
            self.pos += 1;
            let n = self.expr(1)?;
            self.ws();
            if self.s.as_bytes().get(self.pos) != Some(&b')') {
                return Err(self.err("expected ')'"));
            }
            self.pos += 1;
            return Ok(n);
        }
        if b.is_ascii_digit() || b == b'.' {
            self.pos += 1;
            while let Some(&c) = self.s.as_bytes().get(self.pos) {
                if c.is_ascii_digit()
                    || c == b'.'
                    || c == b'e'
                    || c == b'E'
                    || ((c == b'+' || c == b'-')
                        && matches!(self.s.as_bytes()[self.pos - 1], b'e' | b'E'))
                {
                    self.pos += 1
                } else {
                    break;
                }
            }
            let n = self.s[start..self.pos]
                .parse::<f32>()
                .map_err(|_| self.err("invalid number"))?;
            if !n.is_finite() {
                return Err(self.err("number must be finite f32"));
            }
            return Ok(Node::Num(n));
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            self.pos += 1;
            while self
                .s
                .as_bytes()
                .get(self.pos)
                .map_or(false, |c| c.is_ascii_alphanumeric() || *c == b'_')
            {
                self.pos += 1
            }
            let name = self.s[start..self.pos].to_string();
            self.ws();
            if self.s.as_bytes().get(self.pos) == Some(&b'(') {
                self.pos += 1;
                let mut args = vec![];
                self.ws();
                if self.s.as_bytes().get(self.pos) != Some(&b')') {
                    loop {
                        args.push(self.expr(1)?);
                        self.ws();
                        match self.s.as_bytes().get(self.pos) {
                            Some(b',') => self.pos += 1,
                            Some(b')') => break,
                            _ => return Err(self.err("expected ',' or ')'")),
                        }
                    }
                }
                self.pos += 1;
                let expected = FUNCS
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, n)| *n)
                    .ok_or_else(|| {
                        self.err(&format!(
                            "unknown function '{name}'; did you mean '{}' ?",
                            nearest(&name, FUNCS.iter().map(|(n, _)| *n))
                        ))
                    })?;
                if args.len() != expected {
                    return Err(self.err(&format!("{name} needs {expected} arguments")));
                }
                return Ok(Node::Call(name, args));
            }
            if !VARS.contains(&name.as_str()) {
                return Err(self.err(&format!(
                    "unknown variable '{name}'; did you mean '{}' ?",
                    nearest(&name, VARS.iter().copied())
                )));
            }
            return Ok(Node::Var(name));
        }
        Err(self.err("unexpected token"))
    }
}
fn nearest<'a>(s: &str, names: impl Iterator<Item = &'a str>) -> &'a str {
    names
        .min_by_key(|n| {
            let mut d = 0;
            for (a, b) in s.bytes().zip(n.bytes()) {
                if a != b {
                    d += 1
                }
            }
            d + s.len().abs_diff(n.len())
        })
        .unwrap_or("src")
}
pub fn compile(s: &str, max_ops: usize) -> Result<Program, String> {
    if s.len() > 65536 {
        return Err("pixel_expr source cap 65536 bytes exceeded".into());
    }
    let mut p = Parser {
        s,
        pos: 0,
        ops: 0,
        depth: 0,
        cap: max_ops,
    };
    let root = p.expr(1)?;
    p.ws();
    if p.pos != s.len() {
        return Err(p.err("trailing token"));
    }
    Ok(Program { root, ops: p.ops })
}
#[derive(Clone, Copy)]
struct Val {
    c: [f32; 4],
    vector: bool,
}
impl Val {
    fn scalar(v: f32) -> Self {
        Self {
            c: [v; 4],
            vector: false,
        }
    }
    fn vector(c: [f32; 4]) -> Self {
        Self { c, vector: true }
    }
}
struct Ctx<'a> {
    x: usize,
    y: usize,
    t: f32,
    src: &'a Frame,
    aux: Option<&'a Frame>,
    params: [f32; 8],
}
pub fn sample(f: &Frame, u: f32, v: f32, edge: &str) -> [f32; 4] {
    let map = |q: f32| {
        if edge == "wrap" {
            q - q.floor()
        } else {
            q.clamp(0., 1.)
        }
    };
    if edge == "transparent" && (u < 0. || v < 0. || u > 1. || v > 1.) {
        return [0.; 4];
    }
    let x = map(u) * (f.w.saturating_sub(1)) as f32;
    let y = map(v) * (f.h.saturating_sub(1)) as f32;
    let (a, b) = (x.floor() as usize, y.floor() as usize);
    let (c, d) = ((a + 1).min(f.w - 1), (b + 1).min(f.h - 1));
    let (tx, ty) = (x - a as f32, y - b as f32);
    let mut out = [0.; 4];
    for j in 0..4 {
        let p = f.d[(b * f.w + a) * 4 + j];
        let q = f.d[(b * f.w + c) * 4 + j];
        let r = f.d[(d * f.w + a) * 4 + j];
        let s = f.d[(d * f.w + c) * 4 + j];
        out[j] = (p + (q - p) * tx) + ((r + (s - r) * tx) - (p + (q - p) * tx)) * ty;
    }
    out
}
fn noise(seed: f32, x: f32, y: f32) -> f32 {
    let mut z = (seed.to_bits() as u64) ^ ((x.to_bits() as u64) << 32) ^ (y.to_bits() as u64);
    z = z.wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((z ^ (z >> 31)) >> 40) as f32 / 16777216.
}
fn eval(n: &Node, c: &Ctx) -> Val {
    match n {
        Node::Num(v) => Val::scalar(*v),
        Node::Var(k) => {
            let i = (c.y * c.src.w + c.x) * 4;
            match k.as_str() {
                "src" => Val::vector(c.src.d[i..i + 4].try_into().unwrap()),
                "r" | "g" | "b" | "a" => Val::scalar(
                    c.src.d[i + match k.as_str() {
                        "r" => 0,
                        "g" => 1,
                        "b" => 2,
                        _ => 3,
                    }],
                ),
                "x" => Val::scalar(c.x as f32),
                "y" => Val::scalar(c.y as f32),
                "u" => Val::scalar(c.x as f32 / c.src.w.saturating_sub(1).max(1) as f32),
                "v" => Val::scalar(c.y as f32 / c.src.h.saturating_sub(1).max(1) as f32),
                "w" => Val::scalar(c.src.w as f32),
                "h" => Val::scalar(c.src.h as f32),
                "t" => Val::scalar(c.t),
                _ => Val::scalar(c.params[k.as_bytes()[1] as usize - b'0' as usize]),
            }
        }
        Node::Unary(a) => {
            let mut a = eval(a, c);
            for v in &mut a.c {
                *v = -*v
            }
            a
        }
        Node::Bin(op, a, b) => {
            let (a, b) = (eval(a, c), eval(b, c));
            let mut o = Val {
                c: [0.; 4],
                vector: a.vector || b.vector,
            };
            for j in 0..4 {
                o.c[j] = match op {
                    '+' => a.c[j] + b.c[j],
                    '-' => a.c[j] - b.c[j],
                    '*' => a.c[j] * b.c[j],
                    _ => a.c[j] / b.c[j],
                }
            }
            o
        }
        Node::Call(k, args) => {
            let mut vs = [Val::scalar(0.); 4];
            for (i, a) in args.iter().enumerate() {
                vs[i] = eval(a, c)
            }
            if ["red", "green", "blue", "alpha"].contains(&k.as_str()) {
                return Val::scalar(
                    vs[0].c[match k.as_str() {
                        "red" => 0,
                        "green" => 1,
                        "blue" => 2,
                        _ => 3,
                    }],
                );
            }
            if k == "vec4" {
                return Val::vector([vs[0].c[0], vs[1].c[0], vs[2].c[0], vs[3].c[0]]);
            }
            if k == "sample" || k == "aux1" {
                return Val::vector(sample(
                    if k == "sample" {
                        c.src
                    } else {
                        c.aux.unwrap_or(c.src)
                    },
                    vs[0].c[0],
                    vs[1].c[0],
                    "clamp",
                ));
            }
            let mut o = Val {
                c: [0.; 4],
                vector: vs.iter().any(|v| v.vector),
            };
            for j in 0..4 {
                let (a, b, d) = (vs[0].c[j], vs[1].c[j], vs[2].c[j]);
                o.c[j] = match k.as_str() {
                    "sin" => a.sin(),
                    "pow" => a.powf(b),
                    "mix" => a + (b - a) * d,
                    "smoothstep" => {
                        let q = ((d - a) / (b - a)).clamp(0., 1.);
                        q * q * (3. - 2. * q)
                    }
                    "clamp" => a.max(b).min(d),
                    "fract" => a - a.floor(),
                    "floor" => a.floor(),
                    "noise" => noise(a, b, d),
                    "min" => a.min(b),
                    "max" => a.max(b),
                    "atan2" => a.atan2(b),
                    "sqrt" => a.sqrt(),
                    _ => 0.,
                }
            }
            o
        }
    }
}
pub fn apply(
    program: &Program,
    src: &Frame,
    aux: Option<&Frame>,
    params: [f32; 8],
    t: f32,
) -> Result<Frame, String> {
    let lim = crate::resource_limits::current();
    if lim
        .expr_max_ops
        .map_or(false, |cap| program.ops as u64 > cap)
    {
        return Err(format!(
            "expr_max_ops limit exceeded; set_limits expr_max_ops"
        ));
    }
    if src.w == 0
        || src.h == 0
        || src.d.len()
            != src
                .w
                .checked_mul(src.h)
                .and_then(|v| v.checked_mul(4))
                .ok_or("frame overflow")?
    {
        return Err("pixel_expr needs valid frame".into());
    }
    let cost = src
        .w
        .checked_mul(src.h)
        .and_then(|n| n.checked_mul(program.ops))
        .ok_or("pixel_expr operation count overflow")?;
    let limits = crate::resource_limits::current();
    if limits
        .expr_max_pixel_ops
        .map_or(false, |cap| cost as u64 > cap)
    {
        return Err(format!(
            "pixel_expr expr_max_pixel_ops cap {} exceeded ({cost}); set_limits expr_max_pixel_ops",
            limits.expr_max_pixel_ops.unwrap_or(u64::MAX)
        ));
    }
    crate::resource_limits::reserve(src.d.len() * 4)?;
    let mut out = Frame::try_new(src.w, src.h)?;
    let begin = std::time::Instant::now();
    let failed = std::sync::atomic::AtomicBool::new(false);
    let timed_out = std::sync::atomic::AtomicBool::new(false);
    crate::par::rows_mut(&mut out.d, src.w * 4, |y, row| {
        crate::guard::check();
        if limits
            .expr_timeout_ms
            .map_or(false, |cap| begin.elapsed().as_millis() > cap as u128)
        {
            timed_out.store(true, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        for (x, px) in row.chunks_mut(4).enumerate() {
            let v = eval(
                &program.root,
                &Ctx {
                    x,
                    y,
                    t,
                    src,
                    aux,
                    params,
                },
            );
            px.copy_from_slice(&v.c);
            if !v.vector {
                px[3] = src.d[(y * src.w + x) * 4 + 3];
            }
            if px.iter().any(|v| !v.is_finite()) {
                failed.store(true, std::sync::atomic::Ordering::Relaxed)
            }
        }
    });
    if timed_out.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(format!(
            "expr_timeout_ms exceeded ({} ms); set_limits expr_timeout_ms",
            limits.expr_timeout_ms.unwrap_or(u64::MAX)
        ));
    }
    if failed.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("invalid_argument: pixel_expr produced nonfinite output".into());
    }
    Ok(out)
}
