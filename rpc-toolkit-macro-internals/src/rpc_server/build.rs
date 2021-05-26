use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

use super::*;

pub fn build(args: RpcServerArgs) -> TokenStream {
    let mut command = args.command;
    let arguments = std::mem::replace(
        &mut command.segments.last_mut().unwrap().arguments,
        PathArguments::None,
    );
    command.segments.push(PathSegment {
        ident: Ident::new("rpc_handler", command.span()),
        arguments,
    });
    let ctx = args.ctx;
    let status_fn = args.status_fn.unwrap_or_else(|| {
        syn::parse2(quote! { |_| ::rpc_toolkit::hyper::StatusCode::OK }).unwrap()
    });
    quote! {
        {
            let ctx = #ctx;
            let status_fn = #status_fn;
            let builder = ::rpc_toolkit::rpc_server_helpers::make_builder(&ctx);
            let make_svc = ::rpc_toolkit::hyper::service::make_service_fn(move |_| {
                let ctx = ctx.clone();
                async move {
                    Ok::<_, ::rpc_toolkit::hyper::Error>(::rpc_toolkit::hyper::service::service_fn(move |mut req| {
                        let ctx = ctx.clone();
                        async move {
                            let rpc_req = ::rpc_toolkit::rpc_server_helpers::make_request(&mut req).await;
                            ::rpc_toolkit::rpc_server_helpers::to_response(
                                &req,
                                match rpc_req {
                                    Ok(rpc_req) => Ok((
                                        rpc_req.id,
                                        #command(
                                            ctx,
                                            ::rpc_toolkit::yajrc::RpcMethod::as_str(&rpc_req.method),
                                            rpc_req.params,
                                        )
                                        .await,
                                    )),
                                    Err(e) => Err(e),
                                },
                                status_fn,
                            )
                        }
                    }))
                }
            });
            builder.serve(make_svc)
        }
    }
}
