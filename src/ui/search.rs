//! What the search actually matches against.
//!
//! Searching should match what you can *see*. A note reading `the **flour**`
//! shows the words "the flour", so that is what searching for "the flour" has
//! to find, and searching for `*` has to find nothing, because there is no
//! asterisk on screen. Both fall out of stripping the markup first, which is
//! [`markdown::plain`]'s job. This module exists because of what that costs.
//!
//! # Why there is a cache here
//!
//! The sidebar asks about every node in the tree, twice a frame, for as long as
//! the search box has text in it, and every task carries two markdown fields.
//! Measured on a two thousand note vault, one pass is 12.8ms and essentially
//! all of it is the parse: `plain`'s own string building and the lowercasing
//! together account for half a millisecond of that. So a plain implementation
//! drops frames while you type, which is the one place in this app where
//! latency is felt directly.
//!
//! The stripped text is therefore memoised on **the field's own bytes**. Keying
//! on content rather than on a node id and a timestamp is what keeps this a
//! memo rather than a second copy of the state: the same input cannot map to a
//! stale answer, there is nothing to invalidate, no signature anywhere else has
//! to change, and emptying the whole thing changes only speed. The same pass
//! warm is 0.55ms, twenty three times faster and within a whisker of what a
//! bare `contains` over the raw text costs.
//!
//! `cargo run --release --example bench_search` prints all of those, so a
//! change to the parser can be checked against this claim rather than trusted.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::markdown;

/// Entries kept before the memo is emptied.
///
/// A whole vault fits inside this comfortably. The cap is here because editing
/// a note leaves its previous text behind as a key nothing will ever ask for
/// again, so an afternoon of typing would otherwise grow the map without bound.
/// Emptying it rather than evicting the oldest entry keeps this to one line and
/// costs a single slow frame in a session that has already had thousands of
/// fast ones.
const CAP: usize = 4096;

thread_local! {
    /// Keyed by the source text itself, so a lookup hashes and then compares
    /// rather than trusting a hash: a collision here would mean searching one
    /// note and matching against another, and a wrong search result is worse
    /// than a slow one.
    static MEMO: RefCell<HashMap<Box<str>, Arc<str>>> = RefCell::new(HashMap::new());
}

/// Does `text` mention `needle`, as a reader would see it?
///
/// `needle` must already be trimmed and lowercased, which [`crate::ui::Filter`]
/// does once per pass rather than once per field.
#[must_use]
pub fn mentions(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if text.is_empty() {
        return false;
    }
    stripped(text).contains(needle)
}

/// The markup-stripped, lowercased form of `text`.
fn stripped(text: &str) -> Arc<str> {
    MEMO.with_borrow_mut(|memo| {
        if let Some(hit) = memo.get(text) {
            return Arc::clone(hit);
        }
        if memo.len() >= CAP {
            memo.clear();
        }
        let value: Arc<str> = markdown::plain(text).to_lowercase().into();
        memo.insert(text.into(), Arc::clone(&value));
        value
    })
}

/// Empties the memo.
///
/// Only for tests, which need to be able to prove the cache is a cache: that
/// what comes back after clearing it is the same answer, and that a note edited
/// under the same key is not answered from the old one.
pub fn forget() {
    MEMO.with_borrow_mut(HashMap::clear);
}
