use proc_macro2::{Span, TokenStream};
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
    let middleware_name_pre = (0..)
        .map(|i| Ident::new(&format!("middleware_pre_{}", i), Span::call_site()))
        .take(args.middleware.len());
    let middleware_name_pre2 = middleware_name_pre.clone();
    let middleware_name = (0..)
        .map(|i| Ident::new(&format!("middleware_{}", i), Span::call_site()))
        .take(args.middleware.len());
    let middleware_name_inv = middleware_name
        .clone()
        .collect::<Vec<_>>()
        .into_iter()
        .rev();
    let middleware = args.middleware.iter();
    let res = quote! {
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
                            #(
                                let #middleware_name_pre = match ::rpc_toolkit::rpc_server_helpers::constrain_middleware(#middleware)(&mut req).await? {
                                    Ok(a) => a,
                                    Err(res) => return Ok(res),
                                };
                            )*
                            let rpc_req = ::rpc_toolkit::rpc_server_helpers::make_request(&mut req).await;
                            match rpc_req {
                                Ok(mut rpc_req) => {
                                    #(
                                        let #middleware_name = match #middleware_name_pre2(&mut rpc_req).await? {
                                            Ok(a) => a,
                                            Err(res) => return Ok(res),
                                        };
                                    )*
                                    let mut rpc_res = ::rpc_toolkit::rpc_server_helpers::to_response(
                                        &req,
                                        Ok((
                                            rpc_req.id,
                                            #command(
                                                ctx,
                                                ::rpc_toolkit::yajrc::RpcMethod::as_str(&rpc_req.method),
                                                rpc_req.params,
                                            )
                                            .await,
                                        )),
                                        status_fn,
                                    )?;
                                    #(
                                        #middleware_name_inv(&mut rpc_res).await?;
                                    )*
                                    Ok::<_, ::rpc_toolkit::hyper::http::Error>(rpc_res)
                                }
                                Err(e) => ::rpc_toolkit::rpc_server_helpers::to_response(
                                    &req,
                                    Err(e),
                                    status_fn,
                                ),
                            }
                        }
                    }))
                }
            });
            builder.serve(make_svc)
        }
    };
    // panic!("{}", res);
    res
}
