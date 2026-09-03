//! Prints the parse of a markdown document, for looking at it by eye.
//!
//! ```sh
//! cargo run --example parse_markdown             # a built-in sample
//! cargo run --example parse_markdown notes.md    # a file
//! ```
//!
//! Useful while building the renderer: if something will not format, this says
//! whether the parser or the renderer is at fault, without a window in the way.

use trackcrab::markdown::{HighlightColor, LineKind, parse, plain};

const SAMPLE: &str = "\
# Acme migration

Kickoff is **Thursday**, run by _Rob_ in platform engineering.
Contract ref `AC-2291`, see [the SOW](https://example.com/sow) for scope.

## Waves

1. Discovery and landing zone
2. Lift the ==yellow|non-critical== estate
3. Cut over ==#e84c4c|the database==

## Open items

- [x] Direct Connect ordered
- [ ] Application inventory from Rob
  - [ ] the list of hard-coded IPs
    - [ ] and whatever the ~~old~~ current DNS story is

---

```rust
// Nothing in here is markdown: *ptr stays a pointer.
let quota = **handle;
```

Escaped for the avoidance of doubt: \\*not italic\\*.
";

fn main() {
    let source = std::env::args().nth(1).map_or_else(
        || SAMPLE.to_owned(),
        |path| std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}")),
    );

    let doc = parse(&source);
    println!("{} line(s)\n", doc.lines.len());

    for (number, (line, inline)) in doc.rows().enumerate() {
        let kind = match &line.kind {
            LineKind::Blank => "blank".to_owned(),
            LineKind::Paragraph => "para".to_owned(),
            LineKind::Heading(level) => format!("h{level}"),
            LineKind::Bullet => "bullet".to_owned(),
            LineKind::Numbered(n) => format!("num {n}"),
            LineKind::Task(true) => "task [x]".to_owned(),
            LineKind::Task(false) => "task [ ]".to_owned(),
            LineKind::Divider => "divider".to_owned(),
            LineKind::FenceOpen { lang } => {
                let lang = &source[lang.clone()];
                if lang.is_empty() {
                    "fence".to_owned()
                } else {
                    format!("fence {lang}")
                }
            }
            LineKind::FenceClose => "fence end".to_owned(),
            LineKind::Code => "code".to_owned(),
        };

        let marker = &source[line.marker.clone()];
        println!(
            "{:>3}  {:<10} depth {}  marker {:?}",
            number + 1,
            kind,
            line.depth,
            marker
        );

        for span in &inline.spans {
            let mut tags = Vec::new();
            for (on, name) in [
                (span.style.bold, "bold"),
                (span.style.italic, "italic"),
                (span.style.underline, "underline"),
                (span.style.strike, "strike"),
                (span.style.code, "code"),
            ] {
                if on {
                    tags.push(name.to_owned());
                }
            }
            if let Some(colour) = span.style.highlight {
                tags.push(match colour {
                    HighlightColor::Default => "mark".to_owned(),
                    HighlightColor::Named(p) => format!("mark:{}", p.name()),
                    HighlightColor::Rgb([r, g, b]) => format!("mark:#{r:02x}{g:02x}{b:02x}"),
                });
            }
            if let Some(url) = &span.style.link {
                tags.push(format!("link:{}", &source[url.clone()]));
            }
            let tags = if tags.is_empty() {
                "-".to_owned()
            } else {
                tags.join("+")
            };
            println!(
                "       {:<28} {tags}",
                format!("{:?}", &source[span.range.clone()])
            );
        }
        if !inline.markup.is_empty() {
            let hidden: Vec<_> = inline.markup.iter().map(|r| &source[r.clone()]).collect();
            println!("       hidden: {hidden:?}");
        }
    }

    println!("\n--- what search sees ---\n{}", plain(&source));
}
