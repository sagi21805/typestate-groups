// Misuse of `#[group]`.
#![allow(dead_code)]
use typestate_groups_macros::{group, state, state_types};

#[state_types]
trait Meta {
    type Value;
}

mod inner {
    pub trait Meta {
        type Value;
    }

    pub struct Hidden;
}

#[state]
struct A;
#[state]
struct B;
#[state]
struct C;
#[state]
struct D;

struct Foo;

#[group(Inherent)]
impl Foo {
    fn method() {}
}

#[group(NotTuple)]
impl Meta for A {
    type Value = u8;
}

#[group(PathTrait)]
impl inner::Meta for (A,) {
    type Value = u8;
}

#[group(RefState)]
impl Meta for (&A,) {
    type Value = u8;
}

#[group(OtherAttribute)]
impl Meta for (B,) {
    #[allow(dead_code)]
    type Value = u64;
}

#[group(DuplicateSize)]
impl Meta for (C,) {
    #[size(4)]
    #[size(8)]
    type Value = u64;
}

#[group(SizeNotInteger)]
impl Meta for (D,) {
    #[size("eight")]
    type Value = u64;
}

#[state]
struct Generic<T>(core::marker::PhantomData<T>);

// `Value` depends on `T`, which the group doesn't carry.
#[group(Unlisted)]
impl<T> Meta for (Generic<T>,) {
    type Value = Vec<T>;
}

#[group(NotAParam<U>)]
impl<T> Meta for (Generic<T>,) {
    type Value = u8;
}

#[group(NotAName<Vec<T>>)]
impl<T> Meta for (Generic<T>,) {
    type Value = u8;
}

#[group(GenericSize<T>)]
impl<T> Meta for (Generic<T>,) {
    #[size(8)]
    type Value = Option<Box<T>>;
}

#[state_types]
trait NoAssocType {
    fn describe(&self) -> String;
}

fn main() {}
