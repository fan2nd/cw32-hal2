// SPDX-License-Identifier: MIT OR Apache-2.0
//! Project-authored enum formatting for generated Rust metadata.
//!
//! `EnumDebug` prefixes each variant with its enum name. Unit variants and
//! tuple variants with exactly one field are supported.
//!
//! ```compile_fail
//! use cw32_data_macros::EnumDebug;
//! #[derive(EnumDebug)]
//! struct NotAnEnum;
//! ```
//!
//! ```compile_fail
//! use cw32_data_macros::EnumDebug;
//! #[derive(EnumDebug)]
//! enum Named { Value { value: u8 } }
//! ```
//!
//! ```compile_fail
//! use cw32_data_macros::EnumDebug;
//! #[derive(EnumDebug)]
//! enum Many { Value(u8, u8) }
//! ```
//!
//! ```compile_fail
//! use cw32_data_macros::EnumDebug;
//! #[derive(EnumDebug)]
//! enum EmptyTuple { Value() }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

#[proc_macro_derive(EnumDebug)]
pub fn derive_enum_debug(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let Data::Enum(body) = input.data else {
        return Err(syn::Error::new_spanned(
            input.ident,
            "EnumDebug requires an enum",
        ));
    };
    let enum_name = input.ident;
    let mut generics = input.generics;
    for parameter in generics.type_params_mut() {
        parameter.bounds.push(syn::parse_quote!(::core::fmt::Debug));
    }
    let mut branches = Vec::with_capacity(body.variants.len());
    for variant in body.variants {
        let variant_name = variant.ident;
        let label = quote!(concat!(
            stringify!(#enum_name),
            "::",
            stringify!(#variant_name)
        ));
        branches.push(match variant.fields {
            Fields::Unit => quote!(Self::#variant_name => formatter.write_str(#label)),
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                quote!(Self::#variant_name(payload) => formatter.debug_tuple(#label).field(payload).finish())
            }
            unsupported => return Err(syn::Error::new_spanned(
                unsupported,
                "EnumDebug supports unit variants or tuple variants with exactly one field",
            )),
        });
    }
    let (impl_parameters, type_parameters, constraints) = generics.split_for_impl();
    let matching = if branches.is_empty() {
        quote!(match *self {})
    } else {
        quote!(match self { #(#branches),* })
    };
    Ok(quote! {
        impl #impl_parameters ::core::fmt::Debug for #enum_name #type_parameters #constraints {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #matching
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand;

    #[test]
    fn rejects_non_enum_inputs_with_a_specific_diagnostic() {
        for source in ["struct Item;", "union Item { byte: u8 }"] {
            let error = expand(syn::parse_str(source).unwrap()).unwrap_err();
            assert_eq!(error.to_string(), "EnumDebug requires an enum");
        }
    }

    #[test]
    fn rejects_all_unsupported_field_shapes() {
        for source in [
            "enum Item { Value() }",
            "enum Item { Value(u8, u8) }",
            "enum Item { Value { byte: u8 } }",
            "enum Item { Value {} }",
        ] {
            let error = expand(syn::parse_str(source).unwrap()).unwrap_err();
            assert_eq!(
                error.to_string(),
                "EnumDebug supports unit variants or tuple variants with exactly one field"
            );
        }
    }
}
