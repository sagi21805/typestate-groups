// Misuse of `#[group_trait]`.
#![allow(dead_code)]
use typestate_groups_macros::{group_trait, state_types};

#[state_types]
trait Meta {
    type Value;
}

#[group_trait(by = Meta)]
trait AssocType {
    type Extra;

    fn a(&self) -> Self::Extra;
}

#[state_types]
trait Generic<T> {
    type Value;
}

#[group_trait(by = Generic<u8>)]
trait ByGeneric {
    fn a(&self);
}

#[group_trait(by = Meta)]
trait PatternArg {
    fn combine(&self, (x, y): (i32, i32)) -> i32;
}

#[group_trait(by = Meta)]
trait NoReceiver {
    fn make() -> Self;
}

fn main() {}
