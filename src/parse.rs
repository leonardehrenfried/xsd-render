//! Loading XSD files (following include/import/redefine) and resolving
//! all global and anonymous components into the [`Component`] model.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use roxmltree::{Document, Node};

use crate::examples::Globals;
use crate::model::{AttrUse, Component, Occ, Particle, Ref};

pub const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema";

type N<'a> = Node<'a, 'static>;

pub struct SchemaFile {
    pub rel: String,
    pub doc: Document<'static>,
    pub tns: Option<String>,
    /// included without own targetNamespace, adopting the includer's
    pub chameleon: bool,
}

fn xs_children<'a>(n: N<'a>) -> impl Iterator<Item = N<'a>> {
    n.children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(XSD_NS))
}

/// Loads the entry schemas and everything they include, import or redefine.
pub fn load(entries: &[PathBuf]) -> Result<Vec<SchemaFile>> {
    let first = entries[0]
        .canonicalize()
        .with_context(|| format!("cannot open {}", entries[0].display()))?;
    let base = first.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut seen = HashSet::new();
    let mut queue: VecDeque<(PathBuf, Option<String>)> =
        entries.iter().map(|p| (p.clone(), None)).collect();
    let mut out = Vec::new();

    while let Some((path, inherited)) = queue.pop_front() {
        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("warning: cannot open {}: {e}", path.display());
                continue;
            }
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        // Documents borrow their input; the schema set lives for the whole run.
        let text: &'static str = Box::leak(text.into_boxed_str());
        let opts = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        let doc = Document::parse_with_options(text, opts)
            .with_context(|| format!("cannot parse {}", path.display()))?;

        let root = doc.root_element();
        let own = root.attribute("targetNamespace").map(str::to_string);
        let chameleon = own.is_none() && inherited.is_some();
        let tns = own.or(inherited);
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();

        for ch in xs_children(root) {
            let Some(loc) = ch.attribute("schemaLocation") else {
                continue;
            };
            if loc.starts_with("http://") || loc.starts_with("https://") {
                eprintln!("warning: skipping remote schema {loc}");
                continue;
            }
            match ch.tag_name().name() {
                "include" | "redefine" | "override" => queue.push_back((dir.join(loc), tns.clone())),
                "import" => queue.push_back((dir.join(loc), None)),
                _ => {}
            }
        }

        let rel = path
            .strip_prefix(&base)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        out.push(SchemaFile {
            rel,
            doc,
            tns,
            chameleon,
        });
    }
    Ok(out)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Space {
    Type,
    Element,
    Group,
    AttrGroup,
    Attribute,
}

pub struct Model {
    pub components: Vec<Component>,
    pub namespaces: Vec<String>,
    pub main_ns: usize,
    /// (relative path, namespace index)
    pub files: Vec<(String, usize)>,
    pub unresolved: BTreeMap<String, usize>,
    pub duplicates: usize,
    pub globals: Globals,
}

struct Builder<'a> {
    files: &'a [SchemaFile],
    index: HashMap<(Space, String, String), usize>,
    comps: Vec<Component>,
    prefixes: HashMap<String, String>,
    main_ns: String,
    unresolved: BTreeMap<String, usize>,
}

pub fn build(files: &[SchemaFile]) -> Model {
    let main_ns = files
        .first()
        .and_then(|f| f.tns.clone())
        .unwrap_or_default();

    // Preferred prefix per namespace: the first one declared anywhere.
    let mut prefixes = HashMap::new();
    prefixes.insert(XSD_NS.to_string(), "xsd".to_string());
    for f in files {
        for ns in f.doc.root_element().namespaces() {
            if let Some(p) = ns.name() {
                prefixes
                    .entry(ns.uri().to_string())
                    .or_insert_with(|| p.to_string());
            }
        }
    }

    let mut b = Builder {
        files,
        index: HashMap::new(),
        comps: Vec::new(),
        prefixes,
        main_ns,
        unresolved: BTreeMap::new(),
    };

    // Pass 1: register all global components so references can be resolved.
    let mut duplicates = 0;
    let mut pending = Vec::new();
    for (fi, f) in files.iter().enumerate() {
        let tops = xs_children(f.doc.root_element()).flat_map(|c| match c.tag_name().name() {
            "redefine" | "override" => xs_children(c).collect::<Vec<_>>(),
            _ => vec![c],
        });
        for n in tops {
            let (space, k) = match n.tag_name().name() {
                "complexType" => (Space::Type, "ct"),
                "simpleType" => (Space::Type, "st"),
                "element" => (Space::Element, "el"),
                "group" => (Space::Group, "gr"),
                "attributeGroup" => (Space::AttrGroup, "ag"),
                "attribute" => (Space::Attribute, "at"),
                _ => continue,
            };
            let Some(name) = n.attribute("name") else {
                continue;
            };
            let ns = f.tns.clone().unwrap_or_default();
            let key = (space, ns.clone(), name.to_string());
            if b.index.contains_key(&key) {
                duplicates += 1;
                continue;
            }
            let idx = b.comps.len();
            b.index.insert(key, idx);
            b.comps.push(Component {
                k,
                n: b.display(&ns, name),
                f: fi,
                ..Default::default()
            });
            pending.push((idx, fi, n));
        }
    }

    // Pass 2: resolve their content. Anonymous types are appended as we go.
    for (idx, fi, n) in pending {
        b.build_comp(idx, n, fi);
    }

    link_back(&mut b.comps);

    let mut namespaces: Vec<String> = Vec::new();
    let mut ns_idx = |ns: &str| match namespaces.iter().position(|x| x == ns) {
        Some(i) => i,
        None => {
            namespaces.push(ns.to_string());
            namespaces.len() - 1
        }
    };
    let main = ns_idx(&b.main_ns);
    let file_list = files
        .iter()
        .map(|f| (f.rel.clone(), ns_idx(f.tns.as_deref().unwrap_or(""))))
        .collect();

    let mut globals = Globals { elements: HashMap::new(), types: HashMap::new() };
    for ((space, ns, local), &i) in &b.index {
        let map = match space {
            Space::Element => &mut globals.elements,
            Space::Type => &mut globals.types,
            _ => continue,
        };
        map.insert((ns.clone(), local.clone()), i);
    }

    Model {
        components: b.comps,
        namespaces,
        main_ns: main,
        files: file_list,
        unresolved: b.unresolved,
        duplicates,
        globals,
    }
}

fn doc(n: N) -> Option<String> {
    let mut parts = Vec::new();
    for a in xs_children(n).filter(|c| c.tag_name().name() == "annotation") {
        for d in xs_children(a).filter(|c| c.tag_name().name() == "documentation") {
            let text: Vec<&str> = d
                .descendants()
                .filter(|x| x.is_text())
                .filter_map(|x| x.text())
                .flat_map(str::split_whitespace)
                .collect();
            if !text.is_empty() {
                parts.push(text.join(" "));
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn occ(n: N) -> Occ {
    let m = n
        .attribute("minOccurs")
        .and_then(|v| v.trim().parse().ok())
        .filter(|&v| v != 1);
    let x = n
        .attribute("maxOccurs")
        .and_then(|v| match v.trim() {
            "unbounded" => Some(-1),
            v => v.parse().ok(),
        })
        .filter(|&v| v != 1);
    Occ { m, x }
}

impl<'a> Builder<'a> {
    fn display(&self, ns: &str, local: &str) -> String {
        if ns == self.main_ns || ns.is_empty() {
            local.to_string()
        } else {
            match self.prefixes.get(ns) {
                Some(p) => format!("{p}:{local}"),
                None => local.to_string(),
            }
        }
    }

    fn qname(&self, node: N, s: &str, fi: usize) -> (String, String) {
        let (prefix, local) = match s.trim().split_once(':') {
            Some((p, l)) => (Some(p), l),
            None => (None, s.trim()),
        };
        let file = &self.files[fi];
        let ns = node
            .lookup_namespace_uri(prefix)
            .map(str::to_string)
            .or_else(|| if file.chameleon { file.tns.clone() } else { None })
            .unwrap_or_default();
        (ns, local.to_string())
    }

    fn resolve(&mut self, space: Space, node: N, s: &str, fi: usize) -> Ref {
        let (ns, local) = self.qname(node, s, fi);
        if ns == XSD_NS {
            return Ref::Name(format!("xsd:{local}"));
        }
        match self.index.get(&(space, ns.clone(), local.clone())) {
            Some(&i) => Ref::Idx(i),
            None => {
                let name = match self.prefixes.get(&ns) {
                    Some(p) => format!("{p}:{local}"),
                    None => format!("{{{ns}}}{local}"),
                };
                *self.unresolved.entry(name.clone()).or_default() += 1;
                Ref::Name(name)
            }
        }
    }

    fn ref_name(&self, r: &Ref) -> String {
        match r {
            Ref::Idx(i) => self.comps[*i].n.clone(),
            Ref::Name(s) => s.clone(),
        }
    }

    /// Resolves the component registered at `idx` from its defining node.
    fn build_comp(&mut self, idx: usize, n: N<'a>, fi: usize) {
        let head = &self.comps[idx];
        let mut c = Component {
            k: head.k,
            n: head.n.clone(),
            f: head.f,
            anon: head.anon,
            own: head.own,
            d: doc(n),
            ..Default::default()
        };
        match c.k {
            "ct" => self.complex(&mut c, n, fi, idx),
            "st" => self.simple(&mut c, n, fi, idx),
            "el" => {
                c.abs = n.attribute("abstract") == Some("true");
                c.nil = n.attribute("nillable") == Some("true");
                if let Some(sg) = n.attribute("substitutionGroup").and_then(|s| s.split_whitespace().next()) {
                    c.sg = Some(self.resolve(Space::Element, n, sg, fi));
                }
                c.t = self.typed(n, fi, idx, &c.n.clone(), "type");
            }
            "gr" => c.p = xs_children(n).find_map(|ch| self.particle(ch, fi, idx, &c.n)),
            "ag" => self.content(&mut c, n, fi, idx),
            "at" => c.t = self.typed(n, fi, idx, &c.n.clone(), "type"),
            _ => unreachable!(),
        }
        self.comps[idx] = c;
    }

    /// Registers and resolves an anonymous complexType/simpleType.
    fn anon(&mut self, n: N<'a>, fi: usize, owner: usize, label: &str) -> usize {
        let k = if n.tag_name().name() == "complexType" { "ct" } else { "st" };
        let idx = self.comps.len();
        self.comps.push(Component {
            k,
            n: label.to_string(),
            f: fi,
            anon: true,
            own: Some(owner),
            ..Default::default()
        });
        self.build_comp(idx, n, fi);
        idx
    }

    /// Type given by attribute `attr`, or by an inline anonymous type.
    fn typed(&mut self, n: N<'a>, fi: usize, owner: usize, label: &str, attr: &str) -> Option<Ref> {
        if let Some(t) = n.attribute(attr) {
            return Some(self.resolve(Space::Type, n, t, fi));
        }
        xs_children(n)
            .find(|c| matches!(c.tag_name().name(), "complexType" | "simpleType"))
            .map(|c| Ref::Idx(self.anon(c, fi, owner, label)))
    }

    fn complex(&mut self, c: &mut Component, n: N<'a>, fi: usize, idx: usize) {
        c.abs = n.attribute("abstract") == Some("true");
        c.mx = n.attribute("mixed") == Some("true");
        for ch in xs_children(n) {
            let name = ch.tag_name().name();
            if name != "simpleContent" && name != "complexContent" {
                continue;
            }
            c.sc = name == "simpleContent";
            c.mx |= ch.attribute("mixed") == Some("true");
            for d in xs_children(ch) {
                c.der = match d.tag_name().name() {
                    "extension" => Some("e"),
                    "restriction" => Some("r"),
                    _ => continue,
                };
                if let Some(base) = d.attribute("base") {
                    c.base = Some(self.resolve(Space::Type, d, base, fi));
                }
                self.content(c, d, fi, idx);
            }
        }
        self.content(c, n, fi, idx);
    }

    /// Particles, attributes and (for simpleContent restrictions) enumerations directly below `n`.
    fn content(&mut self, c: &mut Component, n: N<'a>, fi: usize, idx: usize) {
        let label = c.n.clone();
        for ch in xs_children(n) {
            match ch.tag_name().name() {
                "sequence" | "choice" | "all" | "group" => c.p = self.particle(ch, fi, idx, &label),
                "attribute" | "attributeGroup" => {
                    let a = self.attr(ch, fi, idx, &label);
                    c.at.push(a);
                }
                "anyAttribute" => c.anya = true,
                "enumeration" => c.en.push((
                    ch.attribute("value").unwrap_or_default().to_string(),
                    doc(ch).unwrap_or_default(),
                )),
                _ => {}
            }
        }
    }

    fn particle(&mut self, n: N<'a>, fi: usize, owner: usize, label: &str) -> Option<Particle> {
        let o = occ(n);
        match n.tag_name().name() {
            kind @ ("sequence" | "choice" | "all") => {
                let s = match kind {
                    "sequence" => "seq",
                    "choice" => "cho",
                    _ => "all",
                };
                let i = xs_children(n)
                    .filter_map(|ch| self.particle(ch, fi, owner, label))
                    .collect();
                Some(Particle::Model { s, i, o })
            }
            "group" => n.attribute("ref").map(|r| Particle::Group {
                g: self.resolve(Space::Group, n, r, fi),
                o,
            }),
            "element" => {
                if let Some(r) = n.attribute("ref") {
                    let r = self.resolve(Space::Element, n, r, fi);
                    Some(Particle::Element {
                        e: self.ref_name(&r),
                        r: Some(r),
                        t: None,
                        o,
                        d: doc(n),
                    })
                } else {
                    let name = n.attribute("name").unwrap_or("?");
                    let t = self.typed(n, fi, owner, &format!("{label}/{name}"), "type");
                    Some(Particle::Element {
                        e: name.to_string(),
                        r: None,
                        t,
                        o,
                        d: doc(n),
                    })
                }
            }
            "any" => Some(Particle::Any {
                any: n.attribute("namespace").unwrap_or("##any").to_string(),
                o,
            }),
            _ => None,
        }
    }

    fn attr(&mut self, n: N<'a>, fi: usize, owner: usize, label: &str) -> AttrUse {
        if n.tag_name().name() == "attributeGroup" {
            return AttrUse::Group {
                g: self.resolve(Space::AttrGroup, n, n.attribute("ref").unwrap_or_default(), fi),
            };
        }
        let u = match n.attribute("use") {
            Some("required") => Some("r"),
            Some("prohibited") => Some("p"),
            _ => None,
        };
        let (name, r, t) = match n.attribute("ref") {
            Some(r) => {
                let r = self.resolve(Space::Attribute, n, r, fi);
                (self.ref_name(&r), Some(r), None)
            }
            None => {
                let name = n.attribute("name").unwrap_or("?").to_string();
                let t = self.typed(n, fi, owner, &format!("{label}@{name}"), "type");
                (name, None, t)
            }
        };
        AttrUse::Attr {
            n: name,
            r,
            t,
            u,
            df: n.attribute("default").map(str::to_string),
            fx: n.attribute("fixed").map(str::to_string),
            d: doc(n),
        }
    }

    fn simple(&mut self, c: &mut Component, n: N<'a>, fi: usize, idx: usize) {
        let label = c.n.clone();
        for ch in xs_children(n) {
            match ch.tag_name().name() {
                "restriction" => {
                    c.der = Some("r");
                    c.base = self.typed(ch, fi, idx, &format!("{label}/base"), "base");
                    for f in xs_children(ch) {
                        match f.tag_name().name() {
                            "annotation" | "simpleType" => {}
                            "enumeration" => c.en.push((
                                f.attribute("value").unwrap_or_default().to_string(),
                                doc(f).unwrap_or_default(),
                            )),
                            other => c.fa.push((
                                other.to_string(),
                                f.attribute("value").unwrap_or_default().to_string(),
                            )),
                        }
                    }
                }
                "list" => c.li = self.typed(ch, fi, idx, &format!("{label}/item"), "itemType"),
                "union" => {
                    if let Some(members) = ch.attribute("memberTypes") {
                        for m in members.split_whitespace() {
                            let r = self.resolve(Space::Type, ch, m, fi);
                            c.un.push(r);
                        }
                    }
                    for (i, st) in xs_children(ch)
                        .filter(|s| s.tag_name().name() == "simpleType")
                        .enumerate()
                    {
                        let a = self.anon(st, fi, idx, &format!("{label}/member{}", i + 1));
                        c.un.push(Ref::Idx(a));
                    }
                }
                _ => {}
            }
        }
    }
}

/// Fills the reverse links: used-by, derived types and substitution group members.
/// Uses inside anonymous types are attributed to their named owner.
fn link_back(comps: &mut [Component]) {
    let owner = |mut i: usize| {
        while let Some(o) = comps[i].own {
            i = o;
        }
        i
    };
    let len = comps.len();
    let mut ub = vec![BTreeSet::new(); len];
    let mut sub = vec![BTreeSet::new(); len];
    let mut sgm = vec![BTreeSet::new(); len];
    for (i, c) in comps.iter().enumerate() {
        let src = owner(i);
        for u in c.uses() {
            if u != src && !comps[u].anon {
                ub[u].insert(src);
            }
        }
        if let Some(b) = c.base.as_ref().and_then(Ref::idx) {
            sub[b].insert(i);
        }
        if let Some(h) = c.sg.as_ref().and_then(Ref::idx) {
            sgm[h].insert(i);
        }
    }
    for (i, c) in comps.iter_mut().enumerate() {
        c.ub = std::mem::take(&mut ub[i]).into_iter().collect();
        c.sub = std::mem::take(&mut sub[i]).into_iter().collect();
        c.sgm = std::mem::take(&mut sgm[i]).into_iter().collect();
    }
}
