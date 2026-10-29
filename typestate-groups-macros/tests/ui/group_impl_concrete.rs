// `#[group_impl]` naming a concrete state that doesn't fit the impl.
#![allow(dead_code)]
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Kind {
    type Tag;
}

#[state_types]
trait Place {}

#[state]
struct Head;
#[state]
struct Linked;
#[state]
struct Loose;

#[group(Heads)]
impl Kind for (Head,) {
    type Tag = ();
}

#[group(Nodes)]
impl Kind for (Linked, Loose) {
    type Tag = u32;
}

#[group(InList)]
impl Place for (Head, Linked) {}

#[group(OutOfList)]
impl Place for (Loose,) {}

#[typestate]
struct Node<S: Kind> {
    tag: S::Tag,
}

#[group_trait(by = (Kind, Place))]
trait Describe {
    fn describe(&self) -> &'static str;
}

// `Loose` isn't in `InList`.
#[group_impl(Kind = Nodes, Place = InList, state = Loose)]
impl Describe for Node<Loose> {
    fn describe(&self) -> &'static str {
        "loose"
    }
}

// The impl is for `Node<Loose>`, not `Node<Linked>`.
#[group_impl(Kind = Nodes, Place = InList, state = Linked)]
impl Describe for Node<Loose> {
    fn describe(&self) -> &'static str {
        "linked"
    }
}

// `T` is neither a type parameter of the impl nor a type.
#[group_impl(Kind = Heads, Place = InList, state = T)]
impl Describe for Node<Head> {
    fn describe(&self) -> &'static str {
        "head"
    }
}

fn main() {}
