use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::spanned::Spanned;

use super::*;

pub fn build(args: RpcServerArgs) -> TokenStream {
    let mut command = args.command;
    let mut arguments = std::mem::replace(
        &mut command.segments.last_mut().unwrap().arguments,
        PathArguments::None,
    );
    let command_module = command.clone();
    if let PathArguments::AngleBracketed(a) = &mut arguments {
        a.args.push(syn::parse2(quote! { _ }).unwrap());
    }
    command.segments.push(PathSegment {
        ident: Ident::new("rpc_handler", command.span()),
        arguments,
    });
    let ctx = args.ctx;
    let parent_data = if let Some(data) = args.parent_data {
        quote! { #data }
    } else {
        quote! { () }
    };
    let status_fn = args.status_fn.unwrap_or_else(|| {
        syn::parse2(quote! { |_| ::rpc_toolkit::hyper::StatusCode::OK }).unwrap()
    });
    let middleware_name_clone = (0..)
        .map(|i| Ident::new(&format!("middleware_clone_{}", i), Span::call_site()))
        .take(args.middleware.len());
    let middleware_name_clone2 = middleware_name_clone.clone();
    let middleware_name_clone3 = middleware_name_clone.clone();
    let middleware_name_clone4 = middleware_name_clone.clone();
    let middleware_name_clone5 = middleware_name_clone.clone();
    let middleware_name_clone6 = middleware_name_clone.clone();
    let middleware_name_pre = (0..)
        .map(|i| Ident::new(&format!("middleware_pre_{}", i), Span::call_site()))
        .take(args.middleware.len());
    let middleware_name_pre2 = middleware_name_pre.clone();
    let middleware_name_post = (0..)
        .map(|i| Ident::new(&format!("middleware_post_{}", i), Span::call_site()))
        .take(args.middleware.len());
    let middleware_name_post_inv = middleware_name_post
        .clone()
        .collect::<Vec<_>>()
        .into_iter()
        .rev();
    let middleware_name = (0..)
        .map(|i| Ident::new(&format!("middleware_{}", i), Span::call_site()))
        .take(args.middleware.len());
    let middleware_name2 = middleware_name.clone();
    let middleware = args.middleware.iter();
    let res = quote! {
        {
            let ctx = #ctx;
            let parent_data = #parent_data;
            let status_fn = #status_fn;
            let builder = ::rpc_toolkit::rpc_server_helpers::make_builder(&ctx);
            #(
                let #middleware_name_clone = ::std::sync::Arc::new(#middleware);
            )*
            let make_svc = ::rpc_toolkit::hyper::service::make_service_fn(move |_| {
                let ctx = ctx.clone();
                let parent_data = parent_data.clone();
                #(
                    let #middleware_name_clone3 = #middleware_name_clone2.clone();
                )*
                async move {
                    Ok::<_, ::rpc_toolkit::hyper::Error>(::rpc_toolkit::hyper::service::service_fn(move |mut req| {
                        let ctx = ctx.clone();
                        let parent_data = parent_data.clone();
                        let metadata = #command_module::Metadata::default();
                        #(
                            let #middleware_name_clone5 = #middleware_name_clone4.clone();
                        )*
                        async move {
                            #(
                                let #middleware_name_pre = match ::rpc_toolkit::rpc_server_helpers::constrain_middleware(&*#middleware_name_clone6)(&mut req, metadata).await? {
                                    Ok(a) => a,
                                    Err(res) => return Ok(res),
                                };
                            )*
                            let (mut req_parts, req_body) = req.into_parts();
                            let (mut res_parts, _) = ::rpc_toolkit::hyper::Response::new(()).into_parts();
                            let rpc_req = ::rpc_toolkit::rpc_server_helpers::make_request(&req_parts, req_body).await;
                            match rpc_req {
                                Ok(mut rpc_req) => {
                                    #(
                                        let #middleware_name_post = match #middleware_name_pre2(&mut req_parts, &mut rpc_req).await? {
                                            Ok(a) => a,
                                            Err(res) => return Ok(res),
                                        };
                                    )*
                                    let mut rpc_res = match ::rpc_toolkit::serde_json::from_value(::rpc_toolkit::serde_json::Value::Object(rpc_req.params)) {
                                        Ok(params) => #command(ctx, parent_data, &req_parts, &mut res_parts, ::rpc_toolkit::yajrc::RpcMethod::as_str(&rpc_req.method), params).await,
                                        Err(e) => Err(e.into())
                                    };
                                    #(
                                        let #middleware_name = match #middleware_name_post_inv(&mut res_parts, &mut rpc_res).await? {
                                            Ok(a) => a,
                                            Err(res) => return Ok(res),
                                        };
                                    )*
                                    let mut res = ::rpc_toolkit::rpc_server_helpers::to_response(
                                        &req_parts.headers,
                                        res_parts,
                                        Ok((
                                            rpc_req.id,
                                            rpc_res,
                                        )),
                                        status_fn,
                                    )?;
                                    #(
                                        #middleware_name2(&mut res).await?;
                                    )*
                                    Ok::<_, ::rpc_toolkit::hyper::http::Error>(res)
                                }
                                Err(e) => ::rpc_toolkit::rpc_server_helpers::to_response(
                                    &req_parts.headers,
                                    res_parts,
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
