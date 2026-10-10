// Misuse of `#[group_trait]`.
#![allow(dead_code)]
use typestate_groups_macros::{group_trait, state_types};

#[state_types]
trait Meta {
    type Value;
}

#[group_trait(by = Meta)]
trait PatternArg {
    fn combine(&self, (x, y): (i32, i32)) -> i32;
}

#[group_trait(by = Meta)]
trait NoReceiver {
    fn make() -> Self;
}

#[group_trait(by = Meta)]
trait DefaultConst {
    const LIMIT: u32 = 10;

    fn a(&self) -> u32;
}

fn main() {}
