use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

use super::*;

pub fn build(args: RunCliArgs) -> TokenStream {
    let mut command_handler = args.command.clone();
    let mut arguments = std::mem::replace(
        &mut command_handler.segments.last_mut().unwrap().arguments,
        PathArguments::None,
    );
    let command = command_handler.clone();
    if let PathArguments::AngleBracketed(a) = &mut arguments {
        a.args.push(syn::parse2(quote! { () }).unwrap());
        a.args.push(syn::parse2(quote! { _ }).unwrap());
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
    let make_ctx = if let Some(make_ctx) = args.make_ctx {
        let ident = make_ctx.matches_ident;
        let body = make_ctx.body;
        quote! {
            {
                let #ident = &rpc_toolkit_matches;
                #body
            }
        }
    } else {
        quote! { &rpc_toolkit_matches }
    };
    let parent_data = if let Some(data) = args.parent_data {
        quote! { #data }
    } else {
        quote! { () }
    };
    let exit_fn = args.exit_fn.unwrap_or_else(|| {
        syn::parse2(quote! { |err: ::rpc_toolkit::yajrc::RpcError| {
            eprintln!("{}", err.message);
            if let Some(data) = err.data {
                eprintln!("{}", data);
            }
            std::process::exit(err.code);
        } })
        .unwrap()
    });
    quote! {
        {
            let rpc_toolkit_matches = #app.get_matches();
            let rpc_toolkit_ctx = #make_ctx;
            let rpc_toolkit_parent_data = #parent_data;
            if let Err(err) = #command_handler(
                rpc_toolkit_ctx,
                rpc_toolkit_parent_data,
                None,
                &rpc_toolkit_matches,
                "".into(),
                (),
            ) {
                drop(rpc_toolkit_matches);
                (#exit_fn)(err);
            } else {
                drop(rpc_toolkit_matches);
            }
        }
    }
}
