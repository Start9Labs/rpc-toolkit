use super::*;
use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

pub fn build(args: RunCliArgs) -> TokenStream {
    let mut command_handler = args.command.clone();
    let mut arguments = std::mem::replace(
        &mut command_handler.segments.last_mut().unwrap().arguments,
        PathArguments::None,
    );
    let command = command_handler.clone();
    if let PathArguments::AngleBracketed(a) = &mut arguments {
        a.args.push(syn::parse2(quote! { () }).unwrap());
    }
    command_handler.segments.push(PathSegment {
        ident: Ident::new("cli_handler", command.span()),
        arguments,
    });
    let app = if let Some(mut_app) = args.mut_app {
        let ident = mut_app.app_ident;
        let body = mut_app.body;
        quote! {
            {
                let #ident = #command::build_app();
                #body
            }
        }
    } else {
        quote! { #command::build_app() }
    };
    let make_ctx = if let Some(make_seed) = args.make_seed {
        let ident = make_seed.matches_ident;
        let body = make_seed.body;
        quote! {
            {
                let #ident = &rpc_toolkit_matches;
                rpc_toolkit::SeedableContext::new(#body)
            }
        }
    } else {
        quote! { rpc_toolkit::SeedableContext::new(&rpc_toolkit_matches) }
    };
    let exit_fn = args
        .exit_fn
        .unwrap_or_else(|| syn::parse2(quote! { |code| code }).unwrap());
    quote! {
        {
            let rpc_toolkit_matches = #app.get_matches();
            let rpc_toolkit_ctx = #make_ctx;
            if let Err(e) = #command_handler(
                rpc_toolkit_ctx,
                None,
                &rpc_toolkit_matches,
                "".into(),
                (),
            ) {
                eprintln!("{}", e.message);
                if let Some(data) = e.data {
                    eprintln!("{:?}", data);
                }
                let exit_fn = #exit_fn;
                drop(rpc_toolkit_matches);
                std::process::exit(exit_fn(e.code))
            } else {
                drop(rpc_toolkit_matches);
                std::process::exit(0)
            }
        }
    }
}
