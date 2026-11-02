//! Resolved schema model, serialized as compact JSON into the HTML page.
//!
//! Field names are deliberately short because NeTEx produces tens of
//! thousands of components and the JSON is embedded in a single file.

use serde::Serialize;

/// Reference to another component: an index into the component list, or a
/// display name for built-in XSD types and anything that could not be resolved.
#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Ref {
    Idx(usize),
    Name(String),
}

impl Ref {
    pub fn idx(&self) -> Option<usize> {
        match self {
            Ref::Idx(i) => Some(*i),
            Ref::Name(_) => None,
        }
    }
}

/// Occurrence constraints; `None` means the XSD default of 1, `x = -1` means unbounded.
#[derive(Serialize, Clone, Copy, Default, Debug)]
pub struct Occ {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub m: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Particle {
    /// sequence / choice / all
    Model {
        s: &'static str,
        i: Vec<Particle>,
        #[serde(flatten)]
        o: Occ,
    },
    Element {
        e: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        r: Option<Ref>,
        #[serde(skip_serializing_if = "Option::is_none")]
        t: Option<Ref>,
        #[serde(flatten)]
        o: Occ,
        #[serde(skip_serializing_if = "Option::is_none")]
        d: Option<String>,
    },
    Group {
        g: Ref,
        #[serde(flatten)]
        o: Occ,
    },
    Any {
        any: String,
        #[serde(flatten)]
        o: Occ,
    },
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum AttrUse {
    Attr {
        n: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        r: Option<Ref>,
        #[serde(skip_serializing_if = "Option::is_none")]
        t: Option<Ref>,
        /// "r" = required, "p" = prohibited, absent = optional
        #[serde(skip_serializing_if = "Option::is_none")]
        u: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        df: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fx: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        d: Option<String>,
    },
    Group {
        g: Ref,
    },
}

/// A schema component. `k` is one of: ct (complexType), st (simpleType),
/// el (element), gr (group), ag (attributeGroup), at (attribute).
#[derive(Serialize, Clone, Default, Debug)]
pub struct Component {
    pub k: &'static str,
    pub n: String,
    /// index into the file list
    pub f: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    /// anonymous (inline) type; `own` is the component it is defined in
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anon: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own: Option<usize>,

    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub abs: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mx: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nil: bool,
    /// complexType with simpleContent
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sc: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<Ref>,
    /// derivation: "e" = extension, "r" = restriction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub der: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<Particle>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub at: Vec<AttrUse>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anya: bool,

    /// type of an element / attribute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<Ref>,
    /// substitution group head
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sg: Option<Ref>,

    /// enumerations: (value, documentation)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub en: Vec<(String, String)>,
    /// other facets: (facet, value)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fa: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub li: Option<Ref>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub un: Vec<Ref>,

    /// reverse links, filled in after resolution
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ub: Vec<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sub: Vec<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sgm: Vec<usize>,
}

impl Particle {
    /// All component references contained in this particle tree.
    pub fn refs(&self, out: &mut Vec<usize>) {
        match self {
            Particle::Model { i, .. } => i.iter().for_each(|p| p.refs(out)),
            Particle::Element { r, t, .. } => out.extend(r.iter().chain(t).filter_map(Ref::idx)),
            Particle::Group { g, .. } => out.extend(g.idx()),
            Particle::Any { .. } => {}
        }
    }
}

impl Component {
    /// References that count as "uses" (everything except derivation and substitution).
    pub fn uses(&self) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(p) = &self.p {
            p.refs(&mut out);
        }
        for a in &self.at {
            match a {
                AttrUse::Attr { r, t, .. } => out.extend(r.iter().chain(t).filter_map(Ref::idx)),
                AttrUse::Group { g } => out.extend(g.idx()),
            }
        }
        out.extend(self.t.iter().chain(&self.li).chain(&self.un).filter_map(Ref::idx));
        out
    }
}
