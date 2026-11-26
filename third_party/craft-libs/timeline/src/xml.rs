// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/timeline,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! XML for the xmeml and FCPXML modules: bounded reading and pretty writing through craft-xml,
//! with the small node view their readers are written against.

pub(crate) use craft_xml::esc_attr as escape_attr;
use craft_xml::{Doctype, El, Limits, Options, UnknownEntity};

/// The reader's limits: craft-xml's defaults, which stop a document at 256 levels of nesting
/// (real timelines nest a few levels per nested sequence), 2 000 000 nodes and 256 MiB.
pub const LIMITS: Limits = Limits::DEFAULT;

/// Read a document strictly. A plain `<!DOCTYPE xmeml>` is skipped; a declaration with an
/// internal subset is refused, and so is a reference to an entity XML does not define, so
/// nothing in a document can expand beyond its own size.
pub(crate) fn parse(text: &str) -> Result<El, String> {
    let options =
        Options { doctype: Doctype::RejectInternalSubset, unknown_entity: UnknownEntity::Error, ..Options::default() }.strict().with_limits(LIMITS);
    options.parse(text.as_bytes()).map_err(|e| e.to_string())
}

/// An element of a parsed document.
#[derive(Clone, Copy)]
pub(crate) struct Node<'a>(pub &'a El);

impl<'a> Node<'a> {
    pub fn name(self) -> &'a str {
        self.0.local()
    }
    pub fn has_tag_name(self, name: &str) -> bool {
        self.0.local() == name
    }
    pub fn attribute(self, name: &str) -> Option<&'a str> {
        self.0.attr(name)
    }
    /// Child elements.
    pub fn children(self) -> impl Iterator<Item = Node<'a>> {
        self.0.elements().map(Node)
    }
    /// This element and every element below it, in document order.
    pub fn descendants(self) -> impl Iterator<Item = Node<'a>> {
        std::iter::once(self).chain(self.0.descendants().map(Node))
    }
    /// The text this element starts with, if it starts with text.
    pub fn text(self) -> Option<&'a str> {
        match self.0.children.first() {
            Some(craft_xml::Node::Text(t) | craft_xml::Node::CData(t)) => Some(t.as_str()),
            _ => None,
        }
    }
    /// A number that is this element's alone while the document is alive.
    pub fn id(self) -> usize {
        std::ptr::from_ref(self.0) as usize
    }
}

pub(crate) fn child<'a>(n: Node<'a>, name: &str) -> Option<Node<'a>> {
    n.children().find(|c| c.name() == name)
}

pub(crate) fn children<'a>(n: Node<'a>, name: &'static str) -> impl Iterator<Item = Node<'a>> {
    n.children().filter(move |c| c.name() == name)
}

pub(crate) fn elements<'a>(n: Node<'a>) -> impl Iterator<Item = Node<'a>> {
    n.children()
}

/// Trimmed text of a node's direct text content.
pub(crate) fn text(n: Node<'_>) -> &str {
    n.text().map(str::trim).unwrap_or("")
}

/// Text of the element at `path` below `n`.
pub(crate) fn path_text<'a>(n: Node<'a>, path: &[&str]) -> Option<&'a str> {
    let mut cur = n;
    for p in path {
        cur = child(cur, p)?;
    }
    Some(text(cur))
}

pub(crate) fn child_text<'a>(n: Node<'a>, name: &str) -> Option<&'a str> {
    child(n, name).map(text)
}

pub(crate) fn child_i64(n: Node<'_>, name: &str) -> Option<i64> {
    child_text(n, name).and_then(|t| t.parse::<i64>().ok().or_else(|| t.parse::<f64>().ok().map(|f| f.round() as i64)))
}

pub(crate) fn child_f64(n: Node<'_>, name: &str) -> Option<f64> {
    child_text(n, name).and_then(|t| t.parse::<f64>().ok())
}

pub(crate) fn child_bool(n: Node<'_>, name: &str) -> Option<bool> {
    child_text(n, name).map(|t| t.eq_ignore_ascii_case("true") || t == "1")
}

pub(crate) fn bool_str(b: bool) -> &'static str {
    if b { "TRUE" } else { "FALSE" }
}

/// Format an f64 compactly (no trailing zeros, no exponent for normal ranges).
pub(crate) fn num(v: f64) -> String {
    if v == v.round() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    if !v.is_finite() {
        return "0".into();
    }
    // Shortest representation that parses back to the same f64.
    format!("{v}")
}

/// Pretty-printing XML writer (two-space indent, one element per line).
pub(crate) struct XmlWriter(craft_xml::Writer);

impl XmlWriter {
    pub fn new(doctype: Option<&str>) -> Self {
        let mut w = craft_xml::Writer::pretty("  ");
        w.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        if let Some(d) = doctype {
            w.doctype(d);
        }
        Self(w)
    }
    pub fn open(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.0.open(name, attrs);
    }
    pub fn close(&mut self) {
        self.0.close();
    }
    pub fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.0.empty(name, attrs);
    }
    /// `<name attrs>value</name>`.
    pub fn text_attrs(&mut self, name: &str, attrs: &[(&str, &str)], value: &str) {
        self.0.leaf(name, attrs, value);
    }
    pub fn text(&mut self, name: &str, value: impl std::fmt::Display) {
        self.text_attrs(name, &[], &value.to_string());
    }
    pub fn finish(self) -> String {
        self.0.finish()
    }
}

