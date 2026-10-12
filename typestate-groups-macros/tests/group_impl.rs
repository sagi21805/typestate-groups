//! `#[group_trait]` and `#[group_impl]` dispatch a trait to one impl per
//! group.
#![expect(
    non_camel_case_types,
    reason = "a snake_case trait checks the generated helper names"
)]

use typestate_groups::State;
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Meta {
    type Value;
    type Extra;
}

#[state]
struct A;
#[state]
struct B;
#[state]
struct C;

#[group(Numbers)]
impl Meta for (A, B) {
    type Value = u32;
    type Extra = u8;
}

#[group(Words)]
impl Meta for (C,) {
    type Value = String;
    type Extra = char;
}

#[typestate]
struct Wrap<S: Meta> {
    value: S::Value,
    extra: S::Extra,
}

#[group_trait(by = Meta)]
trait describe {
    const LIMIT: u32;

    fn sum(&self, x: u32, y: u32) -> String;

    fn tag(&self) -> &'static str {
        "default"
    }

    fn doubled(&self, x: u32) -> String {
        self.sum(x, x)
    }
}

// Only `S: Meta`: `#[group_impl]` pins `S::Value` and `S::Extra` to the
// group's types.
#[group_impl(Numbers)]
impl<S: Meta> describe for Wrap<S> {
    const LIMIT: u32 = 42;

    fn sum(&self, x: u32, y: u32) -> String {
        (self.value + u32::from(self.extra) + x + y).to_string()
    }

    fn tag(&self) -> &'static str {
        "numbers"
    }
}

#[group_impl(Words)]
impl<S: Meta + State> describe for Wrap<S> {
    const LIMIT: u32 = 10;

    fn sum(&self, x: u32, y: u32) -> String {
        format!("{}{}{x}{y}", self.value.to_uppercase(), self.extra)
    }
}

#[test]
fn group_impl_dispatches_to_the_states_group() {
    let a = Wrap::<A> { value: 1, extra: 2 };
    let b = Wrap::<B> { value: 1, extra: 2 };
    let c = Wrap::<C> {
        value: "hi".into(),
        extra: '!',
    };

    assert_eq!(
        (a.sum(3, 4), b.sum(3, 4), c.sum(3, 4)),
        ("10".into(), "10".into(), "HI!34".into())
    );
    assert_eq!((b.tag(), c.tag()), ("numbers", "default"));
    assert_eq!((a.doubled(3), c.doubled(3)), ("9".into(), "HI!33".into()));
}

#[test]
fn group_impl_consts_reach_the_trait() {
    assert_eq!(<Wrap<A> as describe>::LIMIT, 42);
    assert_eq!(<Wrap<C> as describe>::LIMIT, 10);
}

#[typestate(state = S)]
struct Pair<S: Meta, I> {
    value: S::Value,
    extra: I,
}

#[group_trait(by = Meta)]
trait Total {
    fn total(&self) -> u32;
}

// The `Item = u32` bindings bound `I`, not the state, so `#[group_impl]`
// keeps them, inline and in the `where` clause.
#[group_impl(Numbers, state = S)]
impl<S: Meta, I: Iterator<Item = u32> + Clone> Total for Pair<S, I>
where
    I: ExactSizeIterator<Item = u32>,
{
    fn total(&self) -> u32 {
        self.value + self.extra.clone().sum::<u32>()
    }
}

#[test]
fn group_impl_keeps_bindings_on_other_parameters() {
    let pair = Pair::<A, _> {
        value: 1,
        extra: [2, 3].into_iter(),
    };
    assert_eq!(pair.total(), 6);
}
