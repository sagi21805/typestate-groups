//! Pointer types whose layout doesn't depend on their pointee.

use core::ptr::NonNull;

use crate::Access;

/// Reaches [`Pointee`](Indirect::Pointee) through a pointer, so a
/// `#[typestate(unsafe_transmute = true)]` field of this type can point
/// at a state-dependent type. Each cast method names the [`Access`] the
/// pointee gets, which [`crate::Permits`] checks.
///
/// Implement it, with [`Repointed`], for your own pointer:
///
/// ```
/// use core::ptr::NonNull;
/// use typestate_groups::{Indirect, ReadWriteShared, Repointed};
///
/// #[repr(transparent)]
/// struct Handle<T>(NonNull<T>);
///
/// // SAFETY: `Handle` holds `T` only behind its `NonNull`, which anyone
/// // may read or write through.
/// unsafe impl<T> Indirect for Handle<T> {
///     type Pointee = T;
///     type CastState = ReadWriteShared;
///     type CastStateRef = ReadWriteShared;
///     type CastStateMut = ReadWriteShared;
/// }
///
/// // SAFETY: `Handle<U>` is a `NonNull<U>`, laid out like `NonNull<T>`.
/// unsafe impl<T, U> Repointed<Handle<T>> for Handle<U> {}
/// ```
///
/// # Safety
///
/// Each access must cover everyone who may see a `Pointee` that `Self`
/// holds, inline or behind its pointer, while the container, cast with
/// that method, lives, and after it ends. Pick each one by who else can
/// reach the pointee.
///
/// - [`Read`](crate::Read) only when moving the container moves the only
///   path to the pointee, as for `Box` cast by value.
/// - [`ReadWrite`](crate::ReadWrite) when the pointer is unique but the
///   pointee's owner reads it again once the cast ends, as for `&mut`.
/// - [`ReadShared`](crate::ReadShared) when others may hold `&` to the
///   pointee, as in every
///   [`cast_state_ref`](crate::Isomorphic::cast_state_ref) and for every
///   pointer that can be copied or cloned, such as `&` and `Arc`.
/// - [`ReadWriteShared`](crate::ReadWriteShared) when anyone may read or
///   write the pointee, as through a raw pointer.
/// - An inline `Pointee` needs at least the access an inline field gets,
///   which is `Read` by value, `ReadShared` through `&` and `ReadWrite`
///   through `&mut`.
#[diagnostic::on_unimplemented(
    message = "make this field `S::Assoc`, a ZST, a type without the \
               state, or a pointer that implements `Indirect`; or remove \
               `unsafe_transmute = true` and convert with `morph`",
    label = "`{Self}` is not a pointer to the state's types",
    note = "`Indirect` is implemented for `*const`, `*mut`, `NonNull`, \
            `&`, `&mut`, `Box`, `Arc`, `Rc`, and `Option` of the last six"
)]
pub unsafe trait Indirect {
    /// The type behind the pointer.
    type Pointee;
    /// How the pointee is used when the container is cast with
    /// [`cast_state`](crate::Isomorphic::cast_state).
    type CastState: Access;
    /// How the pointee is used when the container is cast with
    /// [`cast_state_ref`](crate::Isomorphic::cast_state_ref).
    type CastStateRef: Access;
    /// How the pointee is used when the container is cast with
    /// [`cast_state_mut`](crate::Isomorphic::cast_state_mut).
    type CastStateMut: Access;
}

/// `Self` is `T`. A `#[typestate]` field that names its own struct
/// requires it of its [`Pointee`](Indirect::Pointee), so the cast checks
/// the struct once instead of recursing into it.
///
/// # Safety
///
/// `Self` must be `T`.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "point this field at exactly `{T}`, not `{Self}`: a field \
               that names its own struct must point at it",
    note = "or convert with `morph`"
)]
pub unsafe trait SameType<T> {}

// SAFETY: `T` is `T`.
unsafe impl<T> SameType<T> for T {}

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

// SAFETY: a pointer to a sized `T` is one address whatever `T` is.
// Anyone may read or write through a raw pointer. Others may read
// through `&`. The owner of a `&mut` reads it again once the borrow
// ends, unless the cast is only through `&`.
indirect! {
    impl<T, U> *const T => *const U:
    cast(
            val = ReadWriteShared,
            ref = ReadWriteShared,
            mut = ReadWriteShared,
        );
    impl<T, U> *mut T => *mut U:
    cast(
        val = ReadWriteShared,
        ref = ReadWriteShared,
        mut = ReadWriteShared,
    );
    impl<T, U> NonNull<T> => NonNull<U>:
    cast(
        val = ReadWriteShared,
        ref = ReadWriteShared,
        mut = ReadWriteShared,
    );
    impl<'a, T, U> &'a T => &'a U:
    cast(val = ReadShared, ref = ReadShared, mut = ReadShared);
    impl<'a, T, U> &'a mut T => &'a mut U:
    cast(val = ReadWrite, ref = ReadShared, mut = ReadWrite);
}

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

// SAFETY: std guarantees `Option` of each a null niche.
unsafe impl<T> NullNiche for NonNull<T> {}
unsafe impl<T> NullNiche for &T {}
unsafe impl<T> NullNiche for &mut T {}

#[cfg(feature = "alloc")]
mod alloc_impls {
    use super::NullNiche;
    use alloc::{boxed::Box, rc::Rc, sync::Arc};

    // SAFETY: `Box<T>` is one pointer, as std guarantees, and only it
    // reaches its pointee, so the pointee gets the container's access.
    // `Arc<T>` and `Rc<T>` are one `NonNull` to a `repr(C)` header
    // followed by `T`, which std doesn't document; `LAYOUT_CHECK` still
    // compares their size. Others may read through them.
    indirect! {
        impl<T, U> Box<T> => Box<U>:
            cast(val = Read, ref = ReadShared, mut = ReadWrite);
        impl<T, U> Arc<T> => Arc<U>:
            cast(val = ReadShared, ref = ReadShared, mut = ReadShared);
        impl<T, U> Rc<T> => Rc<U>:
            cast(val = ReadShared, ref = ReadShared, mut = ReadShared);
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
    type CastState = P::CastState;
    type CastStateRef = P::CastStateRef;
    type CastStateMut = P::CastStateMut;
}

// SAFETY: both have the layouts of `P` and `Q`, which `Repointed` makes
// equal.
unsafe impl<P: NullNiche, Q: NullNiche + Repointed<P>> Repointed<Option<P>>
    for Option<Q>
{
}
