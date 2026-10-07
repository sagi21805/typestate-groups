// `#[group]` and `#[group_impl]` misuse that rustc reports while resolving
// names, which an expansion error in the same file would hide.
#![allow(dead_code)]
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Meta {
    type Value;
}

trait Plain {
    type Value;
}

#[state]
struct A;

#[group(NoStateTypes)]
impl Plain for (A,) {
    type Value = u8;
}

#[typestate]
struct Wrap<S: Meta> {
    value: S::Value,
}

#[group_trait(by = Meta)]
trait Describe {
    fn describe(&self) -> String;
}

// TODO: `#[group_impl]` doesn't check that `#[group]` declared the name,
// so rustc reports a plain "cannot find type".
#[group_impl(NeverDeclared)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> String {
        String::new()
    }
}

#[state]
struct Generic<T>(core::marker::PhantomData<T>);
#[state]
struct Unit;

// `T` is unconstrained in `impl<T> Meta for Unit`.
#[group(Mixed)]
impl<T> Meta for (Generic<T>, Unit) {
    type Value = u8;
}

#[state]
struct Owned<T>(core::marker::PhantomData<T>);

#[group(Holding<T>)]
impl<T> Meta for (Owned<T>,) {
    type Value = Vec<T>;
}

// The group needs its argument, as in `#[group_impl(Holding<T>)]`.
#[group_impl(Holding)]
impl<S: Meta> Describe for Wrap<S> {
    fn describe(&self) -> String {
        String::new()
    }
}

fn main() {}
