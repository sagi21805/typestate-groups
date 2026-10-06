//! `transmute_state` and its ref/mut forms. Runs under
//! `cargo +nightly miri test` too.

use core::{
    fmt::Debug,
    marker::{PhantomData, PhantomPinned},
    num::NonZeroU32,
    ptr::NonNull,
};
use typestate_groups::{Indirect, Isomorphic, ReadWriteShared, Repointed};
use typestate_groups_macros::{group, state, state_types, typestate};

#[state_types]
trait Meta {
    type Value;
}

#[state]
#[derive(Debug)]
struct Unsigned;
#[state]
#[derive(Debug)]
struct Signed;
#[state]
struct Byte;
#[state]
struct Flag;
#[state]
struct Letter;
#[state]
struct Count;
#[state]
struct Wide;
#[state]
struct Bytes;

#[group(UnsignedGroup)]
impl Meta for (Unsigned,) {
    #[size(4)]
    type Value = u32;
}

#[group(SignedGroup)]
impl Meta for (Signed,) {
    #[size(4)]
    type Value = i32;
}

#[group(ByteGroup)]
impl Meta for (Byte,) {
    #[size(1)]
    type Value = u8;
}

#[group(FlagGroup)]
impl Meta for (Flag,) {
    #[size(1)]
    type Value = bool;
}

#[group(LetterGroup)]
impl Meta for (Letter,) {
    #[size(4)]
    type Value = char;
}

#[group(CountGroup)]
impl Meta for (Count,) {
    #[size(4)]
    type Value = NonZeroU32;
}

#[group(WideGroup)]
impl Meta for (Wide,) {
    #[size(8)]
    type Value = u64;
}

#[group(BytesGroup)]
impl Meta for (Bytes,) {
    #[size(8)]
    type Value = [u8; 8];
}

#[typestate(unsafe_transmute = true)]
struct Wrap<S: Meta> {
    value: S::Value,
}

#[test]
fn transmute_state_by_value_ref_and_mut() {
    let unsigned = Wrap::<Unsigned> { value: 0xdead_beef };
    // SAFETY: every bit pattern is a valid `i32`.
    let mut signed = unsafe { unsigned.transmute_state::<Signed>() };
    assert_eq!(signed.value, 0xdead_beefu32 as i32);

    signed.value = 7;
    // SAFETY: every bit pattern is a valid `u32`.
    let unsigned: &Wrap<Unsigned> =
        unsafe { signed.transmute_state_ref() };
    assert_eq!(unsigned.value, 7);

    // SAFETY: every bit pattern is a valid `u32` and `i32`.
    unsafe { signed.transmute_state_mut::<Unsigned>() }.value = 9;
    assert_eq!(signed.value, 9);
}

#[test]
fn transmute_state_into_valid_bits_of_a_narrower_type() {
    let byte = Wrap::<Byte> { value: 1 };
    // SAFETY: 1 is a valid `bool`.
    let flag: Wrap<Flag> = unsafe { byte.transmute_state() };
    assert!(flag.value);

    let unsigned = Wrap::<Unsigned> { value: 'z' as u32 };
    // SAFETY: `'z' as u32` is a valid `char`.
    let letter: Wrap<Letter> = unsafe { unsigned.transmute_state() };
    assert_eq!(letter.value, 'z');

    let mut unsigned = Wrap::<Unsigned> { value: 7 };
    // SAFETY: 7 is a valid `NonZeroU32`, and the borrow writes none.
    let count: &mut Wrap<Count> =
        unsafe { unsigned.transmute_state_mut() };
    assert_eq!(count.value.get(), 7);
}

/// The projection need not come first, ZSTs and fields without `S` ride
/// along, and `T` carries over into the target.
#[typestate(state = S, unsafe_transmute = true)]
struct Mixed<S: Meta, T> {
    head: u16,
    value: S::Value,
    tail: T,
    _unit: (),
    _pinned: PhantomPinned,
    _state: PhantomData<fn() -> S>,
}

#[test]
fn transmute_state_keeps_the_other_fields() {
    let unsigned = Mixed::<Unsigned, String> {
        head: 1,
        value: 2,
        tail: "owned".into(),
        _unit: (),
        _pinned: PhantomPinned,
        _state: PhantomData,
    };
    // SAFETY: every bit pattern is a valid `i32`.
    let signed: Mixed<Signed, String> =
        unsafe { unsigned.transmute_state::<Signed>() };

    assert_eq!(
        (signed.head, signed.value, signed.tail.as_str()),
        (1, 2, "owned")
    );
}

/// `u64` and `[u8; 8]` differ in alignment, so `align = 8` pins it.
#[typestate(state = S, unsafe_transmute = true, align = 8)]
struct Forced<S: Meta> {
    value: S::Value,
    tag: u8,
}

#[test]
fn forced_alignment_bridges_differently_aligned_states() {
    assert_eq!(align_of::<Forced<Bytes>>(), 8);

    let wide = Forced::<Wide> {
        value: u64::from_ne_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
        tag: 9,
    };
    // SAFETY: every bit pattern is a valid `[u8; 8]`.
    let bytes = unsafe { wide.transmute_state::<Bytes>() };

    assert_eq!((bytes.value, bytes.tag), ([1, 2, 3, 4, 5, 6, 7, 8], 9));
}

// A parenthesised projection, or one substituted through a `macro_rules!`
// `$ty` (which reaches the attribute wrapped in an invisible group), is
// still a bare projection. Bounds next to the `#[state_types]` trait
// don't stop the transmute.
#[typestate(state = S, unsafe_transmute = true)]
#[expect(unused_parens, reason = "checks a parenthesised projection")]
struct Paren<S: Meta + Debug> {
    value: (S::Value),
}

macro_rules! via_macro {
    ($ty:ty) => {
        #[typestate(state = S, unsafe_transmute = true)]
        struct ViaMacro<S: Meta> {
            value: $ty,
        }
    };
}
via_macro!(S::Value);

#[test]
fn transmute_state_sees_through_parens_and_macro_groups() {
    // SAFETY: every bit pattern is a valid `i32`.
    let signed: Paren<Signed> =
        unsafe { Paren::<Unsigned> { value: 3 }.transmute_state() };
    assert_eq!(signed.value, 3);

    // SAFETY: every bit pattern is a valid `i32`.
    let signed: ViaMacro<Signed> =
        unsafe { ViaMacro::<Unsigned> { value: 5 }.transmute_state() };
    assert_eq!(signed.value, 5);
}

#[state_types]
trait Payload {
    type Item;
}

#[state]
struct Text;
#[state]
struct Number;

#[group(TextGroup)]
impl Payload for (Text,) {
    type Item = String;
}

#[group(NumberGroup)]
impl Payload for (Number,) {
    type Item = u64;
}

/// A pointer of the user's own.
#[repr(transparent)]
struct Handle<T>(NonNull<T>);

// SAFETY: `Handle` holds `T` only behind its `NonNull`, which anyone may
// read or write through.
unsafe impl<T> Indirect for Handle<T> {
    type Pointee = T;
    type CastState = ReadWriteShared;
    type CastStateRef = ReadWriteShared;
    type CastStateMut = ReadWriteShared;
}

// SAFETY: `Handle<U>` is a `NonNull<U>`, laid out like `NonNull<T>`.
unsafe impl<T, U> Repointed<Handle<T>> for Handle<U> {}

type Ptr<T> = NonNull<T>;

/// A pointer keeps its layout whatever it points at, so its pointee needs
/// no `#[size(N)]`. Aliases and user pointers count too.
#[typestate(unsafe_transmute = true)]
struct Pointers<'a, S: Payload> {
    raw: *const S::Item,
    non_null: NonNull<S::Item>,
    nullable: Option<NonNull<S::Item>>,
    shared: Option<&'a S::Item>,
    boxed: Option<Box<S::Item>>,
    aliased: Ptr<S::Item>,
    handle: Handle<S::Item>,
    _item: PhantomData<S::Item>,
}

#[test]
fn transmute_state_keeps_pointer_addresses() {
    let text = String::from("pointee");
    let ptr = NonNull::from(&text);
    let pointers = Pointers::<Text> {
        raw: ptr.as_ptr(),
        non_null: ptr,
        nullable: None,
        shared: None,
        boxed: None,
        aliased: ptr,
        handle: Handle(ptr),
        _item: PhantomData,
    };
    // SAFETY: the pointers are never read as `u64`, and the `Option`s are
    // `None`.
    let number: Pointers<Number> = unsafe { pointers.transmute_state() };

    assert_eq!(number.raw.cast(), ptr.as_ptr());
    assert_eq!(number.non_null.cast(), ptr);
    assert!(number.nullable.is_none());
    assert_eq!(number.aliased.cast(), ptr);
    assert_eq!(number.handle.0.cast(), ptr);
}
