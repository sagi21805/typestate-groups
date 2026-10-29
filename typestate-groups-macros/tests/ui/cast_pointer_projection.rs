// A projection that holds a pointer, or a `Cell`, can't change state by cast
// even inside its own group, because zerocopy can't vouch for its bytes.
#![allow(dead_code)]

use core::{cell::Cell, ptr::NonNull};
use typestate_groups::Isomorphic;
use typestate_groups_macros::{group, state, state_types, typestate};

#[state_types]
trait Node {
    type Prev;
    type Count;
}

#[state]
struct Linked;
#[state]
struct Loose;

#[group(Nodes)]
impl Node for (Linked, Loose) {
    #[size(8)]
    type Prev = Option<NonNull<u64>>;
    #[size(1)]
    type Count = Cell<u8>;
}

#[typestate(unsafe_transmute = true)]
struct Meta<S: Node> {
    prev: S::Prev,
    count: S::Count,
}

fn pointer_within_its_group(meta: Meta<Linked>) {
    let _ = meta.cast_state::<Loose>();
}

fn cell_within_its_group(meta: &Meta<Linked>) {
    let _ = meta.cast_state_ref::<Loose>();
}

fn main() {}
