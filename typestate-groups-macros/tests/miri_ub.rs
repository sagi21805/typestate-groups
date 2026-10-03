//! Unchecked `transmute_state` calls that Miri must reject as undefined
//! behaviour. `tests/ui/cast_state.rs` writes the same cases, under the
//! same names, with `cast_state`, and none of them compile.
//!
//! UB stops Miri instead of panicking, so `#[should_panic]` can't catch
//! it. Every case is ignored; run one at a time to see Miri reject it:
//!
//! ```sh
//! cargo +nightly miri test -p typestate-groups-macros --test miri_ub -- \
//!     --ignored --exact <name>
//! ```

use core::{cell::Cell, num::NonZeroU32, ptr::NonNull};
use typestate_groups::Isomorphic;
use typestate_groups_macros::{group, state, state_types, typestate};

#[state_types]
trait Byte {
    type Value;
}

#[state]
struct Raw;
#[state]
struct Flag;

#[group(RawGroup)]
impl Byte for (Raw,) {
    #[size(1)]
    type Value = u8;
}

#[group(FlagGroup)]
impl Byte for (Flag,) {
    #[size(1)]
    type Value = bool;
}

#[typestate(unsafe_transmute = true)]
struct Small<S: Byte> {
    value: S::Value,
}

#[state_types]
trait Word {
    type Value;
}

#[state]
struct Bits;
#[state]
struct Letter;
#[state]
struct Count;
#[state]
struct Shared;
#[state]
struct Gapped;
#[state]
struct Quad;

/// One padding byte after `small`.
#[repr(C)]
struct Padded {
    small: u8,
    wide: u16,
}

#[group(BitsGroup)]
impl Word for (Bits,) {
    #[size(4)]
    type Value = u32;
}

#[group(LetterGroup)]
impl Word for (Letter,) {
    #[size(4)]
    type Value = char;
}

#[group(CountGroup)]
impl Word for (Count,) {
    #[size(4)]
    type Value = NonZeroU32;
}

#[group(SharedGroup)]
impl Word for (Shared,) {
    #[size(4)]
    type Value = Cell<u32>;
}

#[group(GappedGroup)]
impl Word for (Gapped,) {
    #[size(4)]
    type Value = Padded;
}

#[group(QuadGroup)]
impl Word for (Quad,) {
    #[size(4)]
    type Value = [u8; 4];
}

#[typestate(unsafe_transmute = true, align = 4)]
struct Large<S: Word> {
    value: S::Value,
}

#[typestate(unsafe_transmute = true)]
struct Pointer<S: Word> {
    value: NonNull<S::Value>,
}

#[typestate(unsafe_transmute = true)]
struct Ref<'a, S: Word> {
    value: &'a S::Value,
}

#[typestate(unsafe_transmute = true)]
struct RefMut<'a, S: Byte> {
    value: &'a mut S::Value,
}

/// Bytes at an address that is 4-aligned, so one past it is not.
#[repr(align(4))]
struct Aligned([u8; 8]);

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_u8_two_into_bool() {
    let raw = Small::<Raw> { value: 2 };
    let flag: Small<Flag> = unsafe { raw.transmute_state() };
    assert!(flag.value);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_surrogate_into_char() {
    let bits = Large::<Bits> { value: 0xd800 };
    let letter: Large<Letter> = unsafe { bits.transmute_state() };
    assert_eq!(letter.value.len_utf8(), 3);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_zero_into_non_zero() {
    let bits = Large::<Bits> { value: 0 };
    let count: &Large<Count> = unsafe { bits.transmute_state_ref() };
    assert_eq!(count.value.get(), 0);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_reads_padding_as_an_integer() {
    let gapped = Large::<Gapped> {
        value: Padded { small: 1, wide: 2 },
    };
    let bits: Large<Bits> = unsafe { gapped.transmute_state() };
    assert_ne!(bits.value, 0);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_mut_writes_an_invalid_bool_back() {
    let mut flag = Small::<Flag> { value: false };
    unsafe { flag.transmute_state_mut::<Raw>() }.value = 2;
    assert!(flag.value);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_ref_writes_through_a_shared_integer() {
    let bits = Large::<Bits> { value: 1 };
    let shared: &Large<Shared> = unsafe { bits.transmute_state_ref() };
    shared.value.set(2);
    assert_eq!(bits.value, 2);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_non_null_surrogate_into_char() {
    let surrogate = 0xd800u32;
    let bits = Pointer::<Bits> {
        value: NonNull::from(&surrogate),
    };
    let letter: Pointer<Letter> = unsafe { bits.transmute_state() };
    assert_eq!(unsafe { *letter.value.as_ptr() }.len_utf8(), 3);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_shared_pointee_into_cell() {
    let value = 1u32;
    let bits = Ref::<Bits> { value: &value };
    let shared: Ref<Shared> = unsafe { bits.transmute_state() };
    shared.value.set(2);
    assert_eq!(value, 2);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_borrowed_bool_gets_an_invalid_byte_back() {
    let mut value = false;
    let raw: RefMut<Raw> =
        unsafe { RefMut::<Flag> { value: &mut value }.transmute_state() };
    *raw.value = 2;
    assert!(value);
}

#[test]
#[ignore = "undefined behaviour: run alone under Miri"]
fn transmute_state_reads_a_misaligned_pointee() {
    let bytes = Aligned([1; 8]);
    let quad = Pointer::<Quad> {
        value: unsafe {
            NonNull::from(&bytes.0).cast::<[u8; 4]>().byte_add(1)
        },
    };
    let bits: Pointer<Bits> = unsafe { quad.transmute_state() };
    assert_eq!(unsafe { *bits.value.as_ptr() }, 0x0101_0101);
}
