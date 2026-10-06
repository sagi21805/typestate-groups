//! `cast_state` and its ref/mut forms between states whose fields stay
//! valid. Runs under `cargo +nightly miri test` too.

use core::{
    cell::Cell, marker::PhantomData, num::NonZeroU32, ptr::NonNull,
};
use std::{rc::Rc, sync::Arc};
use typestate_groups::Isomorphic;
use typestate_groups_macros::{group, state, state_types, typestate};
use zerocopy::{FromBytes, Immutable, IntoBytes};

#[state_types]
trait Meta {
    type Value;
    type Extra;
}

#[state]
struct Unsigned;
#[state]
struct Signed;
#[state]
struct Pixel;
#[state]
struct Letter;
#[state]
struct Count;

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[group(UnsignedGroup)]
impl Meta for (Unsigned,) {
    #[size(4)]
    type Value = u32;
    #[size(2)]
    type Extra = [u8; 2];
}

#[group(SignedGroup)]
impl Meta for (Signed,) {
    #[size(4)]
    type Value = i32;
    #[size(2)]
    type Extra = u16;
}

#[group(PixelGroup)]
impl Meta for (Pixel,) {
    #[size(4)]
    type Value = Rgba;
    #[size(2)]
    type Extra = i16;
}

#[group(LetterGroup)]
impl Meta for (Letter,) {
    #[size(4)]
    type Value = char;
    #[size(2)]
    type Extra = [u8; 2];
}

#[group(CountGroup)]
impl Meta for (Count,) {
    #[size(4)]
    type Value = NonZeroU32;
    #[size(2)]
    type Extra = [u8; 2];
}

/// Padding sits between `extra` and an 8-aligned `payload`.
#[typestate(state = S, unsafe_transmute = true, align = 8)]
struct Wrap<S: Meta, T> {
    value: S::Value,
    extra: S::Extra,
    payload: T,
    _state: PhantomData<S>,
}

fn wrap<S: Meta, T>(
    value: S::Value,
    extra: S::Extra,
    payload: T,
) -> Wrap<S, T> {
    Wrap {
        value,
        extra,
        payload,
        _state: PhantomData,
    }
}

#[test]
fn cast_state_reinterprets_every_projection() {
    let unsigned =
        wrap::<Unsigned, _>(u32::MAX, [0x34, 0x12], String::from("owned"));
    let signed = unsigned.cast_state::<Signed>();
    assert_eq!(signed.value, -1);
    assert_eq!(signed.extra, u16::from_ne_bytes([0x34, 0x12]));
    assert_eq!(signed.payload, "owned");

    let rgba = Rgba {
        r: 1,
        g: 2,
        b: 3,
        a: 4,
    };
    let unsigned = wrap::<Pixel, _>(rgba, -1, Box::new(5u64))
        .cast_state::<Unsigned>();
    assert_eq!(unsigned.value, u32::from_ne_bytes([1, 2, 3, 4]));
    assert_eq!(unsigned.extra, [0xff, 0xff]);
    assert_eq!(*unsigned.payload, 5);
}

#[test]
fn cast_state_out_of_types_with_invalid_bit_patterns() {
    let letter = wrap::<Letter, _>('z', [0, 0], ());
    assert_eq!(letter.cast_state::<Unsigned>().value, 'z' as u32);

    let count = wrap::<Count, _>(NonZeroU32::MIN, [0, 0], ());
    assert_eq!(count.cast_state_ref::<Unsigned>().value, 1);
}

#[test]
fn cast_state_ref_interleaves_with_other_shared_borrows() {
    let signed = wrap::<Signed, _>(-2, 0, vec![1u8, 2]);
    let plain = &signed;
    let unsigned = signed.cast_state_ref::<Unsigned>();
    let pixel = signed.cast_state_ref::<Pixel>();

    assert_eq!(unsigned.value, u32::MAX - 1);
    assert_eq!(plain.value, -2);
    assert_eq!(pixel.value.r, 0xfe);
    assert_eq!(unsigned.payload, plain.payload);
}

#[test]
fn cast_state_mut_writes_are_seen_after_the_borrow_ends() {
    let mut unsigned = wrap::<Unsigned, _>(0, [0, 0], String::new());

    {
        let pixel = unsigned.cast_state_mut::<Pixel>();
        pixel.value.a = 0x80;
        pixel.payload.push_str("written");
    }
    assert_eq!(unsigned.value, u32::from_ne_bytes([0, 0, 0, 0x80]));
    assert_eq!(unsigned.payload, "written");

    let signed = unsigned.cast_state_mut::<Signed>();
    signed.cast_state_mut::<Pixel>().value.r = 8;
    assert_eq!(unsigned.value.to_ne_bytes()[0], 8);
}

/// Every pointer kind, whose pointees cast like fields held by value.
#[typestate(unsafe_transmute = true)]
struct Pointers<'a, S: Meta> {
    raw: *mut S::Value,
    nullable: Option<NonNull<S::Value>>,
    shared: &'a S::Value,
    unique: &'a mut S::Value,
    boxed: Box<S::Value>,
    atomic: Arc<S::Value>,
    counted: Option<Rc<S::Value>>,
}

#[test]
fn cast_state_reinterprets_every_pointee() {
    let mut target = u32::MAX;
    let ptr = &raw mut target;
    let shared = 1;
    let mut unique = 2;
    let unsigned = Pointers::<Unsigned> {
        raw: ptr,
        nullable: NonNull::new(ptr),
        shared: &shared,
        unique: &mut unique,
        boxed: Box::new(3),
        atomic: Arc::new(u32::MAX),
        counted: Some(Rc::new(5)),
    };
    let atomic = Arc::clone(&unsigned.atomic);

    let view = unsigned.cast_state_ref::<Signed>();
    assert_eq!((*view.shared, *view.boxed), (1, 3));
    assert_eq!(view.counted.as_deref(), Some(&5));

    let mut signed = unsigned.cast_state::<Signed>();
    *signed.unique = -1;
    *signed.cast_state_mut::<Unsigned>().boxed = 4;
    assert_eq!(*signed.boxed, 4);
    // SAFETY: `ptr` points at `target`, which outlives `signed`.
    unsafe {
        assert_eq!(*signed.raw, -1);
        assert_eq!(signed.nullable.map(|p| *p.as_ptr()), Some(-1));
    }

    assert_eq!(*signed.atomic, -1);

    drop(signed);
    assert_eq!(unique, u32::MAX);
    assert_eq!(Arc::strong_count(&atomic), 1);
}

#[state_types]
trait Flagged {
    type Value;
}

#[state]
struct Flag;
#[state]
struct Byte;
#[state]
struct Shared;

#[group(FlagGroup)]
impl Flagged for (Flag,) {
    #[size(1)]
    type Value = bool;
}

#[group(ByteGroup)]
impl Flagged for (Byte,) {
    #[size(1)]
    type Value = u8;
}

#[group(SharedGroup)]
impl Flagged for (Shared,) {
    #[size(1)]
    type Value = Cell<u8>;
}

#[typestate(unsafe_transmute = true)]
struct Slot<S: Flagged> {
    value: S::Value,
}

#[test]
fn cast_state_turns_bools_and_cells_into_bytes() {
    let flag = Slot::<Flag> { value: true };
    assert_eq!(flag.cast_state_ref::<Byte>().value, 1);
    assert_eq!(flag.cast_state::<Byte>().value, 1);

    let mut shared = Slot::<Shared> {
        value: Cell::new(3),
    };
    shared.cast_state_mut::<Byte>().value = 4;
    assert_eq!(shared.value.get(), 4);
    assert_eq!(shared.cast_state::<Byte>().value, 4);
}
