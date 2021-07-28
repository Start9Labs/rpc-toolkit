use std::future::Future;

use futures::future::BoxFuture;
use futures::FutureExt;
use hyper::body::Buf;
use hyper::header::HeaderValue;
use hyper::http::request::Parts as RequestParts;
use hyper::http::response::Parts as ResponseParts;
use hyper::http::Error as HttpError;
use hyper::server::conn::AddrIncoming;
use hyper::server::{Builder, Server};
use hyper::{Body, HeaderMap, Request, Response, StatusCode};
use lazy_static::lazy_static;
use serde_json::{Map, Value};
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

pub async fn make_request(
    req_parts: &RequestParts,
    req_body: Body,
) -> Result<RpcRequest<GenericRpcMethod<String, Map<String, Value>>>, RpcError> {
    let body = hyper::body::aggregate(req_body).await?.reader();
    let rpc_req: RpcRequest<GenericRpcMethod<String, Map<String, Value>>>;
    #[cfg(feature = "cbor")]
    if req_parts
        .headers
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
    req_headers: &HeaderMap<HeaderValue>,
    mut res_parts: ResponseParts,
    res: Result<(Option<Id>, Result<Value, RpcError>), RpcError>,
    status_code_fn: F,
) -> Result<Response<Body>, HttpError> {
    let rpc_res: RpcResponse = match res {
        Ok((id, result)) => RpcResponse { id, result },
        Err(e) => e.into(),
    };
    let body;
    #[cfg(feature = "cbor")]
    if req_headers
        .get("accept")
        .and_then(|h| h.to_str().ok())
        .iter()
        .flat_map(|s| s.split(","))
        .map(|s| s.trim())
        .any(|s| s == "application/cbor")
    // prefer cbor if accepted
    {
        res_parts
            .headers
            .insert("content-type", HeaderValue::from_static("application/cbor"));
        body = serde_cbor::to_vec(&rpc_res).unwrap_or_else(|_| CBOR_INTERNAL_ERROR.clone());
    } else {
        res_parts
            .headers
            .insert("content-type", HeaderValue::from_static("application/json"));
        body = serde_json::to_vec(&rpc_res).unwrap_or_else(|_| JSON_INTERNAL_ERROR.clone());
    }
    #[cfg(not(feature = "cbor"))]
    {
        res_parts
            .headers
            .insert("content-type", HeaderValue::from_static("application/json"));
        body = serde_json::to_vec(&rpc_res).unwrap_or_else(|_| JSON_INTERNAL_ERROR.clone());
    }
    res_parts.headers.insert(
        "content-length",
        HeaderValue::from_str(&format!("{}", body.len()))?,
    );
    res_parts.status = match &rpc_res.result {
        Ok(_) => StatusCode::OK,
        Err(e) => status_code_fn(e.code),
    };
    Ok(Response::from_parts(res_parts, body.into()))
}

// &mut Request<Body> -> Result<Result<Future<&mut RpcRequest<...> -> Future<Result<Result<&mut Response<Body> -> Future<Result<(), HttpError>>, Response<Body>>, HttpError>>>, Response<Body>>, HttpError>
pub type DynMiddleware<Metadata> = Box<
    dyn for<'a> FnOnce(
            &'a mut Request<Body>,
            Metadata,
        ) -> BoxFuture<
            'a,
            Result<Result<DynMiddlewareStage2, Response<Body>>, HttpError>,
        > + Send
        + Sync,
>;
pub fn noop<M: Metadata>() -> DynMiddleware<M> {
    Box::new(|_, _| async { Ok(Ok(noop2())) }.boxed())
}
pub type DynMiddlewareStage2 = Box<
    dyn for<'a> FnOnce(
            &'a mut RequestParts,
            &'a mut RpcRequest<GenericRpcMethod<String, Map<String, Value>>>,
        ) -> BoxFuture<
            'a,
            Result<Result<DynMiddlewareStage3, Response<Body>>, HttpError>,
        > + Send
        + Sync,
>;
pub fn noop2() -> DynMiddlewareStage2 {
    Box::new(|_, _| async { Ok(Ok(noop3())) }.boxed())
}
pub type DynMiddlewareStage3 = Box<
    dyn for<'a> FnOnce(
            &'a mut ResponseParts,
            &'a mut Result<Value, RpcError>,
        ) -> BoxFuture<
            'a,
            Result<Result<DynMiddlewareStage4, Response<Body>>, HttpError>,
        > + Send
        + Sync,
>;
pub fn noop3() -> DynMiddlewareStage3 {
    Box::new(|_, _| async { Ok(Ok(noop4())) }.boxed())
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
    M: Metadata,
    ReqFn: Fn(&'a mut Request<Body>, M) -> ReqFut,
    ReqFut: Future<Output = Result<Result<RpcReqFn, Response<Body>>, HttpError>> + 'a,
    RpcReqFn: FnOnce(
        &'b mut RequestParts,
        &'b mut RpcRequest<GenericRpcMethod<String, Map<String, Value>>>,
    ) -> RpcReqFut,
    RpcReqFut: Future<Output = Result<Result<RpcResFn, Response<Body>>, HttpError>> + 'b,
    RpcResFn: FnOnce(&'c mut ResponseParts, &'c mut Result<Value, RpcError>) -> RpcResFut,
    RpcResFut: Future<Output = Result<Result<ResFn, Response<Body>>, HttpError>> + 'c,
    ResFn: FnOnce(&'d mut Response<Body>) -> ResFut,
    ResFut: Future<Output = Result<(), HttpError>> + 'd,
>(
    f: ReqFn,
) -> ReqFn {
    f
}
