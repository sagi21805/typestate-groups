// Misuse of `#[group_impl]`.
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

#[group(Numbers)]
impl Meta for (Small,) {
    type Value = u32;
}

#[state]
struct Big;

#[group(Words)]
impl Meta for (Big,) {
    type Value = String;
}

#[typestate]
struct Wrap<S: Meta> {
    value: S::Value,
}

#[typestate(state = S)]
struct Pair<S: Meta, I> {
    value: S::Value,
    extra: I,
}

#[group_trait(by = Meta)]
trait Describe {
    fn describe(&self) -> String;
}

struct Foo;

#[group_impl(Numbers)]
impl Foo {
    fn method(&self) {}
}

// `Numbers` sets `Value` to `u32`.
#[group_impl(Numbers)]
impl<S: Meta<Value = String>> Describe for Wrap<S> {
    fn describe(&self) -> String {
        String::new()
    }
}

// Rejected on the state, inline and in `where`, but kept on `I`.
#[group_impl(Numbers, state = S)]
impl<I: Iterator<Item = u32>, S: Meta<Value = String>> Describe
    for Pair<S, I>
{
    fn describe(&self) -> String {
        String::new()
    }
}

// Rejected even when it matches the group.
#[group_impl(Words, state = S)]
impl<S: Meta, I> Describe for Pair<S, I>
where
    I: Iterator<Item = u32>,
    S: Meta<Value = String>,
{
    fn describe(&self) -> String {
        String::new()
    }
}

#[group_impl(Numbers)]
impl<S: Meta, I> Describe for Pair<S, I> {
    fn describe(&self) -> String {
        String::new()
    }
}

#[group_impl(Numbers, state = T)]
impl<S: Meta, I> Describe for Pair<S, I> {
    fn describe(&self) -> String {
        String::new()
    }
}

fn main() {}
