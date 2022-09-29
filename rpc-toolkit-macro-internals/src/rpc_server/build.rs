use proc_macro2::TokenStream;
use quote::quote;

use super::*;

pub fn build(mut args: RpcServerArgs) -> TokenStream {
    let ctx = std::mem::replace(
        &mut args.ctx,
        parse2(quote! { __rpc_toolkit__rpc_server__context }).unwrap(),
    );
    let handler = crate::rpc_handler::build::build(args);
    let res = quote! {
        {
            let __rpc_toolkit__rpc_server__context = #ctx;
            let __rpc_toolkit__rpc_server__builder = ::rpc_toolkit::rpc_server_helpers::make_builder(&__rpc_toolkit__rpc_server__context);
            let handler = #handler;
            __rpc_toolkit__rpc_server__builder.serve(::rpc_toolkit::hyper::service::make_service_fn(move |_| {
                let handler = handler.clone();
                async move { Ok::<_, ::std::convert::Infallible>(::rpc_toolkit::hyper::service::service_fn(move |req| handler(req))) }
            }))
        }
    };
    // panic!("{}", res);
    res
}
