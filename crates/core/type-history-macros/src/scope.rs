//! Bind each expansion to the library module and type discovered by the hook.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Result};

pub(super) fn check(
    module_path: &[String],
    rust_name: &str,
    record: &Ident,
) -> Result<TokenStream> {
    let declared: Ident = syn::parse_str(rust_name)?;
    let modules = module_path
        .iter()
        .skip(1)
        .map(|name| {
            syn::parse_str::<Ident>(name).or_else(|_| syn::parse_str::<Ident>(&format!("r#{name}")))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(quote! {
        const _: () = {
            // A copied or renamed record must equal the authored module's type.
            // Rustc resolves raw and Unicode names, including normalization.
            #[diagnostic::on_unimplemented(message = "unsupported history declaration: expansion must use its discovered module-level type")]
            trait __TypeHistoryDeclaredRecord {}
            impl __TypeHistoryDeclaredRecord for crate::#(#modules::)*#declared {}
            fn __type_history_require_declared<T: __TypeHistoryDeclaredRecord>() {}
            let _ = __type_history_require_declared::<#record>;
        };
    })
}
