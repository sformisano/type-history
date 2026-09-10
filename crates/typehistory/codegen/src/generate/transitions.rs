//! Adjacent conversions with borrowed reads before consuming assignments.

use crate::{
    history::{BirthAssignment, FieldTransition, UpdateAssignment},
    lint_attributes::lint_attributes,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, Path, Type};

use super::Context;

pub(super) fn generate(context: &Context<'_>) -> TokenStream {
    let support = &context.paths.support;
    let error_type = &context.paths.error;
    let functions = context.history.transitions().iter().map(|transition| {
        let source = context.payload(transition.from);
        let destination = context.payload(transition.to);
        let function = context.helper(&format!("upcast_v{}_v{}", transition.from, transition.to));
        let previous = format_ident!("__typehistory_previous", span = Span::mixed_site());
        let source_version = format_ident!("__typehistory_source_version", span = Span::mixed_site());
        let from = context.version(transition.from);
        let to = context.version(transition.to);
        let identity = context.identity();
        let mut borrowed = Vec::new();
        let mut assigned = Vec::new();
        let mut carried = Vec::new();
        let mut assembled = Vec::new();
        for (index, field) in transition.fields.iter().enumerate() {
            let local = format_ident!("__typehistory_field_{index}", span = Span::mixed_site());
            let name = field.name();
            let ty = context.field_type(transition.to, name);
            let (_, retained) = context.field(transition.to, name);
            let lints = lint_attributes(&retained.attributes).collect::<Vec<_>>();
            let with_field_lints = |statement: TokenStream| quote!(#(#lints)* #statement);
            assembled.push(quote!(#name: #local));
            let on_error = {
                let error = format_ident!("__typehistory_error", span = Span::mixed_site());
                quote!(|#error| #error_type::upcast(#identity, #source_version, #from, #to, &#error))
            };
            match field {
                FieldTransition::Carry { name } => carried.push(with_field_lints(quote!(let #local = #previous.#name;))),
                FieldTransition::Birth { assignment, .. } => match assignment {
                    BirthAssignment::BackfillValue(expression) => assigned.push(with_field_lints(quote! {
                        let #local: #ty = (|| -> #ty { #expression })();
                    })),
                    BirthAssignment::CastFromPreviousPayload(path) => borrowed.push(with_field_lints(payload_callback(
                        path, &ty, &source, &previous, &local, &on_error,
                    ))),
                },
                FieldTransition::Convert { name, assignment, .. } => match assignment {
                    UpdateAssignment::CastFromPreviousPayload(path) => borrowed.push(with_field_lints(payload_callback(
                        path, &ty, &source, &previous, &local, &on_error,
                    ))),
                    UpdateAssignment::CastFromPreviousValue(path) => {
                        let callback = format_ident!("__typehistory_callback", span = Span::mixed_site());
                        // A typed function pointer checks both predecessor and destination.
                        let predecessor = context.field_type(transition.from, name);
                        assigned.push(with_field_lints(quote! {
                            let #local: #ty = {
                                let #callback: fn(#predecessor) -> ::core::result::Result<#ty, _> = #path;
                                #callback(#previous.#name).map_err(#on_error)?
                            };
                        }));
                    }
                },
            }
        }
        quote! {
            fn #function(#previous: #source, #source_version: #support::PayloadVersion)
                -> ::core::result::Result<#destination, #error_type>
            {
                #(#borrowed)*
                #(#assigned)*
                #(#carried)*
                ::core::result::Result::Ok(#destination { #(#assembled),* })
            }
        }
    });
    quote!(#(#functions)*)
}

fn payload_callback(
    path: &Path,
    destination: &Type,
    source: &Ident,
    previous: &Ident,
    local: &Ident,
    on_error: &TokenStream,
) -> TokenStream {
    let callback = format_ident!("__typehistory_callback", span = Span::mixed_site());
    quote! {
        let #local: #destination = {
            let #callback: for<'__typehistory_borrow> fn(&'__typehistory_borrow #source) -> ::core::result::Result<#destination, _> = #path;
            #callback(&#previous).map_err(#on_error)?
        };
    }
}
