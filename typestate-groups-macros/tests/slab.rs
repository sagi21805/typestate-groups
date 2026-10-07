//! The slab allocator shape that generic state types, generic group traits
//! and self-pointing containers combine into, with no `unsafe` for the
//! state changes.

use core::ptr::NonNull;
use typestate_groups::{Isomorphic, WithState};
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};
use zerocopy::{FromBytes, Immutable, IntoBytes};

trait Slab {
    const PFLAGS: u32;
}

struct Small;
impl Slab for Small {
    const PFLAGS: u32 = 0b11;
}

#[state]
struct FreeHead;
#[state]
struct FreeAttached;
#[state]
struct FreeDetached;
#[state]
struct FullHead;
#[state]
struct FullAttached;
#[state]
struct FullDetached;

#[state_types]
trait SlabState<T: Slab> {
    type Meta;
    const PFLAGS: u32 = T::PFLAGS;
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(transparent)]
struct FullFreeMeta(u64);

#[group(DoublyLinked)]
impl<T: Slab> SlabState<T>
    for (
        FreeHead,
        FreeAttached,
        FreeDetached,
        FullHead,
        FullAttached,
        FullDetached,
    )
{
    #[size(8)]
    type Meta = FullFreeMeta;
}

#[state_types]
trait SlabList {
    type Head;
    type Attached;
    type Detached;
}

#[group(FreeList)]
impl SlabList for (FreeHead, FreeAttached, FreeDetached) {
    type Head = FreeHead;
    type Attached = FreeAttached;
    type Detached = FreeDetached;
}

#[group(FullList)]
impl SlabList for (FullHead, FullAttached, FullDetached) {
    type Head = FullHead;
    type Attached = FullAttached;
    type Detached = FullDetached;
}

#[typestate(state = S, unsafe_transmute = true)]
#[repr(C)]
struct SlabDescriptor<T: Slab, S: SlabState<T>> {
    state: S::Meta,
    objects: NonNull<T>,
    next: Option<NonNull<SlabDescriptor<T, S>>>,
}

#[group_trait(by = SlabList)]
trait Attach<T: Slab>:
    WithState<State: SlabList<Attached: SlabState<T>, Detached: SlabState<T>>>
{
    fn attach<'a>(
        &mut self,
        other: &'a mut SlabDescriptor<
            T,
            <Self::State as SlabList>::Detached,
        >,
    ) -> &'a mut SlabDescriptor<T, <Self::State as SlabList>::Attached>;
}

#[group_impl(FreeList, state = S)]
impl<T: Slab, S: SlabState<T> + SlabList> Attach<T>
    for SlabDescriptor<T, S>
{
    fn attach<'a>(
        &mut self,
        other: &'a mut SlabDescriptor<T, S::Detached>,
    ) -> &'a mut SlabDescriptor<T, S::Attached> {
        let attached = other.cast_state_mut::<S::Attached>();
        attached.next = Some(NonNull::from(&mut *self).cast());
        attached.state.0 = u64::from(S::PFLAGS);
        attached
    }
}

#[group_impl(FullList, state = S)]
impl<T: Slab, S: SlabState<T> + SlabList> Attach<T>
    for SlabDescriptor<T, S>
{
    fn attach<'a>(
        &mut self,
        other: &'a mut SlabDescriptor<T, S::Detached>,
    ) -> &'a mut SlabDescriptor<T, S::Attached> {
        other.cast_state_mut::<S::Attached>()
    }
}

fn descriptor<S: SlabState<Small, Meta = FullFreeMeta>>(
    objects: &mut Small,
) -> SlabDescriptor<Small, S> {
    SlabDescriptor {
        state: FullFreeMeta(0),
        objects: NonNull::from(objects),
        next: None,
    }
}

#[test]
fn slab_descriptors_attach_by_list() {
    let mut objects = Small;
    let mut head = descriptor::<FreeHead>(&mut objects);
    let mut detached = descriptor::<FreeDetached>(&mut objects);

    let attached = head.attach(&mut detached);
    assert_eq!(attached.state.0, 0b11);
    assert!(attached.next.is_some());

    let mut full = descriptor::<FullHead>(&mut objects);
    let mut detached = descriptor::<FullDetached>(&mut objects);
    let attached: &mut SlabDescriptor<Small, FullAttached> =
        full.attach(&mut detached);
    assert!(attached.next.is_none());
}
