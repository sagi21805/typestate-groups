//! States with generic parameters, and groups that carry them.

use core::{marker::PhantomData, ptr::NonNull};
use typestate_groups::{Isomorphic, WithState};
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};
use zerocopy::{FromBytes, Immutable, IntoBytes};

trait Slab {
    const SIZE: usize;
}

struct Small;
impl Slab for Small {
    const SIZE: usize = 8;
}

#[state]
struct FreeHead<T>(PhantomData<fn() -> T>);
#[state]
struct FreeAttached<T>(PhantomData<fn() -> T>);
#[state]
struct FreeDetached<T>(PhantomData<fn() -> T>);
#[state]
struct FullHead<T>(PhantomData<fn() -> T>);
#[state]
struct FullAttached<T>(PhantomData<fn() -> T>);
#[state]
struct FullDetached<T>(PhantomData<fn() -> T>);

#[state_types]
trait SlabState {
    type Meta;
}

#[state_types]
trait SlabList {
    type Head: SlabState;
    type Attached: SlabState;
    type Detached: SlabState;
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(transparent)]
struct FullFreeMeta(u64);

// `Meta` doesn't depend on `T`, so the group carries no parameter.
#[group(DoublyLinked)]
impl<T: Slab> SlabState
    for (
        FreeHead<T>,
        FreeAttached<T>,
        FreeDetached<T>,
        FullHead<T>,
        FullAttached<T>,
        FullDetached<T>,
    )
{
    #[size(8)]
    type Meta = FullFreeMeta;
}

#[group(FreeList<T>)]
impl<T: Slab> SlabList
    for (FreeHead<T>, FreeAttached<T>, FreeDetached<T>)
{
    type Head = FreeHead<T>;
    type Attached = FreeAttached<T>;
    type Detached = FreeDetached<T>;
}

#[group(FullList<T>)]
impl<T: Slab> SlabList
    for (FullHead<T>, FullAttached<T>, FullDetached<T>)
{
    type Head = FullHead<T>;
    type Attached = FullAttached<T>;
    type Detached = FullDetached<T>;
}

#[typestate(state = S, unsafe_transmute = true)]
struct SlabDescriptor<T: Slab, S: SlabState> {
    state: S::Meta,
    objects: NonNull<T>,
    next: Option<NonNull<SlabDescriptor<T, S>>>,
}

#[group_trait(by = SlabList)]
trait Attach<T: Slab>: WithState<State: SlabList> {
    fn attach<'a>(
        &mut self,
        other: &'a mut SlabDescriptor<
            T,
            <Self::State as SlabList>::Detached,
        >,
    ) -> &'a mut SlabDescriptor<T, <Self::State as SlabList>::Attached>;
}

#[group_impl(FreeList<T>, state = S)]
impl<T: Slab, S: SlabState + SlabList> Attach<T> for SlabDescriptor<T, S> {
    fn attach<'a>(
        &mut self,
        other: &'a mut SlabDescriptor<T, S::Detached>,
    ) -> &'a mut SlabDescriptor<T, S::Attached> {
        let _: PhantomData<S::Detached> = PhantomData::<FreeDetached<T>>;
        let attached = other.cast_state_mut::<S::Attached>();
        attached.state.0 += T::SIZE as u64;
        attached.next = Some(NonNull::from(&mut *self).cast());
        attached
    }
}

#[group_impl(FullList<T>, state = S)]
impl<T: Slab, S: SlabState + SlabList> Attach<T> for SlabDescriptor<T, S> {
    fn attach<'a>(
        &mut self,
        other: &'a mut SlabDescriptor<T, S::Detached>,
    ) -> &'a mut SlabDescriptor<T, S::Attached> {
        let attached = other.cast_state_mut::<S::Attached>();
        attached.state.0 = 0;
        attached
    }
}

/// Generic over the object type, with no where-clause.
fn reattach<T: Slab>(
    detached: &mut SlabDescriptor<T, FreeDetached<T>>,
) -> &mut SlabDescriptor<T, FreeAttached<T>> {
    detached.cast_state_mut::<FreeAttached<T>>()
}

fn descriptor<S: SlabState<Meta = FullFreeMeta>>(
    objects: &mut Small,
    meta: u64,
) -> SlabDescriptor<Small, S> {
    SlabDescriptor {
        state: FullFreeMeta(meta),
        objects: NonNull::from(objects),
        next: None,
    }
}

#[test]
fn generic_states_dispatch_to_the_group_that_carries_their_parameter() {
    let mut objects = Small;
    let mut head = descriptor::<FreeHead<Small>>(&mut objects, 1);
    let mut detached = descriptor::<FreeDetached<Small>>(&mut objects, 2);

    let attached = head.attach(&mut detached);
    assert_eq!(attached.state.0, 10);
    assert_eq!(
        attached.next.map(NonNull::cast::<()>),
        Some(NonNull::from(&mut head).cast())
    );

    let mut full = descriptor::<FullHead<Small>>(&mut objects, 1);
    let mut detached = descriptor::<FullDetached<Small>>(&mut objects, 2);
    assert_eq!(full.attach(&mut detached).state.0, 0);
}

#[test]
fn generic_states_cast_in_code_generic_over_their_parameter() {
    let mut objects = Small;
    let mut detached = descriptor::<FreeDetached<Small>>(&mut objects, 3);
    reattach(&mut detached).state.0 = 4;
    assert_eq!(detached.state.0, 4);
}

#[state_types]
trait Block {
    type Bytes;
}

#[state]
struct Order<const N: usize>;

#[group(Orders<N>)]
impl<const N: usize> Block for (Order<N>,) {
    type Bytes = [u8; N];
}

#[state]
struct Borrowed<'a>(PhantomData<&'a ()>);

#[group(Borrows<'a>)]
impl<'a> Block for (Borrowed<'a>,) {
    type Bytes = &'a [u8];
}

#[test]
fn groups_carry_const_and_lifetime_parameters() {
    let order: <Order<4> as Block>::Bytes = [1; 4];
    let borrowed: <Borrowed<'_> as Block>::Bytes = &order;
    assert_eq!(borrowed.len(), 4);
}
