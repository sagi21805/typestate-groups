// Misuse of the arguments of `#[group_trait]` and `#[group_impl]` with several traits.
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

#[group_trait(by = (Meta, Meta))]
trait Twice {}

#[group_trait(by = ())]
trait Nothing {}

#[group_trait(by = (Meta, Role))]
trait Describe {
    fn describe(&self) -> &'static str;
}

#[group_impl(Numbers, Role = Heads)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "unnamed"
    }
}

#[group_impl(Meta = Numbers, Meta = Words)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "twice"
    }
}

#[group_impl()]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> &'static str {
        "none"
    }
}

fn main() {}
