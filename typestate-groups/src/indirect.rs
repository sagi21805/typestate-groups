//! Pointer types whose layout doesn't depend on their pointee.

use core::ptr::NonNull;

/// Holds [`Pointee`](Indirect::Pointee) only behind a pointer, so a
/// `#[typestate(unsafe_transmute = true)]` field of this type can point
/// at a state-dependent type.
///
/// Implement it, with [`Repointed`], for your own pointer:
///
/// ```
/// use core::ptr::NonNull;
/// use typestate_groups::{Indirect, Repointed, UnknownPointee};
///
/// #[repr(transparent)]
/// struct Handle<T>(NonNull<T>);
///
/// // SAFETY: `Handle` holds `T` only behind its `NonNull`, which anyone
/// // may alias.
/// unsafe impl<T> Indirect for Handle<T> {
///     type Pointee = T;
///     type Aliasing = UnknownPointee;
/// }
///
/// // SAFETY: `Handle<U>` is a `NonNull<U>`, laid out like `NonNull<T>`.
/// unsafe impl<T, U> Repointed<Handle<T>> for Handle<U> {}
/// ```
///
/// # Safety
///
/// `Self` must hold no `Pointee` inline, and
/// [`Aliasing`](Indirect::Aliasing) must cover everyone who may see the
/// pointee while `Self` lives.
#[diagnostic::on_unimplemented(
    message = "make this field `S::Assoc`, a ZST, a type without the \
               state, or a pointer that implements `Indirect`; or remove \
               `unsafe_transmute = true` and convert with `morph`",
    label = "`{Self}` may hold the state's types inline",
    note = "`Indirect` is implemented for `*const`, `*mut`, `NonNull`, \
            `&`, `&mut`, `Box`, `Arc`, `Rc`, and `Option` of the last six"
)]
pub unsafe trait Indirect {
    /// The type behind the pointer.
    type Pointee;
    /// Who else may see the pointee, which decides what a cast checks
    /// of it.
    type Aliasing: Aliasing;
}

/// `Self` is `Src` pointing at another type.
///
/// # Safety
///
/// `Self` and `Src` must have the same size, alignment and field
/// layout whenever their pointees have the same size and alignment.
#[diagnostic::on_unimplemented(
    message = "convert with `morph`: `{Self}` is not `{Src}` pointing at \
               another type",
    note = "implement `Repointed<{Src}>` for `{Self}` if they share a \
            layout"
)]
pub unsafe trait Repointed<Src: Indirect>: Indirect {}

/// `Option<Self>` has `Self`'s layout, with `None` as the null pointer.
///
/// # Safety
///
/// `Self` must never be null, and `Option<Self>` must have the same
/// layout as `Self`.
#[diagnostic::on_unimplemented(
    message = "wrap a non-null pointer in this `Option`, or convert with \
               `morph`: `Option<{Self}>` may not keep its layout across \
               states",
    label = "`Option<{Self}>` holds `{Self}` inline",
    note = "`NullNiche` is implemented for `NonNull`, `&`, `&mut`, \
            `Box`, `Arc` and `Rc`"
)]
pub unsafe trait NullNiche: Indirect {}

/// Who else may see an [`Indirect`]'s pointee: [`UniquePointee`],
/// [`SharedPointee`], [`BorrowedPointee`] or [`UnknownPointee`].
pub trait Aliasing {}

/// Only the pointer sees its pointee, as in `Box<T>`. A cast checks the
/// pointee as the container's access does.
pub enum UniquePointee {}

/// Others may read the pointee, as through `&T`, `Arc<T>` or `Rc<T>`. A
/// cast checks it as [`cast_state_ref`](crate::Isomorphic::cast_state_ref)
/// does, even by value.
pub enum SharedPointee {}

/// The pointee is borrowed from an owner who uses it again after the
/// borrow ends, as through `&mut T`. A cast
/// checks it as [`cast_state_mut`](crate::Isomorphic::cast_state_mut)
/// does, except through `&`.
pub enum BorrowedPointee {}

/// Anyone may read or write the pointee, as through `*const T`,
/// `*mut T` or `NonNull<T>`. A cast checks it both ways.
pub enum UnknownPointee {}

impl Aliasing for UniquePointee {}
impl Aliasing for SharedPointee {}
impl Aliasing for BorrowedPointee {}
impl Aliasing for UnknownPointee {}

// SAFETY: a pointer to a sized `T` is one address whatever `T` is, and
// each aliasing names who else may hold that address.
indirect! {
    impl<T, U> *const T => *const U: UnknownPointee;
    impl<T, U> *mut T => *mut U: UnknownPointee;
    impl<T, U> NonNull<T> => NonNull<U>: UnknownPointee;
    impl<'a, T, U> &'a T => &'a U: SharedPointee;
    impl<'a, T, U> &'a mut T => &'a mut U: BorrowedPointee;
}

// SAFETY: std guarantees `Option` of each a null niche.
unsafe impl<T> NullNiche for NonNull<T> {}
unsafe impl<T> NullNiche for &T {}
unsafe impl<T> NullNiche for &mut T {}

#[cfg(feature = "alloc")]
mod alloc_impls {
    use super::NullNiche;
    use alloc::{boxed::Box, rc::Rc, sync::Arc};

    // SAFETY: `Box<T>` is one pointer, as std guarantees. `Arc<T>` and
    // `Rc<T>` are one `NonNull` to a `repr(C)` header followed by `T`,
    // which std doesn't document; `LAYOUT_CHECK` still compares their
    // size.
    indirect! {
        impl<T, U> Box<T> => Box<U>: UniquePointee;
        impl<T, U> Arc<T> => Arc<U>: SharedPointee;
        impl<T, U> Rc<T> => Rc<U>: SharedPointee;
    }

    // SAFETY: std guarantees `Option<Box<T>>` a null niche. `Arc` and
    // `Rc` wrap a `NonNull`, which gives them one.
    unsafe impl<T> NullNiche for Box<T> {}
    unsafe impl<T> NullNiche for Arc<T> {}
    unsafe impl<T> NullNiche for Rc<T> {}
}

// SAFETY: `NullNiche` gives `Option<P>` the layout of `P`, and `None`
// holds no pointee.
unsafe impl<P: NullNiche> Indirect for Option<P> {
    type Pointee = P::Pointee;
    type Aliasing = P::Aliasing;
}

// SAFETY: both have the layouts of `P` and `Q`, which `Repointed` makes
// equal.
unsafe impl<P: NullNiche, Q: NullNiche + Repointed<P>> Repointed<Option<P>>
    for Option<Q>
{
}
