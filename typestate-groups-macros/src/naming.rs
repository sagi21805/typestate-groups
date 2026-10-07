use quote::format_ident;
use syn::Ident;

/// `A` -> `AGroupMarker`
pub fn group_marker_ident(trait_ident: &Ident) -> Ident {
    format_ident!("{}GroupMarker", trait_ident)
}

/// `A` -> `AGroupMember`
pub fn group_member_ident(trait_ident: &Ident) -> Ident {
    format_ident!("{}GroupMember", trait_ident)
}

/// `Value` -> `__TypestateGroupsLayoutValue`
pub fn layout_assoc_ident(assoc: &Ident) -> Ident {
    format_ident!("__TypestateGroupsLayout{}", assoc)
}

/// The group parameter of `{Trait}GroupMember`.
pub fn group_param_ident() -> Ident {
    format_ident!("__TypestateGroupsGroup")
}

/// `FreeList` -> `__TypestateGroupsSetsFreeList`, whose inherent consts
/// name the associated types the group sets.
pub fn group_sets_ident(group: &Ident) -> Ident {
    format_ident!("__TypestateGroupsSets{}", group)
}

/// The fallback trait `#[group_impl]` checks a binding's name against.
pub fn unset_trait_ident() -> Ident {
    format_ident!("__TypestateGroupsUnset")
}

/// The type a `#[group_trait]` blanket impl implements the trait for.
pub fn implementor_ident() -> Ident {
    format_ident!("__TypestateGroupsImplementor")
}

/// `{Trait}GroupMember`'s alias in a `#[group_trait]` helper module.
pub fn helper_member_ident() -> Ident {
    format_ident!("Member")
}

/// `A` -> `AHelper`
pub fn helper_trait_ident(trait_ident: &Ident) -> Ident {
    format_ident!("{}Helper", trait_ident)
}

/// `A` -> `__a_helper_mod`
pub fn helper_mod_ident(trait_ident: &Ident) -> Ident {
    format_ident!(
        "__{}_helper_mod",
        trait_ident.to_string().to_lowercase()
    )
}

/// The state parameter `Restate` and `TransmutableState` target.
pub fn target_state_ident() -> Ident {
    format_ident!("__TypestateGroupsTargetState")
}
