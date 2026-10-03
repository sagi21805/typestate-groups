#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
mod macros;

mod cast;
mod error_helpers;
mod indirect;
mod layout;
mod morph;
mod transmute;

pub use cast::{
    ByMut, ByRef, ByValue, CastFrom, CastIndirect, CastPointee,
    CastReceiver, CastRefFrom, CastValid, CastableState,
};
pub use error_helpers::PinnedLayout;
pub use indirect::{
    Aliasing, BorrowedPointee, Indirect, NullNiche, Repointed,
    SharedPointee, UniquePointee, UnknownPointee,
};
pub use layout::{
    PinnedTypeAlignment, PinnedTypeLayout, PinnedTypeSize, SameAlignment,
    SameLayout, SameSize, TypeAlignment, TypeLayout, TypeSize,
    UnpinnedTypeAlignment, UnpinnedTypeLayout, UnpinnedTypeSize,
};
pub use morph::{MorphFrom, Morphic, TryMorphFrom};
pub use transmute::{Isomorphic, TransmutableState};
pub use typestate_groups_macros::*;
pub use zerocopy;

/// A type that represents a state of an object.
pub trait State {}

/// A type that has a state.
pub trait WithState {
    type State: State;
}

/// `Self` with its state replaced by `To` and every other generic
/// unchanged.
pub trait Restate<To: State>: WithState {
    /// `Self` in state `To`.
    type Target: WithState<State = To>;
}

/// A type that captures a specific implementation of a [`group_trait`].
pub trait Group {}

/// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
struct ReadmeDoctests;
