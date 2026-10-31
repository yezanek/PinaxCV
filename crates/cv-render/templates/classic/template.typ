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

// The name block: name and headline. Each line ends with a no-break space
// (`~`) so extractors that join lines still see separate words (ADR-024).
#let name-block = {
  block(below: 0.5em)[#text(size: 1.8em, weight: "bold")[#d.basics.full_name~]]
  if d.basics.headline != none { block(below: 0.4em)[#d.basics.headline~] }
}

// Contact details, one item per line. In the main column email, phone and
// location share a line; in the narrow sidebar (`stacked`) each has its own.
#let contact-block(stacked) = {
  let parts = ()
  if d.basics.email != none { parts.push(d.basics.email) }
  if d.basics.phone != none { parts.push(d.basics.phone) }
  if d.basics.location != none { parts.push(d.basics.location) }
  if stacked {
    for p in parts { block(below: 0.4em)[#p~] }
  } else if parts.len() > 0 {
    block(below: 0.4em)[#parts.join(" | ")~]
  }
  for l in d.basics.links {
    block(below: 0.4em)[#if l.label != none [#l.label: ]#url-text(l)~]
  }
  // Personal details switched on for this CV (ADR-088), as plain lines.
  for p in d.basics.personal {
    block(below: 0.4em)[#p.label: #p.value~]
  }
}

// The optional photo (ADR-083) is a PDF artifact: no text, so text
// extraction and the reading order are the same with and without it. In the
// main column it sits to the right of the name (and of the contact block
// when that follows the name); in the sidebar it sits above the name.
#let with-photo(body, stacked) = {
  if not d.basics.photo {
    body
  } else if stacked {
    block(below: 0.8em, pdf.artifact(image("photo.jpg", width: 2.8cm)))
    body
  } else {
    grid(
      columns: (1fr, auto),
      column-gutter: 1em,
      body,
      pdf.artifact(image("photo.jpg", width: 2.8cm)),
    )
  }
}

#let summary-block = if d.summary != none [
  = #d.labels.summary
  #block(breakable: not d.layout.two, d.summary)
]

// The title (bold) and the period (right-aligned) share the first line, and
// the organisation and location follow on the next. Each is its own text run
// with clear space around it, so parsers that group runs by font and spacing
// read title, organisation and dates apart (ADR-061).
#let entry(e) = {
  let rest = ()
  if e.org != none { rest.push(e.org) }
  if e.location != none { rest.push(e.location) }
  let first = none
  if e.title != none {
    first = strong(e.title)
  } else if rest.len() > 0 {
    first = rest.join(" | ")
    rest = ()
  }
  block(breakable: false, sticky: not e.collapsed, below: 0.5em)[
    #first#if e.period != none [#h(1fr)#e.period]
    #if rest.len() > 0 [ \ #rest.join(" | ")]
  ]
  if e.description != none { block(below: 0.5em, breakable: not d.layout.two, e.description) }
  if e.link != none { block(below: 0.5em, url-text(e.link)) }
  if e.bullets.len() > 0 {
    // With two columns a page break never splits a bullet: the other
    // column's text would land inside it in the extracted text (ADR-084).
    if d.layout.two { list(..e.bullets.map(b => block(breakable: false, b))) } else { list(..e.bullets) }
  }
  v(0.4em)
}

#let section-block(s) = {
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

// One column's blocks in order (ADR-084). A name directly followed by the
// contact block is drawn as one header, with the photo beside both.
#let column(blocks, stacked) = {
  let i = 0
  while i < blocks.len() {
    let b = blocks.at(i)
    if b.kind == "name" {
      let next = blocks.at(i + 1, default: none)
      if next != none and next.kind == "contact" {
        with-photo({ name-block; contact-block(stacked) }, stacked)
        i += 1
      } else {
        with-photo(name-block, stacked)
      }
    } else if b.kind == "contact" {
      contact-block(stacked)
    } else if b.kind == "summary" {
      summary-block
    } else if b.kind == "section" {
      section-block(d.sections.at(b.index))
    }
    i += 1
  }
}

// Two columns are one grid row that breaks across pages. The column holding
// the name is on the left: Typst writes a grid's text left column first, so
// the PDF text starts with the name (ADR-084).
#let l = d.layout
#if not l.two {
  column(l.main, false)
} else {
  let side = l.sidebar_width_pct * 1%
  let sidebar = column(l.sidebar, true)
  let main = column(l.main, false)
  if l.sidebar_first {
    grid(columns: (side, 1fr), column-gutter: 1.5em, sidebar, main)
  } else {
    grid(columns: (1fr, side), column-gutter: 1.5em, main, sidebar)
  }
}
