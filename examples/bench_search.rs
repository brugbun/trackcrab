//! Where the cost of a search goes, and why `ui::search` memoises.
//!
//! The numbers quoted in that module's own documentation come from here, so a
//! change to the parser can be checked against the claim rather than trusted.
//!
//! ```sh
//! cargo run --release --example bench_search
//! ```

fn main() {
    // One note of the shape this app actually holds: a heading, prose with two
    // links, a nested list with checkboxes, a highlight, and a fenced block.
    let note = concat!(
        "# Kickoff\n",
        "Spoke to **Acme** about the *VPC* migration. Notes at ",
        "https://example.com/acme/vpc and in [the deck](https://example.com/d).\n",
        "- [ ] confirm the ==yellow|subnet plan==\n",
        "- [x] send the `terraform` diff\n",
        "  - nested detail about the __transit gateway__\n",
        "1. first\n2. second\n---\n",
        "```hcl\nresource \"aws_vpc\" \"main\" { cidr_block = \"10.0.0.0/16\" }\n```\n",
    );
    let notes = 2000usize;
    let corpus: Vec<String> = (0..notes).map(|i| format!("{note}\nnode {i}")).collect();
    let needle = "transit gateway";
    println!("{notes} notes of {} bytes, searching for {needle:?}\n", note.len());

    let time = |label: &str, run: &mut dyn FnMut() -> usize| {
        let start = std::time::Instant::now();
        let hits = run();
        println!("  {label:<24} {:>12.3?}   ({hits} hits)", start.elapsed());
    };

    time("raw contains", &mut || {
        corpus
            .iter()
            .filter(|text| text.to_lowercase().contains(needle))
            .count()
    });
    time("parse", &mut || {
        corpus
            .iter()
            .map(|text| trackcrab::markdown::parse(text).lines.len())
            .sum()
    });
    time("plain + lowercase", &mut || {
        corpus
            .iter()
            .filter(|text| {
                trackcrab::markdown::plain(text)
                    .to_lowercase()
                    .contains(needle)
            })
            .count()
    });
    // Cold, so this pays the parse once per note, then warm, which is every
    // frame after the first.
    trackcrab::ui::search::forget();
    time("search, cold", &mut || {
        corpus
            .iter()
            .filter(|text| trackcrab::ui::search::mentions(text, needle))
            .count()
    });
    time("search, warm", &mut || {
        corpus
            .iter()
            .filter(|text| trackcrab::ui::search::mentions(text, needle))
            .count()
    });
}
