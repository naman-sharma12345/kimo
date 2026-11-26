// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/xml,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! DTD content-model parsing and XML validation.

//! Document Type Definitions: element declarations (content models) and required attributes, and
//! validation of an XML document against them (Structure ▸ Validate).

use std::collections::{HashMap, HashSet};

/// How much text parameter entities may add during expansion (a pass count is not an output
/// bound: each pass replaces whole strings, so references can inflate multiplicatively).
const MAX_EXPANDED_BYTES: usize = 1024 * 1024;
const MAX_DTD_BYTES: usize = 1024 * 1024;
const MAX_VALIDATION_XML_BYTES: usize = 16 * 1024 * 1024;
const MAX_VALIDATION_NODES: usize = 100_000;
const MAX_VALIDATION_DEPTH: usize = 256;
const MAX_VALIDATION_ATTRIBUTES: usize = 1_000_000;
const MAX_MATCH_WORK: usize = 20_000_000;
const MAX_MATCH_POSITIONS: usize = 5_000_000;
const MAX_MATCH_STATES: usize = 100_000;

/// A content particle: a name, a sequence, a choice, or one of those repeated (`?`, `*`, `+`).
#[derive(Clone, Debug, PartialEq)]
enum Cp {
    Name(String),
    Seq(Vec<Cp>),
    Choice(Vec<Cp>),
    Rep(Box<Cp>, char),
}

#[derive(Clone, Debug, PartialEq)]
enum Model {
    Empty,
    Any,
    /// `(#PCDATA | a | b)*`: text and these elements in any order.
    Mixed(Vec<String>),
    Children(Cp),
}

/// A parsed DTD.
#[derive(Clone, Debug, Default)]
pub struct Dtd {
    elements: Vec<(String, Model)>,
    /// Element → its `#REQUIRED` attributes.
    required: HashMap<String, Vec<String>>,
}

/// A problem found by validation: where (an element path such as `Root/story[2]`) and what.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Problem {
    pub path: String,
    pub message: String,
}

impl Dtd {
    /// Declared element names, in order.
    pub fn element_names(&self) -> impl Iterator<Item = &str> {
        self.elements.iter().map(|(n, _)| n.as_str())
    }

    fn model(&self, name: &str) -> Option<&Model> {
        self.elements.iter().find(|(n, _)| n == name).map(|(_, m)| m)
    }
}

/// Markup declarations (`<!…>`) with comments removed and quoted strings kept whole.
fn declarations(src: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find("<!") {
        rest = &rest[i..];
        if rest.starts_with("<!--") {
            let Some(j) = rest.find("-->") else {
                return Err("unterminated DTD comment".into());
            };
            rest = &rest[j + 3..];
            continue;
        }
        let mut quote = None;
        let mut end = None;
        for (k, c) in rest.char_indices().skip(2) {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (None, '"' | '\'') => quote = Some(c),
                (None, '>') => {
                    end = Some(k);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            return Err("unterminated DTD declaration".into());
        };
        out.push(rest[2..end].to_string());
        rest = &rest[end + 1..];
    }
    Ok(out)
}

fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_whitespace() || "(),|?*+".contains(c) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            if !c.is_whitespace() {
                out.push(c.to_string());
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// How deeply groups may nest in a content model; parsing, matching (`ends`) and printing (`show`)
/// recurse once per level.
const MAX_CONTENT_MODEL_DEPTH: usize = 64;

/// Parse a content particle from `t[*i..]`, inside `depth` enclosing groups.
fn particle(t: &[String], i: &mut usize, depth: usize) -> Result<Cp, String> {
    let base = match t.get(*i).map(String::as_str) {
        Some("(") => {
            if depth >= MAX_CONTENT_MODEL_DEPTH {
                return Err(format!("content model nested more than {MAX_CONTENT_MODEL_DEPTH} levels"));
            }
            *i += 1;
            let mut items = vec![particle(t, i, depth + 1)?];
            let mut sep = None;
            loop {
                match t.get(*i).map(String::as_str) {
                    Some(")") => {
                        *i += 1;
                        break;
                    }
                    Some(s @ ("," | "|")) => {
                        if sep.is_some_and(|p| p != s) {
                            return Err("mixed `,` and `|` in one group".into());
                        }
                        sep = Some(if s == "," { "," } else { "|" });
                        *i += 1;
                        items.push(particle(t, i, depth + 1)?);
                    }
                    other => return Err(format!("unexpected `{}` in a content model", other.unwrap_or("end"))),
                }
            }
            if sep == Some("|") { Cp::Choice(items) } else { Cp::Seq(items) }
        }
        Some(n) if !"),|?*+".contains(n) => {
            *i += 1;
            Cp::Name(n.to_string())
        }
        other => return Err(format!("unexpected `{}` in a content model", other.unwrap_or("end"))),
    };
    Ok(match t.get(*i).map(String::as_str) {
        Some(r @ ("?" | "*" | "+")) => {
            *i += 1;
            Cp::Rep(Box::new(base), r.chars().next().unwrap_or('*'))
        }
        _ => base,
    })
}

fn parse_model(s: &str) -> Result<Model, String> {
    let s = s.trim();
    match s {
        "EMPTY" => return Ok(Model::Empty),
        "ANY" => return Ok(Model::Any),
        _ => {}
    }
    if s.contains("#PCDATA") {
        let t = tokens(s);
        if t.first().map(String::as_str) != Some("(") || t.get(1).map(String::as_str) != Some("#PCDATA") {
            return Err("malformed mixed content model".into());
        }
        let mut i = 2;
        let mut allowed = Vec::new();
        if t.get(i).map(String::as_str) == Some(")") {
            i += 1;
            if t.get(i).map(String::as_str) == Some("*") {
                i += 1;
            }
            if i != t.len() {
                return Err("unexpected token after mixed content model".into());
            }
            return Ok(Model::Mixed(allowed));
        }
        loop {
            if t.get(i).map(String::as_str) != Some("|") {
                return Err("expected `|` in mixed content model".into());
            }
            i += 1;
            let Some(name) = t.get(i).filter(|name| !"(),|?*+".contains(name.as_str())) else {
                return Err("expected element name in mixed content model".into());
            };
            if allowed.contains(name) {
                return Err(format!("duplicate `{name}` in mixed content model"));
            }
            allowed.push(name.clone());
            i += 1;
            match t.get(i).map(String::as_str) {
                Some("|") => continue,
                Some(")") => {
                    i += 1;
                    break;
                }
                _ => return Err("expected `|` or `)` in mixed content model".into()),
            }
        }
        if t.get(i).map(String::as_str) != Some("*") || i + 1 != t.len() {
            return Err("mixed content with element names must end in `*`".into());
        }
        return Ok(Model::Mixed(allowed));
    }
    let t = tokens(s);
    let mut i = 0;
    let cp = particle(&t, &mut i, 0)?;
    if i != t.len() {
        return Err(format!("unexpected `{}` after the content model", t[i]));
    }
    Ok(Model::Children(cp))
}

/// How deeply parameter entities may nest: a reference in the DTD is level 1, a reference in its
/// value level 2, and so on.
const MAX_PARAMETER_ENTITY_DEPTH: usize = 64;

/// Expand `%name;` references, one level per round, until a round changes nothing.
fn expand_parameter_entities(src: &str, ents: &[(String, String)]) -> Result<String, String> {
    let mut entities = HashMap::new();
    for (name, value) in ents {
        entities.entry(name.as_str()).or_insert(value.as_str());
    }
    let limit = src.len().saturating_add(MAX_EXPANDED_BYTES);
    let append = |out: &mut String, value: &str| -> Result<(), String> {
        if value.len() > limit.saturating_sub(out.len()) {
            return Err(format!("parameter entities expand the DTD beyond {limit} bytes"));
        }
        out.push_str(value);
        Ok(())
    };
    let too_deep = || format!("parameter entities refer to themselves or nest deeper than {MAX_PARAMETER_ENTITY_DEPTH} levels");
    let mut text = src.to_string();
    // The round after the deepest level changes nothing, hence one more round than levels.
    for _ in 0..=MAX_PARAMETER_ENTITY_DEPTH {
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        let mut changed = false;
        loop {
            let mut special = None;
            for (i, c) in rest.char_indices() {
                if c == '%' {
                    special = Some((i, false));
                    break;
                }
                if c == '<' && rest[i..].starts_with("<!--") {
                    special = Some((i, true));
                    break;
                }
            }
            let Some((i, is_comment)) = special else {
                append(&mut out, rest)?;
                rest = "";
                break;
            };
            if is_comment {
                append(&mut out, &rest[..i])?;
                let comment_tail = &rest[i..];
                let Some(end) = comment_tail.find("-->") else {
                    return Err("unterminated DTD comment".into());
                };
                append(&mut out, &comment_tail[..end + 3])?;
                rest = &comment_tail[end + 3..];
                continue;
            }
            let (prefix, reference) = rest.split_at(i);
            append(&mut out, prefix)?;
            // `find` points to an ASCII `%`, so both split positions are UTF-8 boundaries.
            rest = reference.split_at(1).1;
            // A name has no whitespace and no `%` (`parse` ends a name at whitespace), whatever its
            // length: the search for the closing `;` stops at the first of those, so the searches of
            // a round never overlap and a round stays linear in the text.
            let end =
                rest.bytes().position(|b| matches!(b, b';' | b'%') || b.is_ascii_whitespace()).filter(|&k| rest.as_bytes().get(k) == Some(&b';'));
            if let Some(end) = end
                && let Some(value) = rest.get(..end).and_then(|name| entities.get(name))
            {
                // A value that is its own reference (`<!ENTITY % a "%a;">`) never gets anywhere.
                if reference.get(..end + 2) == Some(*value) {
                    return Err(too_deep());
                }
                append(&mut out, value)?;
                changed = true;
                rest = rest.split_at(end + 1).1;
                continue;
            }
            append(&mut out, "%")?;
        }
        append(&mut out, rest)?;
        text = out;
        if !changed {
            return Ok(text);
        }
    }
    Err(too_deep())
}

/// Parse a DTD (its element and attribute-list declarations; parameter entities are expanded).
pub fn parse(src: &str) -> Result<Dtd, String> {
    if src.len() > MAX_DTD_BYTES {
        return Err(format!("DTD exceeds the {MAX_DTD_BYTES} byte parsing limit"));
    }
    // Parameter entities first, then expand references and read the declarations again.
    let mut ents: Vec<(String, String)> = Vec::new();
    for d in declarations(src)? {
        if let Some(rest) = d.strip_prefix("ENTITY") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('%') {
                let rest = rest.trim_start();
                let name: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
                let val = rest[name.len()..].trim();
                let q = val.chars().next().unwrap_or('"');
                if let Some(v) = val.strip_prefix(q).and_then(|v| v.find(q).map(|e| &v[..e])) {
                    ents.push((name, v.to_string()));
                }
            }
        }
    }
    let text = expand_parameter_entities(src, &ents)?;
    let mut dtd = Dtd::default();
    for d in declarations(&text)? {
        if let Some(rest) = d.strip_prefix("ELEMENT") {
            let rest = rest.trim_start();
            let name: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            let model = parse_model(&rest[name.len()..]).map_err(|e| format!("<!ELEMENT {name}>: {e}"))?;
            dtd.elements.push((name, model));
        } else if let Some(rest) = d.strip_prefix("ATTLIST") {
            // name (attr type default)*; only `#REQUIRED` matters here.
            let mut words = Vec::new();
            let mut cur = String::new();
            let mut quote = None;
            let mut group = 0;
            for c in rest.chars() {
                match (quote, c) {
                    (Some(q), c) if c == q => {
                        quote = None;
                        cur.push(c);
                    }
                    (Some(_), c) => cur.push(c),
                    (None, '"' | '\'') => {
                        quote = Some(c);
                        cur.push(c);
                    }
                    (None, '(') => {
                        group += 1;
                        cur.push(c);
                    }
                    (None, ')') => {
                        group -= 1;
                        cur.push(c);
                    }
                    (None, c) if c.is_whitespace() && group == 0 => {
                        if !cur.is_empty() {
                            words.push(std::mem::take(&mut cur));
                        }
                    }
                    (None, c) => cur.push(c),
                }
            }
            if !cur.is_empty() {
                words.push(cur);
            }
            let Some((el, attrs)) = words.split_first() else { continue };
            let mut k = 0;
            while k < attrs.len() {
                let name = &attrs[k];
                let Some(ty) = attrs.get(k + 1) else {
                    return Err(format!("incomplete ATTLIST for `{el}`"));
                };
                k += 2;
                // NOTATION is a two-token type: the keyword followed by its enumeration.
                if ty == "NOTATION" {
                    if attrs.get(k).is_none_or(|group| !group.starts_with('(') || !group.ends_with(')')) {
                        return Err(format!("malformed NOTATION type for `{name}`"));
                    }
                    k += 1;
                }
                let Some(default) = attrs.get(k).map(String::as_str) else {
                    return Err(format!("missing default declaration for `{name}`"));
                };
                k += 1;
                if default == "#REQUIRED" {
                    dtd.required.entry(el.clone()).or_default().push(name.clone());
                } else if default == "#FIXED" {
                    if k >= attrs.len() {
                        return Err(format!("missing #FIXED value for `{name}`"));
                    }
                    k += 1;
                }
            }
        }
    }
    if dtd.elements.is_empty() {
        return Err("no element declarations".into());
    }
    Ok(dtd)
}

/// What `ends` has worked out for one sequence of children: the end positions of a group (by the
/// address of its node, which lives as long as the model) from a position.
type EndsMemo = HashMap<(usize, usize), Vec<usize>>;

#[derive(Default)]
struct MatchBudget {
    work: usize,
    positions: usize,
    states: usize,
    exhausted: bool,
    reported: bool,
}

impl MatchBudget {
    fn spend(&mut self, work: usize, positions: usize) -> Result<(), &'static str> {
        if self.exhausted {
            return Err("content model matching exceeded its resource budget");
        }
        self.work = self.work.saturating_add(work);
        self.positions = self.positions.saturating_add(positions);
        if self.work > MAX_MATCH_WORK {
            self.exhausted = true;
            return Err("content model matching exceeded its work limit");
        }
        if self.positions > MAX_MATCH_POSITIONS {
            self.exhausted = true;
            return Err("content model matching exceeded its memory limit");
        }
        Ok(())
    }

    fn retain_state(&mut self) -> Result<(), &'static str> {
        self.states = self.states.saturating_add(1);
        if self.states > MAX_MATCH_STATES {
            self.exhausted = true;
            return Err("content model matching exceeded its state limit");
        }
        Ok(())
    }
}

/// End positions after `cp` matches a prefix of `seq[i..]`.
///
/// The positions of each group are worked out once per start position (`memo`): nested repetitions
/// such as `((a*)*)*` ask the inner group for the same positions again and again, which is
/// exponential in their depth without it.
fn ends(cp: &Cp, seq: &[&str], i: usize, memo: &mut EndsMemo, budget: &mut MatchBudget) -> Result<Vec<usize>, &'static str> {
    budget.spend(1, 0)?;
    if let Cp::Name(n) = cp {
        if seq.get(i) == Some(&n.as_str()) {
            budget.spend(0, 1)?;
            return Ok(vec![i + 1]);
        }
        return Ok(vec![]);
    }
    let key = (std::ptr::from_ref(cp) as usize, i);
    if let Some(done) = memo.get(&key) {
        budget.spend(0, done.len())?;
        return Ok(done.clone());
    }
    let mut out: Vec<usize> = match cp {
        Cp::Name(_) => vec![],
        // Keep the positions after each item a set: as a list of every path, `(a?, a?, …)` doubles per item.
        Cp::Seq(items) => {
            let mut at = vec![i];
            for c in items {
                let mut next = Vec::new();
                for p in at {
                    let found = ends(c, seq, p, memo, budget)?;
                    budget.spend(found.len(), found.len())?;
                    next.extend(found);
                }
                next.sort_unstable();
                next.dedup();
                at = next;
            }
            at
        }
        Cp::Choice(items) => {
            let mut all = Vec::new();
            for c in items {
                let found = ends(c, seq, i, memo, budget)?;
                budget.spend(found.len(), found.len())?;
                all.extend(found);
            }
            all
        }
        Cp::Rep(c, r) => {
            let mut all = if *r == '+' { vec![] } else { vec![i] };
            let mut seen: HashSet<usize> = all.iter().copied().collect();
            let mut frontier = ends(c, seq, i, memo, budget)?;
            budget.spend(frontier.len(), frontier.len())?;
            if *r == '?' {
                all.extend(frontier);
            } else {
                while !frontier.is_empty() {
                    let mut new = Vec::new();
                    for p in frontier.iter().copied() {
                        budget.spend(1, 0)?;
                        if seen.insert(p) {
                            budget.spend(0, 1)?;
                            new.push(p);
                        }
                    }
                    budget.spend(new.len(), new.len())?;
                    all.extend(&new);
                    let mut next = Vec::new();
                    let mut next_seen = HashSet::new();
                    for p in new.into_iter().filter(|&p| p > i) {
                        for found in ends(c, seq, p, memo, budget)? {
                            budget.spend(1, 0)?;
                            if !seen.contains(&found) && next_seen.insert(found) {
                                budget.spend(0, 1)?;
                                next.push(found);
                            }
                        }
                    }
                    // Two paths to one position are one position: kept apart they multiply per round.
                    next.sort_unstable();
                    next.dedup();
                    frontier = next;
                }
            }
            all
        }
    };
    out.sort_unstable();
    out.dedup();
    budget.spend(out.len(), out.len())?;
    budget.retain_state()?;
    memo.insert(key, out.clone());
    Ok(out)
}

fn show(cp: &Cp) -> String {
    match cp {
        Cp::Name(n) => n.clone(),
        Cp::Seq(v) => format!("({})", v.iter().map(show).collect::<Vec<_>>().join(", ")),
        Cp::Choice(v) => format!("({})", v.iter().map(show).collect::<Vec<_>>().join(" | ")),
        Cp::Rep(c, r) => format!("{}{r}", show(c)),
    }
}

/// An element of the document being validated.
struct Node {
    name: String,
    attrs: Vec<String>,
    children: Vec<Node>,
    text: bool,
    content: bool,
}

fn bounded_node(e: &quick_xml::events::BytesStart<'_>, depth: usize, nodes: &mut usize, attributes: &mut usize) -> Result<Node, String> {
    if depth >= MAX_VALIDATION_DEPTH {
        return Err(format!("XML nesting exceeds the {MAX_VALIDATION_DEPTH} level validation limit"));
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_VALIDATION_NODES {
        return Err(format!("XML exceeds the {MAX_VALIDATION_NODES} node validation limit"));
    }
    let mut attrs = Vec::new();
    for attr in e.attributes().with_checks(true) {
        let attr = attr.map_err(|err| format!("invalid attribute: {err}"))?;
        attrs.push(String::from_utf8_lossy(attr.key.as_ref()).to_string());
    }
    *attributes = attributes.saturating_add(attrs.len());
    if *attributes > MAX_VALIDATION_ATTRIBUTES {
        return Err(format!("XML exceeds the {MAX_VALIDATION_ATTRIBUTES} attribute validation limit"));
    }
    Ok(Node { name: String::from_utf8_lossy(e.name().as_ref()).to_string(), attrs, children: vec![], text: false, content: false })
}

fn read_tree(xml: &str) -> Result<Node, String> {
    use quick_xml::events::Event;
    if xml.len() > MAX_VALIDATION_XML_BYTES {
        return Err(format!("XML exceeds the {} byte validation limit", MAX_VALIDATION_XML_BYTES));
    }
    let mut r = quick_xml::Reader::from_str(xml);
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    let mut nodes = 0usize;
    let mut attributes = 0usize;
    loop {
        match r.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) => {
                if stack.is_empty() && root.is_some() {
                    return Err("multiple root elements".into());
                }
                let n = bounded_node(&e, stack.len(), &mut nodes, &mut attributes)?;
                if let Some(parent) = stack.last_mut() {
                    parent.content = true;
                }
                stack.push(n);
            }
            Event::Empty(e) => {
                if stack.is_empty() && root.is_some() {
                    return Err("multiple root elements".into());
                }
                let n = bounded_node(&e, stack.len(), &mut nodes, &mut attributes)?;
                match stack.last_mut() {
                    Some(p) => {
                        p.content = true;
                        p.children.push(n);
                    }
                    None => root = Some(n),
                }
            }
            Event::End(e) => {
                let n = stack.pop().ok_or("unbalanced end tag")?;
                if n.name.as_bytes() != e.name().as_ref() {
                    return Err("mismatched end tag".into());
                }
                match stack.last_mut() {
                    Some(p) => p.children.push(n),
                    None => root = Some(n),
                }
            }
            Event::Text(t) => {
                if let Some(p) = stack.last_mut() {
                    p.content = true;
                    if !t.iter().all(u8::is_ascii_whitespace) {
                        p.text = true;
                    }
                } else if !t.iter().all(u8::is_ascii_whitespace) {
                    return Err("text outside the root element".into());
                }
            }
            Event::CData(_) => {
                if let Some(p) = stack.last_mut() {
                    p.text = true;
                    p.content = true;
                }
            }
            Event::GeneralRef(reference) => {
                let name = reference.decode().map_err(|e| e.to_string())?;
                let accepted = if let Some(character) = reference.resolve_char_ref().map_err(|e| e.to_string())? {
                    crate::is_xml_char(character)
                } else {
                    matches!(name.as_ref(), "amp" | "lt" | "gt" | "apos" | "quot")
                };
                if !accepted {
                    return Err(format!("unsupported general entity reference `&{name};`"));
                }
                if let Some(p) = stack.last_mut() {
                    p.text = true;
                    p.content = true;
                } else {
                    return Err("reference outside the root element".into());
                }
            }
            Event::Comment(_) | Event::PI(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.content = true;
                }
            }
            Event::Eof => {
                if !stack.is_empty() {
                    return Err("unclosed element".into());
                }
                return root.ok_or_else(|| "no root element".into());
            }
            _ => {}
        }
    }
}

fn check(dtd: &Dtd, n: &Node, path: &str, out: &mut Vec<Problem>, budget: &mut MatchBudget) {
    let mut problem = |m: String| out.push(Problem { path: path.to_string(), message: m });
    match dtd.model(&n.name) {
        None => problem(format!("`{}` isn't declared in the DTD", n.name)),
        Some(Model::Any) => {}
        Some(Model::Empty) => {
            if n.content {
                problem(format!("`{}` should be empty", n.name));
            }
        }
        Some(Model::Mixed(allowed)) => {
            for c in &n.children {
                if !allowed.contains(&c.name) {
                    problem(format!("`{}` isn't allowed in `{}`", c.name, n.name));
                }
            }
        }
        Some(Model::Children(cp)) => {
            if n.text {
                problem(format!("`{}` can't contain text", n.name));
            }
            let seq: Vec<&str> = n.children.iter().map(|c| c.name.as_str()).collect();
            let matched = ends(cp, &seq, 0, &mut EndsMemo::new(), budget);
            match matched {
                Ok(positions) if positions.contains(&seq.len()) => {}
                Ok(_) => {
                    let got = if seq.is_empty() { "nothing".to_string() } else { seq.join(", ") };
                    problem(format!("`{}` contains {got}; the DTD asks for {}", n.name, show(cp)));
                }
                Err(error) => {
                    if !budget.reported {
                        problem(format!("`{}`: {error}", n.name));
                        budget.reported = true;
                    }
                }
            }
        }
    }
    for a in dtd.required.get(&n.name).into_iter().flatten() {
        if !n.attrs.contains(a) {
            problem(format!("`{}` is missing the required attribute `{a}`", n.name));
        }
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for c in &n.children {
        let k = seen.entry(c.name.as_str()).or_default();
        *k += 1;
        check(dtd, c, &format!("{path}/{}[{k}]", c.name), out, budget);
    }
}

/// Validate `xml` against `dtd`; an empty list when it's valid.
///
/// Validation rejects input above the internal byte, depth, node, attribute and content-model
/// matching budgets. Only predefined entities and numeric character references are resolved; a
/// general entity reference is reported as unsupported rather than expanded or fetched.
pub fn validate(dtd: &Dtd, xml: &str) -> Vec<Problem> {
    match read_tree(xml) {
        Ok(root) => {
            let mut out = Vec::new();
            let path = root.name.clone();
            check(dtd, &root, &path, &mut out, &mut MatchBudget::default());
            out
        }
        Err(e) => vec![Problem { path: String::new(), message: format!("not well-formed XML: {e}") }],
    }
}

