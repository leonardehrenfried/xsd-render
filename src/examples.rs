//! Scanning a folder of XML instance documents and linking them to the
//! global elements and types they use.

use std::collections::HashMap;
use std::path::{Component as PathComp, Path, PathBuf};

use anyhow::{Context, Result};

use crate::model::Component;

const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Global elements and named types, keyed by (namespace, local name).
pub struct Globals {
    pub elements: HashMap<(String, String), usize>,
    pub types: HashMap<(String, String), usize>,
}

/// Scans `dir` recursively for `*.xml` files and records in each component's
/// `ex` the files that use it. Returns the link base (`base_url`, or the path
/// from `out_dir` to `dir`) and the URL-encoded path of every linked example
/// relative to `dir`.
pub fn scan(
    dir: &Path,
    base_url: Option<&str>,
    out_dir: &Path,
    globals: &Globals,
    comps: &mut [Component],
) -> Result<(String, Vec<String>)> {
    let mut files = Vec::new();
    walk(dir, &mut files).with_context(|| format!("scanning {}", dir.display()))?;
    files.sort();

    let base = match base_url {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => relative_to(
            &out_dir.canonicalize().with_context(|| format!("resolving {}", out_dir.display()))?,
            &dir.canonicalize()?,
        ),
    };
    let base = if base.is_empty() { ".".to_string() } else { base };

    let mut links = Vec::new();
    let mut failed = 0;
    for path in &files {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
        let Ok(doc) = roxmltree::Document::parse_with_options(&text, opts) else {
            failed += 1;
            continue;
        };

        let fi = links.len();
        let mut hit = false;
        let mut add = |i: usize, comps: &mut [Component]| {
            if comps[i].ex.last() != Some(&fi) {
                comps[i].ex.push(fi);
            }
            hit = true;
        };
        for n in doc.descendants().filter(|n| n.is_element()) {
            let tag = n.tag_name();
            let key = (tag.namespace().unwrap_or("").to_string(), tag.name().to_string());
            if let Some(&i) = globals.elements.get(&key) {
                add(i, comps);
                // the element's named type is exemplified as well; for an
                // inline type, the named type it is derived from
                let mut t = comps[i].t.as_ref().and_then(|t| t.idx());
                if let Some(a) = t.filter(|&a| comps[a].anon) {
                    t = comps[a].base.as_ref().and_then(|b| b.idx());
                }
                if let Some(t) = t.filter(|&t| !comps[t].anon) {
                    add(t, comps);
                }
            }
            if let Some(qn) = n.attribute((XSI_NS, "type")) {
                let (prefix, local) = qn.split_once(':').map_or((None, qn), |(p, l)| (Some(p), l));
                let ns = n.lookup_namespace_uri(prefix).unwrap_or("");
                if let Some(&t) = globals.types.get(&(ns.to_string(), local.to_string())) {
                    add(t, comps);
                }
            }
        }
        if !hit {
            continue;
        }

        let rel = path.strip_prefix(dir).unwrap_or(path);
        links.push(
            rel.components()
                .map(|c| encode(&c.as_os_str().to_string_lossy()))
                .collect::<Vec<_>>()
                .join("/"),
        );
    }

    eprintln!(
        "{} example files, {} linked{}",
        files.len(),
        links.len(),
        if failed > 0 { format!(", {failed} unreadable") } else { String::new() }
    );
    Ok((base, links))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")) {
            out.push(path);
        }
    }
    Ok(())
}

/// URL-encoded relative path from directory `from` to `to` (both absolute).
fn relative_to(from: &Path, to: &Path) -> String {
    let a: Vec<PathComp> = from.components().collect();
    let b: Vec<PathComp> = to.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    std::iter::repeat_n("..".to_string(), a.len() - common)
        .chain(b[common..].iter().map(|c| encode(&c.as_os_str().to_string_lossy())))
        .collect::<Vec<_>>()
        .join("/")
}

/// Percent-encodes a single path segment.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
