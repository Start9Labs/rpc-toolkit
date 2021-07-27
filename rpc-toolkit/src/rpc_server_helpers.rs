use std::future::Future;

use futures::future::BoxFuture;
use futures::FutureExt;
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

use crate::{Context, Metadata};

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

// &mut Request<Body> -> Result<Result<Future<&mut RpcRequest<...> -> Future<Result<Result<&mut Response<Body> -> Future<Result<(), HttpError>>, Response<Body>>, HttpError>>>, Response<Body>>, HttpError>
pub type DynMiddleware<Params, Metadata> = Box<
    dyn for<'a> FnOnce(
            &'a mut Request<Body>,
            Metadata,
        ) -> BoxFuture<
            'a,
            Result<Result<DynMiddlewareStage2<Params>, Response<Body>>, HttpError>,
        > + Send
        + Sync,
>;
pub fn noop<Params: for<'de> Deserialize<'de> + 'static, M: Metadata>() -> DynMiddleware<Params, M>
{
    Box::new(|_, _| async { Ok(Ok(noop2())) }.boxed())
}
pub type DynMiddlewareStage2<Params> = Box<
    dyn for<'a> FnOnce(
            &'a mut RpcRequest<GenericRpcMethod<String, Params>>,
        ) -> BoxFuture<
            'a,
            Result<Result<DynMiddlewareStage3, Response<Body>>, HttpError>,
        > + Send
        + Sync,
>;
pub fn noop2<Params: for<'de> Deserialize<'de> + 'static>() -> DynMiddlewareStage2<Params> {
    Box::new(|_| async { Ok(Ok(noop3())) }.boxed())
}
pub type DynMiddlewareStage3 = Box<
    dyn for<'a> FnOnce(
            &'a mut Result<Value, RpcError>,
        ) -> BoxFuture<
            'a,
            Result<Result<DynMiddlewareStage4, Response<Body>>, HttpError>,
        > + Send
        + Sync,
>;
pub fn noop3() -> DynMiddlewareStage3 {
    Box::new(|_| async { Ok(Ok(noop4())) }.boxed())
}
pub type DynMiddlewareStage4 = Box<
    dyn for<'a> FnOnce(&'a mut Response<Body>) -> BoxFuture<'a, Result<(), HttpError>>
        + Send
        + Sync,
>;
pub fn noop4() -> DynMiddlewareStage4 {
    Box::new(|_| async { Ok(()) }.boxed())
}

pub fn constrain_middleware<
    'a,
    'b,
    'c,
    'd,
    Params: for<'de> Deserialize<'de> + 'static,
    M: Metadata,
    ReqFn: Fn(&'a mut Request<Body>, M) -> ReqFut,
    ReqFut: Future<Output = Result<Result<RpcReqFn, Response<Body>>, HttpError>> + 'a,
    RpcReqFn: FnOnce(&'b mut RpcRequest<GenericRpcMethod<String, Params>>) -> RpcReqFut,
    RpcReqFut: Future<Output = Result<Result<RpcResFn, Response<Body>>, HttpError>> + 'b,
    RpcResFn: FnOnce(&'c mut Result<Value, RpcError>) -> RpcResFut,
    RpcResFut: Future<Output = Result<Result<ResFn, Response<Body>>, HttpError>> + 'c,
    ResFn: FnOnce(&'d mut Response<Body>) -> ResFut,
    ResFut: Future<Output = Result<(), HttpError>> + 'd,
>(
    f: ReqFn,
) -> ReqFn {
    f
}
