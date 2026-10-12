//! Project-agnostic extensions on `syn` and `proc_macro2` types.

use core::mem;

use extend::ext;
use proc_macro2::{TokenStream, TokenTree};
use quote::{ToTokens, quote};
use syn::{
    AssocType, Attribute, ConstParam, Expr, ExprPath, GenericArgument,
    GenericParam, Generics, Ident, ImplItem, ImplItemConst, ImplItemFn,
    Item, ItemImpl, LifetimeParam, Macro, Path, PathArguments,
    PathSegment, PredicateType, Signature, Token, Type, TypeParam,
    TypeParamBound, TypePath, WherePredicate,
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::Punctuated,
    visit::{self, Visit},
    visit_mut::{self, VisitMut},
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

    /// Removes every `Assoc = Type` binding on a bound of `param` and
    /// returns them.
    ///
    /// `S: Meta<Value = String> + Clone` -> `S: Meta + Clone`, returning
    /// `[Value = String]`
    fn take_assoc_type_bindings(
        &mut self,
        param: &Ident,
    ) -> Vec<AssocType> {
        let mut bindings = TakeAssocTypes::default();
        for tp in self.type_params_mut().filter(|tp| tp.ident == *param) {
            for bound in &mut tp.bounds {
                bindings.visit_type_param_bound_mut(bound);
            }
        }

        let in_where = self
            .where_clause
            .iter_mut()
            .flat_map(|clause| &mut clause.predicates)
            .filter_map(|predicate| predicate.bounds_on_mut(param))
            .flatten();
        for bound in in_where {
            bindings.visit_type_param_bound_mut(bound);
        }
        bindings.0
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

/// Takes out the `Assoc = Type` bindings written directly on the bounds
/// it visits, and none nested in their arguments.
#[derive(Default)]
struct TakeAssocTypes(Vec<AssocType>);

impl VisitMut for TakeAssocTypes {
    fn visit_path_arguments_mut(&mut self, arguments: &mut PathArguments) {
        let PathArguments::AngleBracketed(angle) = arguments else {
            return;
        };
        for arg in mem::take(&mut angle.args) {
            match arg {
                GenericArgument::AssocType(assoc) => self.0.push(assoc),
                arg => angle.args.push(arg),
            }
        }
        if angle.args.is_empty() {
            *arguments = PathArguments::None;
        }
    }
}

#[ext]
pub(crate) impl ItemImpl {
    /// Qualifies each `Self::item` path to a const or fn this impl
    /// defines with `trait_`.
    ///
    /// `Self::LIMIT` -> `<Self as Trait>::LIMIT`
    fn qualify_self_paths(&mut self, trait_: &Path) {
        let items = self
            .items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Const(ImplItemConst { ident, .. })
                | ImplItem::Fn(ImplItemFn {
                    sig: Signature { ident, .. },
                    ..
                }) => Some(ident.clone()),
                _ => None,
            })
            .collect();
        let mut qualify = QualifySelfPaths { trait_, items };
        for item in &mut self.items {
            qualify.visit_impl_item_mut(item);
        }
    }
}

/// Qualifies the `Self::item` paths to `items` it visits with `trait_`,
/// skipping nested items, where `Self` is another type. Reaches into
/// macros whose arguments are comma-separated expressions, like
/// `format!`.
struct QualifySelfPaths<'a> {
    trait_: &'a Path,
    items: Vec<Ident>,
}

impl VisitMut for QualifySelfPaths<'_> {
    fn visit_expr_path_mut(&mut self, expr: &mut ExprPath) {
        match self.own_item(expr) {
            Some(item) => {
                let trait_ = self.trait_;
                *expr = parse_quote!(<Self as #trait_>::#item);
            }
            None => visit_mut::visit_expr_path_mut(self, expr),
        }
    }

    fn visit_macro_mut(&mut self, mac: &mut Macro) {
        let Ok(mut args) = mac.parse_body_with(
            Punctuated::<Expr, Token![,]>::parse_terminated,
        ) else {
            return;
        };
        for arg in &mut args {
            self.visit_expr_mut(arg);
        }
        mac.tokens = args.to_token_stream();
    }

    /// Skips nested items. The default walks into them, but `Self` in a
    /// nested item names another type, so its `Self::item` paths must
    /// stay as written.
    fn visit_item_mut(&mut self, _: &mut Item) {}
}

impl QualifySelfPaths<'_> {
    /// The `item` segment of `expr` when it's `Self::item` and `item` is
    /// one of `items`.
    fn own_item(&self, expr: &ExprPath) -> Option<PathSegment> {
        let ExprPath {
            qself: None, path, ..
        } = expr
        else {
            return None;
        };
        let [self_ty, item] = path.segments.iter().collect::<Vec<_>>()[..]
        else {
            return None;
        };
        (self_ty.ident == "Self" && self.items.contains(&item.ident))
            .then(|| item.clone())
    }
}

#[ext]
pub(crate) impl WherePredicate {
    /// This predicate's bounds when it bounds `param`.
    ///
    /// `S: Meta + Clone` -> `Meta + Clone`
    fn bounds_on_mut(
        &mut self,
        param: &Ident,
    ) -> Option<&mut Punctuated<TypeParamBound, Token![+]>> {
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
