// Misuse of `#[typestate]`.
#![allow(dead_code)]
use typestate_groups_macros::{group, state, state_types, typestate};

#[state_types]
trait Meta {
    type Value;
}

#[state]
struct Small;

#[group(SmallGroup)]
impl Meta for (Small,) {
    #[size(1)]
    type Value = u8;
}

#[typestate(State = S)]
struct BadKey<S> {
    s: S,
}

#[typestate(state = X)]
struct UnknownParam<S> {
    s: S,
}

#[typestate(state = 'a)]
struct Lifetime<'a> {
    s: &'a str,
}

#[typestate]
struct Ambiguous<A, B> {
    a: A,
    b: B,
}

#[typestate(unsafe_transmute = true, align = 8)]
struct AmbiguousWithAlign<S: Meta, T> {
    value: S::Value,
    other: T,
}

#[typestate(state = S, align = 8)]
struct AlignWithoutTransmute<S: Meta> {
    value: S::Value,
}

#[typestate(state = S, unsafe_transmute = false, align = 8)]
struct AlignWithTransmuteOff<S: Meta> {
    value: S::Value,
}

#[typestate(state = S, unsafe_transmute = true, align = 3)]
struct NotPowerOfTwo<S: Meta> {
    value: S::Value,
}

#[typestate(state = S, unsafe_transmute = true, align = 1073741824)]
struct TooLarge<S: Meta> {
    value: S::Value,
}

#[typestate(state = S)]
struct MarkerField<S: Meta> {
    value: S::Value,
    marker: S::Marker,
}

#[typestate(state = S, unsafe_transmute = true)]
struct NoProjection<S> {
    tag: u8,
    _s: core::marker::PhantomData<S>,
}

#[typestate(state = S, unsafe_transmute = true)]
#[repr(Rust)]
struct ReprRust<S: Meta> {
    value: S::Value,
    tag: u8,
}

// Fields that mention the state without being a bare `S::Assoc` or an
// `Indirect` pointer.
#[typestate(state = S, unsafe_transmute = true)]
struct Arrayed<S: Meta> {
    values: [S::Value; 2],
}

#[typestate(state = S, unsafe_transmute = true)]
struct BareState<S: Meta> {
    value: S::Value,
    marker: S,
}

#[typestate(state = S, unsafe_transmute = true)]
struct Qualified<S: Meta> {
    value: <S as Meta>::Value,
}

#[typestate(state = S, unsafe_transmute = true)]
struct Wrapped<S: Meta> {
    value: Option<S::Value>,
}

// std gives `Option` a null niche only around non-null pointers.
#[typestate(state = S, unsafe_transmute = true)]
struct OptionalRaw<S: Meta> {
    value: Option<*const S::Value>,
}

// `Vec` may order its fields differently per element type, and `Result`
// holds its value inline.
#[typestate(state = S, unsafe_transmute = true)]
struct Listed<S: Meta> {
    values: Vec<S::Value>,
}

#[typestate(state = S, unsafe_transmute = true)]
struct Fallible<S: Meta> {
    value: Result<S::Value, ()>,
}

// A `PhantomData` holds nothing to transmute.
#[typestate(state = S, unsafe_transmute = true)]
struct PhantomOnly<S: Meta> {
    _value: core::marker::PhantomData<S::Value>,
}

// A struct that points at itself holds the state's types only by value.
#[typestate(state = S, unsafe_transmute = true)]
struct SelfAndBoxed<S: Meta> {
    value: Box<S::Value>,
    next: Option<core::ptr::NonNull<SelfAndBoxed<S>>>,
}

fn main() {}
