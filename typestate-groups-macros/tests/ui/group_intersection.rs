// Groups `#[group_impl]` names that rustc rejects for a trait grouped by several traits.
#![allow(dead_code)]
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Meta {
    type Value;
}

#[state_types]
trait Role {}

#[state]
struct SmallHead;
#[state]
struct Small;
#[state]
struct BigHead;

#[group(Numbers)]
impl Meta for (SmallHead, Small) {
    type Value = u32;
}

#[group(Words)]
impl Meta for (BigHead,) {
    type Value = String;
}

#[group(Heads)]
impl Role for (SmallHead, BigHead) {}

#[group(Attached)]
impl Role for (Small,) {}

#[typestate]
struct Wrap<S: Meta> {
    value: S::Value,
}

#[group_trait(by = (Meta, Role))]
trait Describe {
    fn describe(&self) -> &'static str;
}

#[group_impl(Meta = Numbers, Role = Heads)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "small head"
    }
}

// Overlaps the impl above.
#[group_impl(Role = Heads, Meta = Numbers)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "small head again"
    }
}

// `Attached` belongs to `Role`, and `Words` to `Meta`.
#[group_impl(Meta = Attached, Role = Words)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "swapped"
    }
}

// Names traits that don't exist, in an order that puts each group in the
// right place.
#[group_impl(A = Numbers, B = Attached)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "small"
    }
}

// Names no `Role` group.
#[group_impl(Meta = Words)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "big"
    }
}

fn main() {}
