use std::future::Future;

use futures::future::BoxFuture;
use hyper::body::Buf;
use hyper::http::Error as HttpError;
use hyper::server::conn::AddrIncoming;
use hyper::server::{Builder, Server};
use hyper::{Body, Request, Response, StatusCode};
use lazy_static::lazy_static;
use serde::Deserialize;
use serde_json::Value;
use url::Host;
use yajrc::{AnyRpcMethod, GenericRpcMethod, Id, RpcError, RpcRequest, RpcResponse};

use crate::Context;

lazy_static! {
    #[cfg(feature = "cbor")]
    static ref CBOR_INTERNAL_ERROR: Vec<u8> =
        serde_cbor::to_vec(&RpcResponse::<AnyRpcMethod<'static>>::from(yajrc::INTERNAL_ERROR)).unwrap();
    static ref JSON_INTERNAL_ERROR: Vec<u8> =
        serde_json::to_vec(&RpcResponse::<AnyRpcMethod<'static>>::from(yajrc::INTERNAL_ERROR)).unwrap();
}

pub fn make_builder<Ctx: Context>(ctx: &Ctx) -> Builder<AddrIncoming> {
    let addr = match ctx.host() {
        Host::Ipv4(ip) => (ip, ctx.port()).into(),
        Host::Ipv6(ip) => (ip, ctx.port()).into(),
        Host::Domain(localhost) if localhost == "localhost" => ([127, 0, 0, 1], ctx.port()).into(),
        _ => ([0, 0, 0, 0], ctx.port()).into(),
    };
    Server::bind(&addr)
}

pub async fn make_request<Params: for<'de> Deserialize<'de> + 'static>(
    req: &mut Request<Body>,
) -> Result<RpcRequest<GenericRpcMethod<String, Params>>, RpcError> {
    let body = hyper::body::aggregate(std::mem::replace(req.body_mut(), Body::empty()))
        .await?
        .reader();
    let rpc_req: RpcRequest<GenericRpcMethod<String, Params>>;
    #[cfg(feature = "cbor")]
    if req
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        == Some("application/cbor")
    {
        rpc_req = serde_cbor::from_reader(body)?;
    } else {
        rpc_req = serde_json::from_reader(body)?;
    }
    #[cfg(not(feature = "cbor"))]
    {
        rpc_req = serde_json::from_reader(body)?;
    }

    Ok(rpc_req)
}

pub fn to_response<F: Fn(i32) -> StatusCode>(
    req: &Request<Body>,
    res: Result<(Option<Id>, Result<Value, RpcError>), RpcError>,
    status_code_fn: F,
) -> Result<Response<Body>, HttpError> {
    let rpc_res: RpcResponse = match res {
        Ok((id, result)) => RpcResponse { id, result },
        Err(e) => e.into(),
    };
    let body;
    let mut res = Response::builder();
    #[cfg(feature = "cbor")]
    if req
        .headers()
        .get("accept")
        .and_then(|h| h.to_str().ok())
        .iter()
        .flat_map(|s| s.split(","))
        .map(|s| s.trim())
        .any(|s| s == "application/cbor")
    // prefer cbor if accepted
    {
        res = res.header("content-type", "application/cbor");
        body = serde_cbor::to_vec(&rpc_res).unwrap_or_else(|_| CBOR_INTERNAL_ERROR.clone());
    } else {
        res = res.header("content-type", "application/json");
        body = serde_json::to_vec(&rpc_res).unwrap_or_else(|_| JSON_INTERNAL_ERROR.clone());
    }
    #[cfg(not(feature = "cbor"))]
    {
        res.header("content-type", "application/json");
        body = serde_json::to_vec(&rpc_res).unwrap_or_else(|_| JSON_INTERNAL_ERROR.clone());
    }
    res = res.header("content-length", body.len());
    res = res.status(match &rpc_res.result {
        Ok(_) => StatusCode::OK,
        Err(e) => status_code_fn(e.code),
    });
    res.body(Body::from(body))
}

pub type DynMiddleware<'a, 'b, 'c, Params> = Box<
    dyn FnOnce(
            &'a mut Request<Body>,
        ) -> BoxFuture<
            'a,
            Result<Result<DynMiddlewareStage2<'b, 'c, Params>, Response<Body>>, HttpError>,
        > + Send
        + Sync,
>;
pub type DynMiddlewareStage2<'a, 'b, Params> = Box<
    dyn FnOnce(
            &'a mut RpcRequest<GenericRpcMethod<String, Params>>,
        )
            -> BoxFuture<'a, Result<Result<DynMiddlewareStage3<'b>, Response<Body>>, HttpError>>
        + Send
        + Sync,
>;
pub type DynMiddlewareStage3<'a> =
    Box<dyn FnOnce(&'a mut Response<Body>) -> BoxFuture<'a, Result<(), HttpError>> + Send + Sync>;

pub fn constrain_middleware<
    'a,
    'b,
    'c,
    Params: for<'de> Deserialize<'de> + 'static,
    ReqFn: Fn(&'a mut Request<Body>) -> ReqFut,
    ReqFut: Future<Output = Result<Result<RpcReqFn, Response<Body>>, HttpError>> + 'a,
    RpcReqFn: FnOnce(&'b mut RpcRequest<GenericRpcMethod<String, Params>>) -> RpcReqFut,
    RpcReqFut: Future<Output = Result<Result<ResFn, Response<Body>>, HttpError>> + 'b,
    ResFn: FnOnce(&'c mut Response<Body>) -> ResFut,
    ResFut: Future<Output = Result<(), HttpError>> + 'c,
>(
    f: ReqFn,
) -> ReqFn {
    f
}
