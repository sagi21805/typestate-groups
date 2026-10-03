//! Safe bit-reinterpreting transitions, checked field by field.

use crate::{
    BorrowedPointee, Indirect, Repointed, SharedPointee, State,
    TransmutableState, UniquePointee, UnknownPointee,
};

/// Every valid `Src` is a valid `Self` of the same size.
///
/// # Safety
///
/// The bytes of every valid `Src` must form a valid `Self` whenever both
/// have the same size.
#[diagnostic::on_unimplemented(
    message = "convert with `morph`: `{Src}` may hold bits that are not \
               a valid `{Self}`",
    note = "a cast needs `{Src}: zerocopy::IntoBytes` (no padding) and \
            `{Self}: zerocopy::FromBytes` (every bit pattern valid)"
)]
pub unsafe trait CastFrom<Src> {}

// SAFETY: `IntoBytes` gives `S` no uninitialized bytes, and `FromBytes`
// accepts every initialized bit pattern as a `D`.
#[diagnostic::do_not_recommend]
unsafe impl<S: zerocopy::IntoBytes, D: zerocopy::FromBytes> CastFrom<S>
    for D
{
}

/// Neither `Src` nor `Self` holds an `UnsafeCell`, so a `&Src` can be
/// read as a `&Self`.
///
/// # Safety
///
/// Neither `Self` nor `Src` may hold an `UnsafeCell`.
#[diagnostic::on_unimplemented(
    message = "convert with `morph`, or cast a field held by value with \
               `cast_state` or `cast_state_mut`: `{Src}` or `{Self}` may \
               hold an `UnsafeCell` that a shared alias could write \
               through",
    note = "a cast through `&`, or of a `&` or raw pointer's pointee, \
            also needs `{Src}` and `{Self}` to be `zerocopy::Immutable`"
)]
pub unsafe trait CastRefFrom<Src> {}

// SAFETY: `Immutable` rules out an `UnsafeCell` in either type.
#[diagnostic::do_not_recommend]
unsafe impl<S: zerocopy::Immutable, D: zerocopy::Immutable> CastRefFrom<S>
    for D
{
}

/// How a cast holds the container: [`ByValue`], [`ByRef`] or
/// [`ByMut`].
///
/// You don't pick a receiver by hand: [`cast_state`], [`cast_state_ref`]
/// and [`cast_state_mut`] cast through [`ByValue`], [`ByRef`] and
/// [`ByMut`]. Name one only in a bound, when generic code needs a cast,
/// such as `T: CastableState<To, ByRef>`.
///
/// The receiver decides which fields must stay valid, and in which
/// direction. A `&mut` field shows the difference. Here `Flag` holds a
/// `bool` and `Raw` a `u8`:
///
/// ```
/// # use typestate_groups::{Isomorphic, group, state, state_types, typestate};
/// # #[state_types]
/// # trait Byte {
/// #     type Value;
/// # }
/// # #[state]
/// # struct Flag;
/// # #[state]
/// # struct Raw;
/// # #[group(FlagGroup)]
/// # impl Byte for (Flag,) {
/// #     #[size(1)]
/// #     type Value = bool;
/// # }
/// # #[group(RawGroup)]
/// # impl Byte for (Raw,) {
/// #     #[size(1)]
/// #     type Value = u8;
/// # }
/// #[typestate(unsafe_transmute = true)]
/// struct RefMut<'a, S: Byte> {
///     value: &'a mut S::Value,
/// }
///
/// let mut owner = true;
/// let flag = RefMut::<Flag> { value: &mut owner };
///
/// let raw = flag.cast_state_ref::<Raw>(); // `&` can't write the `bool`
/// assert_eq!(*raw.value, 1); // reads `true` as the `u8` 1
///
/// // Doesn't compile, because it would allow UB:
/// // let raw = flag.cast_state::<Raw>(); // `raw` can write any `u8`
/// // *raw.value = 2; // writes the byte 2 into `owner`
/// // drop(raw); // ends the borrow
/// // if owner {} // UB: a `bool` must be 0 or 1
/// ```
///
/// [`cast_state`]: crate::Isomorphic::cast_state
/// [`cast_state_ref`]: crate::Isomorphic::cast_state_ref
/// [`cast_state_mut`]: crate::Isomorphic::cast_state_mut
pub trait CastReceiver {
    /// The receiver that checks the pointee of a `&mut` field
    /// ([`BorrowedPointee`]) when the container is cast through `Self`.
    type Borrowed: CastReceiver;
}

cast_receiver! {
    /// A cast by value, with [`cast_state`](crate::Isomorphic::cast_state).
    ByValue => ByMut;
    /// A cast through `&`, with
    /// [`cast_state_ref`](crate::Isomorphic::cast_state_ref).
    ByRef => ByRef;
    /// A cast through `&mut`, with
    /// [`cast_state_mut`](crate::Isomorphic::cast_state_mut).
    ByMut => ByMut;
}

/// Every valid `Src` is a valid `Self` when a container holding it is
/// cast through `R`.
///
/// | `R` | Needs |
/// |---|---|
/// | [`ByValue`] | `Self: CastFrom<Src>` |
/// | [`ByRef`] | also `Self: CastRefFrom<Src>` |
/// | [`ByMut`] | also `Src: CastFrom<Self>` |
///
/// `&mut` needs both directions because the source sees what the target
/// wrote once the borrow ends. `&` rules out `UnsafeCell` because a shared
/// `Cell` viewed as another type could be written through the alias.
///
/// # Safety
///
/// Every valid `Src` must be a valid `Self`. For [`ByRef`], neither may
/// hold an `UnsafeCell`, and for [`ByMut`] every valid `Self` must be
/// a valid `Src`.
#[doc(hidden)]
pub unsafe trait CastValid<Src, R: CastReceiver> {}

// SAFETY: `CastFrom<S>` proves every valid `S` a valid `D`.
unsafe impl<S, D: CastFrom<S>> CastValid<S, ByValue> for D {}

// SAFETY: `CastFrom<S>` proves every valid `S` a valid `D`, and
// `CastRefFrom<S>` proves they are both immutable.
unsafe impl<S, D: CastFrom<S> + CastRefFrom<S>> CastValid<S, ByRef> for D {}

// SAFETY: `D: CastFrom<S>` proves every valid `S` a valid `D`, and
// `S: CastFrom<D>` proves every valid `D` a valid `S`.
unsafe impl<S: CastFrom<D>, D: CastFrom<S>> CastValid<S, ByMut> for D {}

/// The pointee `Src` stays valid as `Dst` when a container holding a
/// pointer with this [`Aliasing`](crate::Aliasing) is cast through `R`.
///
/// | Aliasing | Checks the pointee as |
/// |---|---|
/// | [`UniquePointee`] | `R` |
/// | [`SharedPointee`] | [`ByRef`] |
/// | [`BorrowedPointee`] | [`CastReceiver::Borrowed`] |
/// | [`UnknownPointee`] | both [`ByRef`] and [`ByMut`] |
#[doc(hidden)]
pub unsafe trait CastPointee<Src, Dst, R: CastReceiver> {}

// SAFETY: `CastValid<S, R>` proves the pointee valid for the container,
// the only one that reaches it.
unsafe impl<S, D: CastValid<S, R>, R: CastReceiver> CastPointee<S, D, R>
    for UniquePointee
{
}

// SAFETY: `CastValid<S, ByRef>` proves the pointee valid for the
// readers behind shared aliases, with no `UnsafeCell` to write through.
unsafe impl<S, D: CastValid<S, ByRef>, R: CastReceiver>
    CastPointee<S, D, R> for SharedPointee
{
}

// SAFETY: `CastValid<S, R::Borrowed>` proves the pointee valid for the
// container and for the owner, who reads it after the borrow ends.
unsafe impl<S, D: CastValid<S, R::Borrowed>, R: CastReceiver>
    CastPointee<S, D, R> for BorrowedPointee
{
}

// SAFETY: `CastValid<S, ByRef>` and `CastValid<S, ByMut>` prove the
// pointee valid both ways, with no `UnsafeCell`, for anyone who reads or
// writes it.
unsafe impl<
    S,
    D: CastValid<S, ByRef> + CastValid<S, ByMut>,
    R: CastReceiver,
> CastPointee<S, D, R> for UnknownPointee
{
}

/// `Src`'s pointee stays valid as `Self`'s when a container holding it
/// is cast through `R`, as `Src`'s [`Aliasing`](crate::Aliasing) decides
/// through [`CastPointee`].
///
/// # Safety
///
/// `Self` must be `Src` pointing at another type, whose pointee every
/// valid pointee of `Src` is, for everyone who may see it.
/// [`SAME_POINTEE_LAYOUT`](CastIndirect::SAME_POINTEE_LAYOUT) must be
/// `true` only when both pointees share a size and alignment.
#[doc(hidden)]
pub unsafe trait CastIndirect<Src: Indirect, R: CastReceiver>:
    Repointed<Src>
{
    /// Whether both pointees share a size and alignment, which
    /// `CastableState::POINTEE_CHECK` asserts.
    const SAME_POINTEE_LAYOUT: bool;
}

// SAFETY: `CastPointee` proves the pointee valid under `S`'s aliasing,
// and the const compares the pointees' layouts.
unsafe impl<S: Indirect, D: Repointed<S>, R: CastReceiver>
    CastIndirect<S, R> for D
where
    S::Aliasing: CastPointee<S::Pointee, D::Pointee, R>,
{
    const SAME_POINTEE_LAYOUT: bool = size_of::<S::Pointee>()
        == size_of::<D::Pointee>()
        && align_of::<S::Pointee>() == align_of::<D::Pointee>();
}

/// `Self` can be reinterpreted in state `To` through receiver `R` without
/// `unsafe`.
///
/// # Safety
///
/// Every valid `Self` must be a valid `Target`, and for [`ByMut`]
/// every valid `Target` a valid `Self`. For [`ByRef`], no field whose
/// type changes may hold an `UnsafeCell` in either state. Every pointee
/// must stay valid in the same way for every alias that may see it.
#[diagnostic::on_unimplemented(
    message = "add `unsafe_transmute = true` to the `#[typestate]` of \
               `{Self}` to cast it into state `{To}`",
    note = "or convert by value with `morph`"
)]
pub unsafe trait CastableState<To: State, R: CastReceiver>:
    TransmutableState<To>
{
    /// Compile-time check that every pointee keeps its size and
    /// alignment in state `To`. `#[typestate]` overrides it with one
    /// assertion per pointer field.
    const POINTEE_CHECK: () = ();
}
