// PinaxCV "classic" template. ATS rules: docs/ATS_RULES.md and spec section 6.4.
// All user text arrives as data in data.json and is printed as plain strings.

#let d = json("data.json")
#let o = d.opts
#let accent = if o.accent == none { black } else { rgb(o.accent) }

#set document(title: d.meta.title, author: d.meta.author)
#set page(paper: d.paper, margin: o.margin_mm * 1mm)
#set text(
  font: "Inter",
  size: o.font_size_pt * 1pt,
  lang: d.locale,
  ligatures: false,
  discretionary-ligatures: false,
  historical-ligatures: false,
)
#set par(leading: o.line_spacing * 0.65em, justify: false)
#set list(marker: [•], indent: 0.2em, body-indent: 0.5em)

#show heading.where(level: 1): it => block(above: 1.1em, below: 0.6em, sticky: true)[
  #text(size: 1.1em, weight: "bold", fill: accent)[#it.body]
  #v(-0.7em)
  #line(length: 100%, stroke: 0.5pt + accent)
]

#let url-text(l) = if l.clickable { link(l.url, l.url) } else { l.url }

// Contact block: body text at the top of page 1, one item per line. Each
// line ends with a no-break space (`~`) so extractors that join lines still
// see separate words (ADR-024).
#block(below: 0.5em)[#text(size: 1.8em, weight: "bold")[#d.basics.full_name~]]
#if d.basics.headline != none { block(below: 0.4em)[#d.basics.headline~] }
#{
  let parts = ()
  if d.basics.email != none { parts.push(d.basics.email) }
  if d.basics.phone != none { parts.push(d.basics.phone) }
  if d.basics.location != none { parts.push(d.basics.location) }
  if parts.len() > 0 { block(below: 0.4em)[#parts.join(" | ")~] }
}
#for l in d.basics.links {
  block(below: 0.4em)[#if l.label != none [#l.label: ]#url-text(l)~]
}

#if d.summary != none [
  = #d.labels.summary
  #d.summary
]

#let entry(e) = {
  let head = ()
  if e.title != none { head.push(strong(e.title)) }
  if e.org != none { head.push(e.org) }
  let sub = ()
  if e.period != none { sub.push(e.period) }
  if e.location != none { sub.push(e.location) }
  block(breakable: false, sticky: not e.collapsed, below: 0.5em)[
    #head.join(", ")
    #if sub.len() > 0 [ \ #sub.join(" | ")]
  ]
  if e.description != none { block(below: 0.5em, e.description) }
  if e.link != none { block(below: 0.5em, url-text(e.link)) }
  if e.bullets.len() > 0 { list(..e.bullets) }
  v(0.4em)
}

#for s in d.sections {
  if s.kind in ("experience", "education", "projects", "certifications", "custom") {
    if s.heading != "" { heading(level: 1, s.heading) }
    for e in s.entries { entry(e) }
  } else if s.kind == "skills" {
    heading(level: 1, s.heading)
    for g in s.groups {
      block(below: 0.4em)[#if g.label != none [#strong(g.label): ]#g.skills.join(", ")]
    }
  } else if s.kind == "languages" {
    heading(level: 1, s.heading)
    s.items.map(i => if i.level != none { i.name + " (" + i.level + ")" } else { i.name }).join(", ")
  }
}
