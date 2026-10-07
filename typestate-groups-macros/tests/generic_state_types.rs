//! `#[state_types]` traits with generic parameters.

use core::ptr::NonNull;
use typestate_groups::Isomorphic;
use typestate_groups_macros::{group, state, state_types, typestate};
use zerocopy::{FromBytes, Immutable, IntoBytes};

trait Slab {
    const SIZE: usize;
}

struct Small;
impl Slab for Small {
    const SIZE: usize = 8;
}

#[state]
struct FreeDetached;
#[state]
struct FullHead;
#[state]
struct PartialDetached;

#[state_types]
trait SlabState<T: Slab> {
    type Meta;

    /// Read from the trait's parameter, unless a group overrides it.
    const OBJECTS: usize = 64 / T::SIZE;

    /// Set by every group.
    const NAME: &'static str;
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(transparent)]
struct FullFreeMeta(u64);

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(transparent)]
struct PartialMeta(u64);

#[group(DoublyLinked)]
impl<T: Slab> SlabState<T> for (FreeDetached, FullHead) {
    #[size(8)]
    type Meta = FullFreeMeta;

    const NAME: &'static str = "doubly linked";
}

#[group(Partial)]
impl<T: Slab> SlabState<T> for (PartialDetached,) {
    #[size(8)]
    type Meta = PartialMeta;

    const OBJECTS: usize = 1;
    const NAME: &'static str = "partial";
}

#[typestate(state = S, unsafe_transmute = true)]
struct SlabDescriptor<T: Slab, S: SlabState<T>> {
    state: S::Meta,
    objects: NonNull<T>,
    next: Option<NonNull<SlabDescriptor<T, S>>>,
}

fn describe<T: Slab, S: SlabState<T>>() -> (usize, &'static str) {
    (S::OBJECTS, S::NAME)
}

#[test]
fn state_types_consts_default_from_the_trait_or_the_group() {
    assert_eq!(describe::<Small, FullHead>(), (8, "doubly linked"));
    assert_eq!(describe::<Small, PartialDetached>(), (1, "partial"));
}

#[test]
fn generic_state_types_cast_between_groups() {
    let mut objects = Small;
    let mut free = SlabDescriptor::<Small, FreeDetached> {
        state: FullFreeMeta(Small::SIZE as u64),
        objects: NonNull::from(&mut objects),
        next: None,
    };

    let partial = free.cast_state_mut::<PartialDetached>();
    assert_eq!(partial.state.0, 8);
    partial.state.0 = 3;
    assert_eq!(free.state.0, 3);
}

mod owning {
    use typestate_groups_macros::state_types;

    #[state_types]
    pub trait Owns<T> {
        type Item;
    }
}

// `Item` depends on the trait's `T`, so the group carries it.
#[group(Owning<T>)]
impl<T> owning::Owns<T> for (FreeDetached, FullHead) {
    type Item = Vec<T>;
}

#[test]
fn groups_carry_the_parameters_of_generic_state_types() {
    let items: <FullHead as owning::Owns<u8>>::Item = vec![1, 2];
    let _: Owning<u8> = Owning(core::marker::PhantomData);
    assert_eq!(items.len(), 2);
}
