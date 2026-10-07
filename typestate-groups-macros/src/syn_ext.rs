//! Project-agnostic extensions on `syn` and `proc_macro2` types.

use extend::ext;
use proc_macro2::{TokenStream, TokenTree};
use quote::{ToTokens, quote};
use syn::{
    AssocType, Attribute, ConstParam, GenericArgument, GenericParam,
    Generics, Ident, LifetimeParam, Path, PathArguments, PredicateType,
    Token, Type, TypeParam, TypeParamBound, TypePath, WherePredicate,
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::Punctuated,
    visit::{self, Visit},
    visit_mut::VisitMut,
};

#[ext]
pub(crate) impl Type {
    /// This type without parentheses and invisible groups.
    fn peeled(&self) -> &Type {
        match self {
            Type::Paren(paren) => paren.elem.peeled(),
            Type::Group(group) => group.elem.peeled(),
            ty => ty,
        }
    }

    /// A value of this type when it's `()` or `PhantomData<..>`.
    fn zst_value(&self) -> Option<TokenStream> {
        let path = match self.peeled() {
            Type::Tuple(tuple) => {
                return tuple.elems.is_empty().then(|| quote!(()));
            }
            Type::Path(TypePath {
                qself: None, path, ..
            }) => path,
            _ => return None,
        };

        if path.is_std_item("marker", "PhantomData") {
            Some(quote!(::core::marker::PhantomData))
        } else {
            None
        }
    }

    /// Whether this type's spelling shows it to be a ZST.
    fn is_zst(&self) -> bool {
        self.zst_value().is_some()
    }

    /// Whether `ident` appears anywhere inside this type, macro tokens
    /// included.
    fn mentions_ident(&self, ident: &Ident) -> bool {
        let mut finder = FindIdent {
            ident,
            found: false,
        };
        finder.visit_type(self);
        finder.found
    }

    /// This type with every `from` renamed to `to`.
    ///
    /// `Node<S>` -> `Node<T>`
    fn renamed(&self, from: &Ident, to: &Ident) -> Type {
        let mut ty = self.clone();
        Rename {
            from,
            to,
            found: false,
        }
        .visit_type_mut(&mut ty);
        ty
    }
}

#[ext]
pub(crate) impl Path {
    /// Whether this path names std's `module::item`, with any prefix.
    fn is_std_item(&self, module: &str, item: &str) -> bool {
        let idents: Vec<&Ident> =
            self.segments.iter().map(|s| &s.ident).collect();

        let Some((last, prefix)) = idents.split_last() else {
            return false;
        };
        *last == item
            && match prefix {
                [] => true,
                [m] => *m == module,
                [root, m] => {
                    (*root == "std" || *root == "core" || *root == "alloc")
                        && *m == module
                }
                _ => false,
            }
    }

    /// `a::Meta` -> `a::MetaGroupMarker`
    fn with_last_ident(
        &self,
        rename: impl FnOnce(&Ident) -> Ident,
    ) -> Path {
        let mut path = self.clone();
        if let Some(last) = path.segments.last_mut() {
            last.ident = rename(&last.ident);
        }
        path
    }
}

#[ext]
pub(crate) impl Generics {
    /// The type parameter named `ident`, if these generics declare one.
    fn type_param_mut(&mut self, ident: &Ident) -> Option<&mut TypeParam> {
        self.type_params_mut().find(|tp| tp.ident == *ident)
    }

    /// The arguments that name these generics.
    ///
    /// `<'a, T: Bound, const N: usize>` -> `<'a, T, N>`
    fn to_arguments(&self) -> PathArguments {
        if self.params.is_empty() {
            return PathArguments::None;
        }
        let args = self.params.iter().map(|param| -> GenericArgument {
            match param {
                GenericParam::Lifetime(LifetimeParam {
                    lifetime, ..
                }) => GenericArgument::Lifetime(lifetime.clone()),
                GenericParam::Type(TypeParam { ident, .. })
                | GenericParam::Const(ConstParam { ident, .. }) => {
                    parse_quote!(#ident)
                }
            }
        });
        PathArguments::AngleBracketed(parse_quote!(<#(#args),*>))
    }

    /// `S: Meta<Value = String>` -> `Value = String`
    fn assoc_type_binding(&self, param: &Ident) -> Option<&AssocType> {
        let inline = self
            .type_params()
            .filter(|tp| tp.ident == *param)
            .flat_map(|tp| &tp.bounds);
        let in_where = self
            .where_clause
            .iter()
            .flat_map(|clause| &clause.predicates)
            .filter_map(|predicate| predicate.bounds_on(param))
            .flatten();

        inline.chain(in_where).find_map(|bound| {
            let mut first = FirstAssocType::default();
            first.visit_type_param_bound(bound);
            first.0
        })
    }
}

#[ext]
pub(crate) impl PathArguments {
    /// Inserts `arg` after the lifetimes.
    ///
    /// `<'a, T>`, `G` -> `<'a, G, T>`
    fn insert_after_lifetimes(&mut self, arg: GenericArgument) {
        if self.is_none() {
            *self = PathArguments::AngleBracketed(parse_quote!(<>));
        }
        if let PathArguments::AngleBracketed(angle) = self {
            let lifetimes = angle
                .args
                .iter()
                .take_while(|arg| {
                    matches!(arg, GenericArgument::Lifetime(_))
                })
                .count();
            angle.args.insert(lifetimes, arg);
        }
    }
}

/// Keeps the first `Assoc = Type` binding it visits.
#[derive(Default)]
struct FirstAssocType<'ast>(Option<&'ast AssocType>);

impl<'ast> Visit<'ast> for FirstAssocType<'ast> {
    fn visit_generic_argument(&mut self, arg: &'ast GenericArgument) {
        if self.0.is_some() {
            return;
        }
        match arg {
            GenericArgument::AssocType(assoc) => self.0 = Some(assoc),
            _ => visit::visit_generic_argument(self, arg),
        }
    }
}

#[ext]
pub(crate) impl WherePredicate {
    /// This predicate's bounds when it bounds `param`.
    ///
    /// `S: Meta + Clone` -> `Meta + Clone`
    fn bounds_on(
        &self,
        param: &Ident,
    ) -> Option<&Punctuated<TypeParamBound, Token![+]>> {
        match self {
            WherePredicate::Type(PredicateType {
                bounded_ty:
                    Type::Path(TypePath {
                        qself: None, path, ..
                    }),
                bounds,
                ..
            }) if path.is_ident(param) => Some(bounds),
            _ => None,
        }
    }

    /// This predicate with `from` renamed to `to`, when it mentions
    /// `from`.
    ///
    /// `Option<S>: Debug` -> `Option<T>: Debug`
    fn renamed(&self, from: &Ident, to: &Ident) -> Option<WherePredicate> {
        let mut predicate = self.clone();
        let mut rename = Rename {
            from,
            to,
            found: false,
        };
        rename.visit_where_predicate_mut(&mut predicate);
        rename.found.then_some(predicate)
    }
}

/// Renames every `from` it visits to `to`, and records whether it found
/// one.
struct Rename<'a> {
    from: &'a Ident,
    to: &'a Ident,
    found: bool,
}

impl VisitMut for Rename<'_> {
    fn visit_ident_mut(&mut self, ident: &mut Ident) {
        if ident == self.from {
            *ident = self.to.clone();
            self.found = true;
        }
    }
}

#[ext]
pub(crate) impl Attribute {
    /// Whether this attribute is `#[repr(C | transparent)]`.
    fn guarantees_layout(&self) -> bool {
        self.meta.require_list().is_ok_and(|list| {
            list.tokens
                .clone()
                .into_iter()
                .any(|tt| matches!(&tt, TokenTree::Ident(i) if i == "C" || i == "transparent"))
        })
    }
}

#[ext(name = OptionExt)]
pub(crate) impl<T: Parse> Option<T> {
    /// Parses `= <value>` into this slot, rejecting duplicates.
    fn parse_once(
        &mut self,
        key: &Ident,
        input: ParseStream,
    ) -> syn::Result<()> {
        if self.is_some() {
            return Err(syn::Error::new(
                key.span(),
                format!("remove the duplicate `{key}` argument"),
            ));
        }
        input.parse::<Token![=]>()?;
        *self = Some(input.parse()?);
        Ok(())
    }
}

/// Finds `ident` inside one type, and stops visiting once it has.
///
/// syn leaves a macro's tokens unparsed, so a `Type::Macro` falls back to
/// scanning its tokens.
struct FindIdent<'a> {
    ident: &'a Ident,
    found: bool,
}

impl<'ast> Visit<'ast> for FindIdent<'_> {
    fn visit_type(&mut self, ty: &'ast Type) {
        if self.found {
            return;
        }
        match ty {
            Type::Macro(mac) => {
                self.found =
                    mac.to_token_stream().mentions_ident(self.ident);
            }
            _ => visit::visit_type(self, ty),
        }
    }

    fn visit_ident(&mut self, ident: &'ast Ident) {
        self.found |= ident == self.ident;
    }
}

/// Token scan behind `FindIdent`'s macro fallback.
#[ext]
impl TokenStream {
    /// Whether `ident` appears anywhere in these tokens.
    fn mentions_ident(self, ident: &Ident) -> bool {
        self.into_iter().any(|tt| match tt {
            TokenTree::Ident(i) => i == *ident,
            TokenTree::Group(g) => g.stream().mentions_ident(ident),
            _ => false,
        })
    }
}
