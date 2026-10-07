//! Bit-reinterpreting transitions between a container's states.

use crate::{
    ByMut, ByRef, ByValue, CastableState, Restate, State, WithState,
};

/// Bit-reinterprets a container as the same container in another
/// [`State`].
///
/// The `cast_state` methods are safe, and compile only when every field
/// stays valid. The `transmute_state` methods skip that proof and are
/// `unsafe`.
///
/// A field may point at another container, or at its own struct as in an
/// intrusive list. Every node such a pointer reaches is cast with the
/// container, so its projections must stay valid under the pointer's
/// access too. Containers that point at each other make rustc report
/// E0275; convert those with [`morph`](crate::Morphic::morph).
///
/// ```
/// use typestate_groups::{
///     Isomorphic, group, state, state_types, typestate,
/// };
///
/// #[state_types]
/// trait Meta {
///     type Value;
/// }
///
/// #[state]
/// struct Unsigned;
/// #[state]
/// struct Signed;
///
/// #[group(UnsignedGroup)]
/// impl Meta for (Unsigned,) {
///     #[size(4)]
///     type Value = u32;
/// }
///
/// #[group(SignedGroup)]
/// impl Meta for (Signed,) {
///     #[size(4)]
///     type Value = i32;
/// }
///
/// #[typestate(unsafe_transmute = true)]
/// struct Wrap<S: Meta> {
///     value: S::Value,
/// }
///
/// let mut unsigned = Wrap::<Unsigned> { value: u32::MAX };
/// assert_eq!(unsigned.cast_state_ref::<Signed>().value, -1);
///
/// unsigned.cast_state_mut::<Signed>().value = -2;
/// assert_eq!(unsigned.value, u32::MAX - 1);
///
/// let signed = unsigned.cast_state::<Signed>();
/// assert_eq!(signed.value, -2);
/// ```
pub trait Isomorphic: WithState + Sized {
    /// Bit-reinterprets `self` in state `To`.
    ///
    /// # Safety
    ///
    /// The bits of `self` must be a valid target. Prefer
    /// [`cast_state`](Isomorphic::cast_state), which proves it, and run
    /// `cargo miri test` on code that calls this.
    unsafe fn transmute_state<To: State>(
        self,
    ) -> <Self as Restate<To>>::Target
    where
        Self: TransmutableState<To>,
    {
        const { Self::LAYOUT_CHECK }
        let value = core::mem::ManuallyDrop::new(self);
        // SAFETY: `TransmutableState` guarantees `Self` and `Target` share
        // a layout, and `ManuallyDrop` keeps `self` from being dropped
        // twice.
        unsafe {
            core::mem::transmute_copy::<Self, <Self as Restate<To>>::Target>(
                &*value,
            )
        }
    }

    /// Bit-reinterprets `&self` in state `To`.
    ///
    /// # Safety
    ///
    /// The bits of `self` must be a valid target, and no field that
    /// changes type may hold an `UnsafeCell` in either state. Prefer
    /// [`cast_state_ref`](Isomorphic::cast_state_ref), which proves it,
    /// and run `cargo miri test` on code that calls this.
    unsafe fn transmute_state_ref<To: State>(
        &self,
    ) -> &<Self as Restate<To>>::Target
    where
        Self: TransmutableState<To>,
    {
        const { Self::LAYOUT_CHECK }
        // SAFETY: `TransmutableState` guarantees `Self` and `Target` share
        // a layout, and the borrow keeps `self`'s lifetime.
        unsafe {
            &*(self as *const Self as *const <Self as Restate<To>>::Target)
        }
    }

    /// Bit-reinterprets `&mut self` in state `To`.
    ///
    /// # Safety
    ///
    /// The bits of `self` must be a valid target, and every value
    /// written through the result must be valid back in `Self`'s state.
    /// Prefer [`cast_state_mut`](Isomorphic::cast_state_mut), which
    /// proves it, and run `cargo miri test` on code that calls this.
    unsafe fn transmute_state_mut<To: State>(
        &mut self,
    ) -> &mut <Self as Restate<To>>::Target
    where
        Self: TransmutableState<To>,
    {
        const { Self::LAYOUT_CHECK }
        // SAFETY: `TransmutableState` guarantees `Self` and `Target` share
        // a layout, and the borrow keeps `self`'s lifetime and uniqueness.
        unsafe {
            &mut *(self as *mut Self as *mut <Self as Restate<To>>::Target)
        }
    }

    /// Reinterprets `self` in state `To`.
    ///
    /// Compiles only when every field and pointee that changes type
    /// holds bits valid in `To`, as [`CastableState`] proves.
    fn cast_state<To: State>(self) -> <Self as Restate<To>>::Target
    where
        Self: CastableState<To, ByValue>,
    {
        const { <Self as CastableState<To, ByValue>>::POINTEE_CHECK }
        // SAFETY: `CastableState<To, ByValue>` proves every field of
        // `self` valid in `To`.
        unsafe { self.transmute_state() }
    }

    /// Reinterprets `&self` in state `To`.
    ///
    /// Compiles only when every field and pointee that changes type
    /// holds bits valid in `To`, and no field that changes type holds an
    /// `UnsafeCell` in either state, as [`CastableState`] proves.
    fn cast_state_ref<To: State>(&self) -> &<Self as Restate<To>>::Target
    where
        Self: CastableState<To, ByRef>,
    {
        const { <Self as CastableState<To, ByRef>>::POINTEE_CHECK }
        // SAFETY: `CastableState<To, ByRef>` proves every field of `self`
        // valid in `To` and free of cells that could be written through
        // the alias.
        unsafe { self.transmute_state_ref() }
    }

    /// Reinterprets `&mut self` in state `To`.
    ///
    /// Compiles only when every field and pointee that changes type
    /// holds bits valid in `To`, and every value written through the
    /// result is valid back in `Self`'s state, as [`CastableState`]
    /// proves.
    fn cast_state_mut<To: State>(
        &mut self,
    ) -> &mut <Self as Restate<To>>::Target
    where
        Self: CastableState<To, ByMut>,
    {
        const { <Self as CastableState<To, ByMut>>::POINTEE_CHECK }
        // SAFETY: `CastableState<To, ByMut>` proves every field valid
        // in both states, so `self` stays valid after the borrow ends.
        unsafe { self.transmute_state_mut() }
    }
}

impl<T: WithState> Isomorphic for T {}

/// `Self` keeps its layout in state `To`.
///
/// # Safety
///
/// `Self` and `Target` must be the same container with the same field
/// offsets, size and alignment.
#[diagnostic::on_unimplemented(
    message = "add `unsafe_transmute = true` to the `#[typestate]` of \
               `{Self}` to transmute it into state `{To}`",
    note = "both states' groups need the same `#[size(N)]`, and the \
            target must be the same struct",
    note = "or convert by value with `morph`, which needs neither"
)]
pub unsafe trait TransmutableState<To: State>:
    Restate<To> + Sized
{
    /// Compile-time check that `Self` and [`Target`](Restate::Target)
    /// share a size and alignment. `#[typestate]` overrides it to also
    /// compare every field's offset.
    const LAYOUT_CHECK: () = {
        assert!(
            core::mem::size_of::<Self>()
                == core::mem::size_of::<<Self as Restate<To>>::Target>(),
            "`Self` and `Target` must have the same size"
        );
        assert!(
            core::mem::align_of::<Self>()
                == core::mem::align_of::<<Self as Restate<To>>::Target>(),
            "`Self` and `Target` must have the same alignment"
        );
    };
}
