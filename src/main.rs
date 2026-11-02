mod examples;
mod model;
mod parse;

use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::Serialize;

const TEMPLATE: &str = include_str!("../assets/template.html");
const APP_JS: &str = include_str!("../assets/app.js");
const APP_CSS: &str = include_str!("../assets/app.css");

const USAGE: &str = "\
Usage: xsd-render [OPTIONS] <ENTRY.xsd>...

Parses the given XSD files (following include/import/redefine) and writes a
single self-contained HTML page documenting all components and their links.

Options:
  -o, --output <FILE>   output file [default: schema.html]
  -t, --title <TITLE>   page title [default: name of the first entry file]
  -e, --examples <DIR>  scan DIR recursively for XML examples and link them from
                        the elements and types they use
  -u, --examples-url <URL>
                        base URL the example links point to (e.g. the examples
                        folder on GitHub) [default: path relative to the output]
  -h, --help            print this help
  -V, --version         print the version";

#[derive(Serialize)]
struct Data<'a> {
    title: &'a str,
    main: usize,
    nss: &'a [String],
    files: &'a [(String, usize)],
    c: &'a [model::Component],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    ex: &'a [String],
    /// base URL or path the example paths are relative to
    #[serde(skip_serializing_if = "str::is_empty")]
    exb: &'a str,
}

fn main() -> Result<()> {
    let mut entries = Vec::new();
    let mut output = PathBuf::from("schema.html");
    let mut title = None;
    let mut examples = None;
    let mut examples_url = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => output = args.next().map(PathBuf::from).ok_or_else(|| anyhow::anyhow!("{arg} needs a value"))?,
            "-t" | "--title" => title = Some(args.next().ok_or_else(|| anyhow::anyhow!("{arg} needs a value"))?),
            "-e" | "--examples" => examples = Some(args.next().map(PathBuf::from).ok_or_else(|| anyhow::anyhow!("{arg} needs a value"))?),
            "-u" | "--examples-url" => examples_url = Some(args.next().ok_or_else(|| anyhow::anyhow!("{arg} needs a value"))?),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("xsd-render {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            a if a.starts_with('-') => bail!("unknown option {a}\n\n{USAGE}"),
            _ => entries.push(PathBuf::from(arg)),
        }
    }
    if entries.is_empty() {
        bail!("no entry schema given\n\n{USAGE}");
    }
    if examples_url.is_some() && examples.is_none() {
        bail!("--examples-url requires --examples");
    }
    let title = title.unwrap_or_else(|| {
        entries[0]
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "XSD".into())
    });

    let files = parse::load(&entries)?;
    let mut model = parse::build(&files);
    let (example_base, example_links) = match &examples {
        Some(dir) => {
            let out_dir = match output.parent() {
                Some(p) if !p.as_os_str().is_empty() => p,
                _ => std::path::Path::new("."),
            };
            examples::scan(dir, examples_url.as_deref(), out_dir, &model.globals, &mut model.components)?
        }
        None => (String::new(), Vec::new()),
    };

    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for c in model.components.iter().filter(|c| !c.anon) {
        *counts.entry(c.k).or_default() += 1;
    }
    eprintln!("{} files, {} components {:?}", files.len(), model.components.len(), counts);
    if model.duplicates > 0 {
        eprintln!("warning: {} duplicate global definitions ignored", model.duplicates);
    }
    if !model.unresolved.is_empty() {
        eprintln!("warning: {} unresolved references:", model.unresolved.len());
        for (name, n) in model.unresolved.iter().take(20) {
            eprintln!("  {name} ({n}x)");
        }
    }

    let json = serde_json::to_string(&Data {
        title: &title,
        main: model.main_ns,
        nss: &model.namespaces,
        files: &model.files,
        c: &model.components,
        ex: &example_links,
        exb: &example_base,
    })?
    // keep the JSON from terminating its <script> element
    .replace("</", "<\\/");

    let html = TEMPLATE
        .replace("{{TITLE}}", &html_escape(&title))
        .replace("{{CSS}}", APP_CSS)
        .replace("{{JS}}", APP_JS)
        .replace("{{DATA}}", &json);
    std::fs::write(&output, html)?;
    eprintln!("wrote {} ({:.1} MB)", output.display(), std::fs::metadata(&output)?.len() as f64 / 1e6);
    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
