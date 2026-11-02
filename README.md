# xsd-render

Turns a (large) XML Schema into a single self-contained HTML page documenting every
complex type, simple type, element, group, attribute group and attribute, with links
between them. Styled with [Pico CSS](https://picocss.com/) (loaded from a CDN).

    cargo run --release -- path/to/NeTEx/xsd/NeTEx_publication.xsd -t NeTEx -o netex.html

Starting from the entry files, all `include`/`import`/`redefine` schemas are followed.
The resolved model is embedded as JSON and rendered on demand in the browser:

- content tables in the style of the OJP documentation: inherited content first (per
  ancestor), groups/choices expanded inline, mandatory elements in bold, cardinality,
  linked types and documentation
- attributes (incl. inherited and attribute groups), enumerations and facets
- reverse links: used by, derived types, substitution group members
- search (`/`), kind filter and a file tree in the sidebar
- links to example instance documents (`--examples <DIR>`, optionally with
  `--examples-url <URL>` to point the links at e.g. GitHub instead of local paths)

## Installation

    cargo install xsd-render

then

    xsd-render NeTEx/xsd/NeTEx_publication.xsd -t NeTEx -o netex.html

With examples linked to GitHub:

    xsd-render NeTEx/xsd/NeTEx_publication.xsd -t NeTEx -o netex.html \
      -e NeTEx/examples -u https://github.com/NeTEx-CEN/NeTEx/blob/master/examples

## License

MIT
