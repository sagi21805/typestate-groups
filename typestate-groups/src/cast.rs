//! Safe bit-reinterpreting transitions, checked field by field.

use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::{State, TransmutableState};

/// How a cast holds the container: [`ByValue`], [`ByRef`] or [`ByMut`].
///
/// [`cast_state`] casts [`ByValue`], [`cast_state_ref`] [`ByRef`] and
/// [`cast_state_mut`] [`ByMut`], so you name one only in a bound, when
/// generic code needs a cast, such as `T: CastableState<To, ByRef>`.
///
/// How the cast holds the container decides which [`Access`] each field
/// gets. A `&mut` field shows the difference. Here `Flag` holds a `bool`
/// and `Raw` a `u8`:
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
/// let raw = flag.cast_state_ref::<Raw>(); // `ReadShared`: can't write the `bool`
/// assert_eq!(*raw.value, 1); // reads `true` as the `u8` 1
///
/// // Doesn't compile, because it would allow UB:
/// let raw = flag.cast_state::<Raw>(); // `ReadWrite`: can write any `u8`
/// *raw.value = 2; // writes the byte 2 into `owner`
/// drop(raw); // ends the borrow
/// if owner {} // UB: a `bool` must be 0 or 1
/// ```
///
/// [`cast_state`]: crate::Isomorphic::cast_state
/// [`cast_state_ref`]: crate::Isomorphic::cast_state_ref
/// [`cast_state_mut`]: crate::Isomorphic::cast_state_mut
pub trait CastBy {}

/// A cast by value, with [`cast_state`](crate::Isomorphic::cast_state).
pub enum ByValue {}

/// A cast through `&`, with
/// [`cast_state_ref`](crate::Isomorphic::cast_state_ref).
pub enum ByRef {}

/// A cast through `&mut`, with
/// [`cast_state_mut`](crate::Isomorphic::cast_state_mut).
pub enum ByMut {}

impl CastBy for ByValue {}
impl CastBy for ByRef {}
impl CastBy for ByMut {}

/// How the cast bytes are used once a cast ends, as [`Read`],
/// [`ReadShared`], [`ReadWrite`] or [`ReadWriteShared`]. [`Permits`]
/// says which bytes each one accepts.
///
/// An access answers two questions.
///
/// - Can others see the bytes during the cast? A cast that moves the only
///   path to the bytes is exclusive, because Rust doesn't move a borrowed
///   value, so no `&` to them is alive. A cast through `&`, or through a
///   pointer that can be copied or cloned, is shared.
/// - Does anyone read the bytes as the source after the cast? Then every
///   value the target writes must be a valid source too, so the bytes must
///   be valid both ways.
///
/// |           | valid one way  | valid both ways       |
/// |-----------|----------------|-----------------------|
/// | exclusive | [`Read`]       | [`ReadWrite`]         |
/// | shared    | [`ReadShared`] | [`ReadWriteShared`]   |
///
/// ```compile_fail,E0277
/// # use core::cell::Cell;
/// # use typestate_groups::{Isomorphic, group, state, state_types, typestate};
/// # #[state_types]
/// # trait Byte {
/// #     type Value;
/// # }
/// # #[state]
/// # struct Shared;
/// # #[state]
/// # struct Plain;
/// # #[group(SharedGroup)]
/// # impl Byte for (Shared,) {
/// #     #[size(1)]
/// #     type Value = Cell<u8>;
/// # }
/// # #[group(PlainGroup)]
/// # impl Byte for (Plain,) {
/// #     #[size(1)]
/// #     type Value = u8;
/// # }
/// #[typestate(unsafe_transmute = true)]
/// struct Ref<'a, S: Byte> {
///     value: &'a S::Value,
/// }
///
/// let cell = Cell::new(1u8);
/// let shared = Ref::<Shared> { value: &cell };
///
/// // Many `&Cell<u8>` to one byte are fine, since every alias agrees the
/// // byte may change.
/// let alias = shared.value;
/// alias.set(2);
///
/// // Doesn't compile, because adding, removing or moving a cell makes
/// // the aliases disagree. The shared accesses require `Immutable` on
/// // both sides.
/// let plain = shared.cast_state::<Plain>(); // `&u8` to the same byte
/// alias.set(3); // UB, because `plain` treats the byte as frozen
/// ```
pub trait Access {}

/// Only the target uses the bytes, and nobody reads them as the source
/// again, so they need to be valid one way only. A cell on either side
/// is fine, since no alias sees what is written through it.
pub enum Read {}

/// The target reads the bytes while others hold `&` to them. Neither side
/// may hold an `UnsafeCell`, so nobody writes the bytes and they need to
/// be valid one way only.
pub enum ReadShared {}

/// The target reads and writes the bytes, and the source reads them again
/// once the cast ends, so they must be valid both ways.
pub enum ReadWrite {}

/// Anyone may read or write the bytes, as through a raw pointer, so they
/// must be valid both ways, with no `UnsafeCell`.
pub enum ReadWriteShared {}

impl Access for Read {}
impl Access for ReadShared {}
impl Access for ReadWrite {}
impl Access for ReadWriteShared {}

/// The bytes of any valid `Src` form a valid `Dst` when used as the
/// [`Access`] `Self` describes. [`CastableState`] requires it of every
/// field and pointee that changes type.
///
/// # Safety
///
/// Every valid `Src` must be a valid `Dst` of the same size, used as
/// `Self` describes.
pub unsafe trait Permits<Src, Dst>: Access {}

// SAFETY: `IntoBytes` gives `S` no uninitialized bytes, and `FromBytes`
// accepts every initialized bit pattern as a `D`.
unsafe impl<S: IntoBytes, D: FromBytes> Permits<S, D> for Read {}

// SAFETY: as for `Read`, and `Immutable` rules out an `UnsafeCell` that a
// shared alias could write through.
unsafe impl<S: IntoBytes + Immutable, D: FromBytes + Immutable>
    Permits<S, D> for ReadShared
{
}

// SAFETY: `IntoBytes` and `FromBytes` on both sides make every valid `S`
// a valid `D` and every valid `D` a valid `S`.
unsafe impl<S: IntoBytes + FromBytes, D: IntoBytes + FromBytes>
    Permits<S, D> for ReadWrite
{
}

// SAFETY: as for `ReadWrite`, and `Immutable` rules out an `UnsafeCell`.
unsafe impl<
    S: IntoBytes + FromBytes + Immutable,
    D: IntoBytes + FromBytes + Immutable,
> Permits<S, D> for ReadWriteShared
{
}

/// `Self` can be reinterpreted in state `To`, cast by `B`, without
/// `unsafe`. Each field and pointee that changes type names the
/// [`Access`] it needs, and [`Permits`] checks it.
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
pub unsafe trait CastableState<To: State, B: CastBy>:
    TransmutableState<To>
{
    /// Compile-time check that every pointee keeps its size and
    /// alignment in state `To`. [`crate::typestate`] overrides it with one
    /// assertion per pointer field.
    const POINTEE_CHECK: () = ();
}
