(() => {
'use strict';

const D = JSON.parse(document.getElementById('xsd-data').textContent);
const C = D.c;
const KIND = { ct: 'complex type', st: 'simple type', el: 'element', gr: 'group', ag: 'attribute group', at: 'attribute' };
const SHORT = { ct: 'CT', st: 'ST', el: 'E', gr: 'G', ag: 'AG', at: '@' };
const SPACE = { ct: 'type', st: 'type', el: 'element', gr: 'group', ag: 'attributeGroup', at: 'attribute' };

const esc = s => String(s).replace(/[&<>"]/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[ch]));
const isIdx = r => typeof r === 'number';

// Readable, mostly stable URL fragments: #element/StopPlace, #type/StopPlace_VersionStructure
const HID = new Array(C.length);
const BY_HID = new Map();
C.forEach((c, i) => {
  let h = (c.anon ? 'anon/' : SPACE[c.k] + '/') + c.n;
  if (BY_HID.has(h)) h += '~' + i;
  HID[i] = h;
  BY_HID.set(h, i);
});
const href = i => '#' + encodeURI(HID[i]);

function link(r) {
  if (r === undefined || r === null) return '';
  if (!isIdx(r)) return `<code class="builtin">${esc(r)}</code>`;
  const c = C[r];
  const label = c.anon ? `(anonymous ${KIND[c.k]})` : c.n;
  return `<a href="${href(r)}" title="${KIND[c.k]}">${esc(label)}</a>`;
}
const badge = k => `<span class="kind k-${k}" title="${KIND[k]}">${SHORT[k]}</span>`;
const occ = p => { const m = p.m ?? 1, x = p.x ?? 1; return m + '..' + (x < 0 ? '*' : x); };
const docHtml = d => d ? esc(d).replace(/\n/g, '<br>') : '';
const byName = (a, b) => C[a].n.localeCompare(C[b].n);

function table(cls, head, rows) {
  return `<div class="overflow-auto"><table class="xsd ${cls}"><thead><tr>${head.map(h => `<th>${h}</th>`).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table></div>`;
}

function refList(title, list, open) {
  if (!list || !list.length) return '';
  const sorted = [...list].sort(byName);
  const links = sorted.map(i => `${link(i)}${C[i].anon ? '' : ''}`).join('<span class="muted"> · </span>');
  return `<details${open || list.length <= 40 ? ' open' : ''}><summary>${title} (${list.length})</summary><p class="reflist">${links}</p></details>`;
}

// Used-by references grouped by kind of the referencing component.
function usedBy(c) {
  if (!c.ub || !c.ub.length) return '';
  const groups = {};
  for (const i of c.ub) (groups[C[i].k] ||= []).push(i);
  return '<h3>Used by</h3>' + Object.keys(KIND).filter(k => groups[k])
    .map(k => refList(KIND[k] + 's', groups[k])).join('');
}

// ---- Substitution groups ----------------------------------------------------

function substitutes(i, depth = 0, seen = new Set(), out = []) {
  for (const j of (C[i].sgm || []).slice().sort(byName)) {
    if (seen.has(j)) continue;
    seen.add(j);
    out.push([j, depth]);
    substitutes(j, depth + 1, seen, out);
  }
  return out;
}

// ---- Content model ------------------------------------------------------------

// Content rows are collected as plain objects first, so the table can decide
// afterwards whether it needs the column naming choice alternatives.
//   { sect }                                     section heading (inheritance)
//   { note, depth, br }                          sentence introducing a choice
//   { name, occ, type, doc, depth, br, mand, cls } element / structure row

const letter = k => k < 26 ? String.fromCharCode(97 + k) : String(k + 1);

function quantity(p) {
  const m = p.m ?? 1, x = p.x ?? 1;
  if (m === 0) return x === 1 ? 'none or one' : x < 0 ? 'none, one or more' : `none or up to ${x}`;
  if (m === 1) return x === 1 ? 'one' : x < 0 ? 'one or more' : `one to ${x}`;
  return `${m} to ${x < 0 ? 'any number' : x}`;
}

// What a choice alternative consists of, in the words of the OJP tables.
function branchKind(q) {
  if (q.s === 'seq' && q.i.length === 1 && q.m === undefined && q.x === undefined) return branchKind(q.i[0]);
  if (q.e !== undefined) return 'elements';
  if (q.s === 'cho') return 'choices';
  if (q.s) return 'element sequences';
  if (q.g !== undefined) return 'groups';
  return 'wildcards';
}

function particleRows(p, depth, stack, rows, br = '') {
  if (!p) return;
  const row = r => rows.push({ depth, br, occ: occ(p), type: '', doc: '', ...r });
  if (p.s === 'cho') {
    const labels = p.i.map((_, k) => (br ? br + '.' : '') + letter(k));
    const kinds = new Map();
    p.i.forEach((q, k) => {
      const kind = branchKind(q);
      if (!kinds.has(kind)) kinds.set(kind, []);
      kinds.get(kind).push(labels[k]);
    });
    const parts = [...kinds].map(([kind, ls]) => `${kind} (${ls.join(', ')})`).join(' or ');
    const intro = br ? `In case of choice <em>${esc(br)}</em>, the element contains` : 'The element contains';
    rows.push({ note: `${intro} <em>${quantity(p)}</em> of the following ${parts}`, depth, br });
    p.i.forEach((q, k) => particleRows(q, depth + 1, stack, rows, labels[k]));
  } else if (p.s) {
    if (p.s === 'seq' && p.m === undefined && p.x === undefined) {
      p.i.forEach(q => particleRows(q, depth, stack, rows, br));
      return;
    }
    row({ name: p.s === 'all' ? 'all' : 'sequence', cls: 'struct' });
    p.i.forEach(q => particleRows(q, depth + 1, stack, rows, br));
  } else if (p.g !== undefined) {
    const g = isIdx(p.g) ? C[p.g] : null;
    row({ name: 'group ' + link(p.g), cls: 'struct', doc: g ? docHtml(g.d) : '' });
    if (g && !stack.has(p.g)) {
      stack.add(p.g);
      particleRows(g.p, depth + 1, stack, rows, br);
      stack.delete(p.g);
    }
  } else if (p.e !== undefined) {
    let name = esc(p.e), type = p.t, doc = p.d, extra = '';
    if (p.r !== undefined) {
      name = '→ ' + link(p.r);
      if (isIdx(p.r)) {
        const el = C[p.r];
        type = el.t;
        doc = doc || el.d;
        if (el.sgm && el.sgm.length) {
          const n = substitutes(p.r).length;
          extra = ` <small class="muted">${el.abs ? 'abstract, ' : ''}${n} substitute${n === 1 ? '' : 's'}</small>`;
        }
      }
    }
    row({ name: name + extra, type: link(type), doc: docHtml(doc), mand: (p.m ?? 1) > 0 });
  } else if (p.any !== undefined) {
    row({ name: '<em>any</em> <small class="muted">' + esc(p.any) + '</small>' });
  }
}

function contentRowsTable(rows) {
  const alt = rows.some(r => r.br);
  const cols = alt ? 5 : 4;
  const html = rows.map(r => {
    if (r.sect !== undefined) return `<tr class="sect"><td colspan="${cols}">${r.sect}</td></tr>`;
    const brCell = alt ? `<td class="br">${r.br ? `<em>${esc(r.br)}</em>` : ''}</td>` : '';
    if (r.note !== undefined) return `<tr class="note">${brCell}<td colspan="4" class="nm" style="--d:${r.depth}">${r.note}</td></tr>`;
    return `<tr${r.cls ? ` class="${r.cls}"` : ''}>${brCell}<td class="nm${r.mand ? ' mand' : ''}" style="--d:${r.depth}">${r.name}</td>` +
      `<td class="occ">${r.occ}</td><td>${r.type}</td><td class="doc">${r.doc}</td></tr>`;
  });
  return table('content', [...(alt ? ['Alt.'] : []), 'Element', 'Card.', 'Type', 'Description'], html);
}

// Extension chain of a complex type, root ancestor first.
function extensionChain(i) {
  const out = [i];
  let c = C[i];
  while (c.k === 'ct' && c.der === 'e' && isIdx(c.base) && C[c.base].k === 'ct' && !out.includes(c.base)) {
    out.push(c.base);
    c = C[c.base];
  }
  return out.reverse();
}

// All ancestors, any derivation, nearest first.
function ancestors(i) {
  const out = [];
  let r = C[i].base;
  while (r !== undefined && !out.includes(r)) {
    out.push(r);
    if (!isIdx(r)) break;
    r = C[r].base;
  }
  return out;
}

function contentTable(i) {
  const chain = extensionChain(i).filter(j => C[j].p);
  const rows = [];
  for (const j of chain) {
    const c = C[j];
    if (chain.length > 1 || j !== i) {
      let label = j === i ? 'Own content' : 'Inherited from ' + link(j);
      if (c.der === 'r' && c.base !== undefined) label += ' (restricts ' + link(c.base) + ')';
      rows.push({ sect: label });
    }
    particleRows(c.p, 0, new Set(), rows);
  }
  return rows.length ? contentRowsTable(rows) : '';
}

function attrRows(list, from, stack, seen, rows) {
  for (const a of list || []) {
    if (a.g !== undefined) {
      if (!isIdx(a.g)) {
        rows.push(`<tr><td class="nm">group ${link(a.g)}</td><td colspan="5" class="muted">unresolved</td></tr>`);
      } else if (!stack.has(a.g)) {
        stack.add(a.g);
        attrRows(C[a.g].at, a.g, stack, seen, rows);
        stack.delete(a.g);
      }
      continue;
    }
    if (seen.has(a.n)) continue;
    seen.add(a.n);
    if (a.u === 'p') continue;
    let type = a.t, doc = a.d;
    if (isIdx(a.r)) { type = C[a.r].t; doc = doc || C[a.r].d; }
    const value = [a.df !== undefined ? `default <code>${esc(a.df)}</code>` : '', a.fx !== undefined ? `fixed <code>${esc(a.fx)}</code>` : ''].join(' ');
    rows.push(`<tr><td class="nm${a.u === 'r' ? ' mand' : ''}">${a.r !== undefined ? link(a.r) : esc(a.n)}</td>` +
      `<td>${a.u === 'r' ? 'required' : 'optional'}</td><td>${link(type)}</td><td>${value}</td>` +
      `<td class="doc">${docHtml(doc)}</td><td>${from !== null ? link(from) : ''}</td></tr>`);
  }
}

function attributesTable(i) {
  const rows = [], seen = new Set(), visited = new Set();
  let j = i;
  // nearest definition wins; walk up the whole derivation chain
  while (isIdx(j) && (C[j].k === 'ct' || C[j].k === 'ag') && !visited.has(j)) {
    visited.add(j);
    attrRows(C[j].at, j === i ? null : j, new Set(), seen, rows);
    j = C[j].base;
  }
  if (!rows.length) return '';
  return '<h3>Attributes</h3>' + table('attrs', ['Attribute', 'Use', 'Type', 'Value', 'Description', 'From'], rows);
}

function enumTable(c) {
  if (!c.en || !c.en.length) return '';
  return `<h3>Enumeration (${c.en.length})</h3>` +
    table('enum', ['Value', 'Description'], c.en.map(([v, d]) => `<tr><td><code>${esc(v)}</code></td><td class="doc">${docHtml(d)}</td></tr>`));
}

function facts(list) {
  const rows = list.filter(([, v]) => v).map(([k, v]) => `<tr><th scope="row">${k}</th><td>${v}</td></tr>`);
  return rows.length ? `<table class="xsd facts"><tbody>${rows.join('')}</tbody></table>` : '';
}

function simpleValue(c) {
  // the value type of simple content: walk the base chain to the first simple type
  const chain = ancestors(C.indexOf(c));
  return chain.map(link).join(' → ');
}

// ---- Pages --------------------------------------------------------------------

function header(i) {
  const c = C[i];
  const [file, ns] = D.files[c.f];
  let h = `<hgroup><h2>${badge(c.k)} ${esc(c.n)}</h2><p><span class="muted">${KIND[c.k]}${c.abs ? ', abstract' : ''} in</span> <code>${esc(file)}</code> <span class="muted">·</span> <code>${esc(D.nss[ns])}</code></p></hgroup>`;
  if (c.anon && c.own !== undefined) h += `<p>Anonymous ${KIND[c.k]} defined inline in ${badge(C[c.own].k)} ${link(c.own)}.</p>`;
  if (c.d) h += `<p class="docblock">${docHtml(c.d)}</p>`;
  return h;
}

function complexBody(i) {
  const c = C[i];
  const anc = ancestors(i);
  let h = facts([
    ['Base type', c.base !== undefined ? link(c.base) + ` <span class="muted">(${c.der === 'e' ? 'extension' : 'restriction'})</span>` : ''],
    ['Inheritance', anc.length > 1 ? anc.map(link).join(' → ') : ''],
    ['Content', c.sc ? 'simple content: ' + simpleValue(c) : c.mx ? 'mixed' : ''],
    ['Any attribute', c.anya ? 'yes' : ''],
  ]);
  const content = contentTable(i);
  if (content) h += '<h3>Content</h3>' + content;
  h += enumTable(c);
  h += attributesTable(i);
  return h;
}

const PAGES = {
  ct(i) {
    const c = C[i];
    return header(i) + complexBody(i) + refList('Derived types', c.sub) + usedBy(c);
  },
  st(i) {
    const c = C[i];
    const anc = ancestors(i);
    let h = header(i) + facts([
      ['Base type', c.base !== undefined ? link(c.base) : ''],
      ['Inheritance', anc.length > 1 ? anc.map(link).join(' → ') : ''],
      ['List of', c.li !== undefined ? link(c.li) : ''],
      ['Union of', c.un ? c.un.map(link).join(', ') : ''],
      ...(c.fa || []).map(([k, v]) => [k, `<code>${esc(v)}</code>`]),
    ]);
    return h + enumTable(c) + refList('Derived types', c.sub) + usedBy(c);
  },
  el(i) {
    const c = C[i];
    const subs = substitutes(i);
    let h = header(i) + facts([
      ['Type', link(c.t)],
      ['Substitutes for', c.sg !== undefined ? link(c.sg) : ''],
      ['Nillable', c.nil ? 'yes' : ''],
    ]);
    if (subs.length) {
      h += `<details${subs.length <= 40 ? ' open' : ''}><summary>Substitution group members (${subs.length})</summary><ul>` +
        subs.map(([j, d]) => `<li style="margin-left:${d * 1.25}rem">${link(j)}${C[j].abs ? ' <small class="muted">abstract</small>' : ''}</li>`).join('') +
        '</ul></details>';
    }
    if (isIdx(c.t)) {
      const t = C[c.t];
      if (t.k === 'ct') h += (t.anon ? '<h3>Inline type</h3>' : `<h3>Type ${link(c.t)}</h3><p class="doc">${docHtml(t.d)}</p>`) + complexBody(c.t);
      else h += enumTable(t);
    }
    return h + usedBy(c);
  },
  gr(i) {
    const c = C[i];
    const rows = [];
    particleRows(c.p, 0, new Set([i]), rows);
    return header(i) + (rows.length ? '<h3>Content</h3>' + contentRowsTable(rows) : '') + usedBy(c);
  },
  ag(i) {
    return header(i) + attributesTable(i) + usedBy(C[i]);
  },
  at(i) {
    const c = C[i];
    return header(i) + facts([['Type', link(c.t)]]) + usedBy(c);
  },
};

function home() {
  const counts = {};
  for (const c of C) if (!c.anon) counts[c.k] = (counts[c.k] || 0) + 1;
  const roots = [];
  C.forEach((c, i) => {
    if (c.k === 'el' && !c.anon && !c.abs && c.sg === undefined && !(c.ub && c.ub.length) && D.files[c.f][1] === D.main) roots.push(i);
  });
  return `<hgroup><h2>${esc(D.title)}</h2><p>${D.files.length} schema files · ${C.length} components (incl. anonymous types)</p></hgroup>` +
    facts(Object.keys(KIND).map(k => [`${badge(k)} ${KIND[k]}s`, counts[k] ? String(counts[k]) : ''])) +
    `<h3>Root elements</h3><p class="muted">Global elements of the main namespace that are not referenced by any other component.</p>` +
    `<p class="reflist">${roots.sort(byName).map(link).join('<span class="muted"> · </span>')}</p>` +
    `<h3>Reading the tables</h3><ul class="legend">
      <li><strong>Bold</strong> names are mandatory (minOccurs ≥ 1); cardinality is shown as <code>min..max</code>.</li>
      <li><code>→ Name</code> is a reference to a global element; abstract elements list their number of substitutes.</li>
      <li>Rows under a <em>group</em>, <em>choice</em> or <em>sequence</em> are indented; groups are expanded inline.</li>
      <li>Content inherited by extension is listed first, per ancestor, followed by the type's own content.</li>
    </ul>`;
}

const main = document.getElementById('main');
function route() {
  const i = BY_HID.get(decodeURI(location.hash.slice(1)));
  if (i === undefined) {
    main.innerHTML = home();
    document.title = D.title;
  } else {
    main.innerHTML = PAGES[C[i].k](i);
    document.title = `${C[i].n} – ${D.title}`;
  }
  main.scrollTop = 0;
  window.scrollTo(0, 0);
}
window.addEventListener('hashchange', route);

// ---- Sidebar: search and file tree ----------------------------------------------

const q = document.getElementById('q');
const kindSel = document.getElementById('kind');
const results = document.getElementById('results');
const LOWER = C.map(c => c.n.toLowerCase());
const item = i => `<li>${badge(C[i].k)}<a href="${href(i)}" title="${esc(C[i].n)}">${esc(C[i].n)}</a></li>`;
const MAX_RESULTS = 300;

const BY_FILE = D.files.map(() => []);
C.forEach((c, i) => { if (!c.anon) BY_FILE[c.f].push(i); });
const DIRS = new Map();
D.files.forEach(([path], fi) => {
  const cut = path.lastIndexOf('/');
  const dir = cut < 0 ? '.' : path.slice(0, cut);
  if (!DIRS.has(dir)) DIRS.set(dir, []);
  DIRS.get(dir).push(fi);
});
const dirNames = [...DIRS.keys()].sort();

function fileItems(fi) {
  const kind = kindSel.value;
  return BY_FILE[fi].filter(i => !kind || C[i].k === kind).sort(byName).map(item).join('');
}

function renderSidebar() {
  const query = q.value.trim().toLowerCase();
  const kind = kindSel.value;
  if (!query) {
    results.innerHTML = dirNames.map(dir =>
      `<details data-dir="${esc(dir)}"><summary>${esc(dir)}/ <small class="muted">${DIRS.get(dir).length}</small></summary></details>`).join('');
    return;
  }
  const hits = [];
  for (let i = 0; i < C.length; i++) {
    if (C[i].anon || (kind && C[i].k !== kind)) continue;
    const n = LOWER[i], pos = n.indexOf(query);
    if (pos < 0) continue;
    hits.push([n === query ? 0 : pos === 0 ? 1 : 2, n.length, i]);
  }
  hits.sort((a, b) => a[0] - b[0] || a[1] - b[1] || byName(a[2], b[2]));
  results.innerHTML = `<p class="muted"><small>${hits.length} match${hits.length === 1 ? '' : 'es'}${hits.length > MAX_RESULTS ? `, showing ${MAX_RESULTS}` : ''}</small></p>` +
    `<ul>${hits.slice(0, MAX_RESULTS).map(h => item(h[2])).join('')}</ul>`;
}

// file tree is filled lazily when a folder/file is opened
results.addEventListener('toggle', e => {
  const d = e.target;
  if (!d.open || d.dataset.filled) return;
  d.dataset.filled = '1';
  if (d.dataset.dir !== undefined) {
    d.insertAdjacentHTML('beforeend', DIRS.get(d.dataset.dir).map(fi => {
      const name = D.files[fi][0].split('/').pop();
      return `<details data-file="${fi}"><summary>${esc(name)} <small class="muted">${BY_FILE[fi].length}</small></summary></details>`;
    }).join(''));
  } else if (d.dataset.file !== undefined) {
    d.insertAdjacentHTML('beforeend', `<ul>${fileItems(+d.dataset.file) || '<li class="muted">nothing of this kind</li>'}</ul>`);
  }
}, true);

let timer;
q.addEventListener('input', () => { clearTimeout(timer); timer = setTimeout(renderSidebar, 120); });
kindSel.addEventListener('change', renderSidebar);
document.addEventListener('keydown', e => {
  if (e.key === '/' && document.activeElement !== q) { e.preventDefault(); q.focus(); q.select(); }
});

renderSidebar();
route();
})();
