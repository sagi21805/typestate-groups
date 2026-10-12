// Misuse of a `#[group_trait]` trait with generic parameters.
#![allow(dead_code)]
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Meta {
    type Value;
}

#[state]
struct Small;
#[state]
struct Big;

#[group(Numbers)]
impl Meta for (Small,) {
    type Value = u32;
}

#[group(Words)]
impl Meta for (Big,) {
    type Value = String;
}

#[typestate]
struct Wrap<S: Meta> {
    value: S::Value,
}

#[group_trait(by = Meta)]
trait Scale<F> {
    fn scale(&self, factor: F) -> u32;
}

// Every group impl names the trait's argument.
#[group_impl(Numbers)]
impl<S: Meta> Scale for Wrap<S> {
    fn scale(&self, factor: u32) -> u32 {
        factor
    }
}

#[group_trait(by = Meta)]
trait Describe<F> {
    fn describe(&self, prefix: F) -> String;
}

#[group_impl(Numbers, state = S)]
impl<S: Meta, F: ToString> Describe<F> for Wrap<S> {
    fn describe(&self, prefix: F) -> String {
        prefix.to_string()
    }
}

fn main() {
    // `Big` is in `Words`, which doesn't implement `Describe`.
    let big = Wrap::<Big> {
        value: String::new(),
    };
    big.describe("word");
}
