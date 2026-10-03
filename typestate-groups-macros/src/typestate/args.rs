//! The arguments of `#[typestate(..)]`.

use syn::{
    Ident, LitBool, LitInt, Token,
    parse::{Parse, ParseStream},
};

use crate::syn_ext::OptionExt as _;

/// `#[typestate(state = S, unsafe_transmute = true, align = N)]`
pub(crate) struct TypeStateArgs {
    pub(super) state: Option<Ident>,
    pub(super) transmute: Transmute,
}

impl Parse for TypeStateArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut state: Option<Ident> = None;
        let mut unsafe_transmute: Option<LitBool> = None;
        let mut align: Option<LitInt> = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "state" => state.parse_once(&key, input)?,
                "unsafe_transmute" => {
                    unsafe_transmute.parse_once(&key, input)?
                }
                "align" => align.parse_once(&key, input)?,
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `state = <Ident>`, `unsafe_transmute = \
                         <bool>` or `align = <integer literal>`",
                    ));
                }
            }

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        let align = align.map_or(Alignment::Inferred, Alignment::Forced);
        let transmute =
            if unsafe_transmute.is_some_and(|flag| flag.value()) {
                Transmute::On(align)
            } else if let Alignment::Forced(n) = align {
                return Err(syn::Error::new(
                    n.span(),
                    "add `unsafe_transmute = true` to use `align`",
                ));
            } else {
                Transmute::Off
            };

        Ok(TypeStateArgs { state, transmute })
    }
}

/// Whether `#[typestate]` implements `TransmutableState`.
pub(super) enum Transmute {
    Off,
    On(Alignment),
}

/// The container's alignment.
pub(super) enum Alignment {
    Inferred,
    /// `align = N`
    Forced(LitInt),
}
