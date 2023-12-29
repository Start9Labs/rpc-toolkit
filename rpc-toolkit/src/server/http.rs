use std::any::TypeId;
use std::task::Context;

use futures::future::{join_all, BoxFuture};
use futures::FutureExt;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::request::Parts;
use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::{RpcError, RpcMethod};

use crate::server::{RpcRequest, RpcResponse, SingleOrBatchRpcRequest};
use crate::util::{internal_error, parse_error};
use crate::{handler, HandleAny, Server};

const FALLBACK_ERROR: &str = "{\"error\":{\"code\":-32603,\"message\":\"Internal error\",\"data\":\"Failed to serialize rpc response\"}}";

pub fn fallback_rpc_error_response() -> Response<Bytes> {
    Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_LENGTH, FALLBACK_ERROR.len())
        .body(Bytes::from_static(FALLBACK_ERROR.as_bytes()))
        .unwrap()
}

pub fn json_http_response<T: Serialize>(t: &T) -> Response<Bytes> {
    let body = match serde_json::to_vec(t) {
        Ok(a) => a,
        Err(_) => return fallback_rpc_error_response(),
    };
    Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_LENGTH, body.len())
        .body(Bytes::from(body))
        .unwrap_or_else(|_| fallback_rpc_error_response())
}

type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

#[async_trait::async_trait]
pub trait Middleware<Context: Send + 'static>: Clone + Send + Sync + 'static {
    type Metadata: DeserializeOwned + Send + 'static;
    #[allow(unused_variables)]
    async fn process_http_request(
        &mut self,
        context: &Context,
        request: &mut Request<BoxBody>,
    ) -> Result<(), Response<Bytes>> {
        Ok(())
    }
    #[allow(unused_variables)]
    async fn process_rpc_request(
        &mut self,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        Ok(())
    }
    #[allow(unused_variables)]
    async fn process_rpc_response(&mut self, response: &mut RpcResponse) {}
    #[allow(unused_variables)]
    async fn process_http_response(&mut self, response: &mut Response<Bytes>) {}
}

#[allow(private_bounds)]
trait _Middleware<Context>: Send + Sync {
    fn dyn_clone(&self) -> DynMiddleware<Context>;
    fn process_http_request<'a>(
        &'a mut self,
        context: &'a Context,
        request: &'a mut Request<BoxBody>,
    ) -> BoxFuture<'a, Result<(), Response<Bytes>>>;
    fn process_rpc_request<'a>(
        &'a mut self,
        metadata: Value,
        request: &'a mut RpcRequest,
    ) -> BoxFuture<'a, Result<(), RpcResponse>>;
    fn process_rpc_response<'a>(&'a mut self, response: &'a mut RpcResponse) -> BoxFuture<'a, ()>;
    fn process_http_response<'a>(
        &'a mut self,
        response: &'a mut Response<Bytes>,
    ) -> BoxFuture<'a, ()>;
}
impl<Context: Send + 'static, T: Middleware<Context> + Send + Sync> _Middleware<Context> for T {
    fn dyn_clone(&self) -> DynMiddleware<Context> {
        DynMiddleware(Box::new(<Self as Clone>::clone(&self)))
    }
    fn process_http_request<'a>(
        &'a mut self,
        context: &'a Context,
        request: &'a mut Request<BoxBody>,
    ) -> BoxFuture<'a, Result<(), Response<Bytes>>> {
        <Self as Middleware<Context>>::process_http_request(self, context, request)
    }
    fn process_rpc_request<'a>(
        &'a mut self,
        metadata: Value,
        request: &'a mut RpcRequest,
    ) -> BoxFuture<'a, Result<(), RpcResponse>> {
        <Self as Middleware<Context>>::process_rpc_request(
            self,
            match imbl_value::from_value(metadata) {
                Ok(a) => a,
                Err(e) => return async { Err(internal_error(e).into()) }.boxed(),
            },
            request,
        )
    }
    fn process_rpc_response<'a>(&'a mut self, response: &'a mut RpcResponse) -> BoxFuture<'a, ()> {
        <Self as Middleware<Context>>::process_rpc_response(self, response)
    }
    fn process_http_response<'a>(
        &'a mut self,
        response: &'a mut Response<Bytes>,
    ) -> BoxFuture<'a, ()> {
        <Self as Middleware<Context>>::process_http_response(self, response)
    }
}

struct DynMiddleware<Context>(Box<dyn _Middleware<Context>>);
impl<Context> Clone for DynMiddleware<Context> {
    fn clone(&self) -> Self {
        self.0.dyn_clone()
    }
}

pub struct HttpServer<Context: crate::Context> {
    inner: Server<Context>,
    middleware: Vec<DynMiddleware<Context>>,
}
impl<Context: crate::Context> Clone for HttpServer<Context> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            middleware: self.middleware.clone(),
        }
    }
}
impl<Context: crate::Context> Server<Context> {
    pub fn for_http(self) -> HttpServer<Context> {
        HttpServer {
            inner: self,
            middleware: Vec::new(),
        }
    }
    pub fn middleware<T: Middleware<Context>>(self, middleware: T) -> HttpServer<Context> {
        self.for_http().middleware(middleware)
    }
}
impl<Context: crate::Context> HttpServer<Context> {
    pub fn middleware<T: Middleware<Context>>(mut self, middleware: T) -> Self {
        self.middleware.push(DynMiddleware(Box::new(middleware)));
        self
    }
    async fn process_http_request(&self, mut req: Request<BoxBody>) -> Response<Bytes> {
        let mut mid = self.middleware.clone();
        match async {
            let ctx = (self.inner.make_ctx)().await?;
            for middleware in mid.iter_mut().rev() {
                if let Err(e) = middleware.0.process_http_request(&ctx, &mut req).await {
                    return Ok::<_, RpcError>(e);
                }
            }
            let (_, body) = req.into_parts();
            match serde_json::from_slice::<SingleOrBatchRpcRequest>(
                &*body.collect().await.map_err(internal_error)?.to_bytes(),
            )
            .map_err(parse_error)?
            {
                SingleOrBatchRpcRequest::Single(rpc_req) => {
                    let mut res =
                        json_http_response(&self.process_rpc_request(&mut mid, rpc_req).await);
                    for middleware in mid.iter_mut() {
                        middleware.0.process_http_response(&mut res).await;
                    }
                    Ok(res)
                }
                SingleOrBatchRpcRequest::Batch(rpc_reqs) => {
                    let (mids, rpc_res): (Vec<_>, Vec<_>) =
                        join_all(rpc_reqs.into_iter().map(|rpc_req| async {
                            let mut mid = mid.clone();
                            let res = self.process_rpc_request(&mut mid, rpc_req).await;
                            (mid, res)
                        }))
                        .await
                        .into_iter()
                        .unzip();
                    let mut res = json_http_response(&rpc_res);
                    for mut mid in mids.into_iter().fold(
                        vec![Vec::with_capacity(rpc_res.len()); mid.len()],
                        |mut acc, x| {
                            for (idx, middleware) in x.into_iter().enumerate() {
                                acc[idx].push(middleware);
                            }
                            acc
                        },
                    ) {
                        for middleware in mid.iter_mut() {
                            middleware.0.process_http_response(&mut res).await;
                        }
                    }
                    Ok(res)
                }
            }
        }
        .await
        {
            Ok(a) => a,
            Err(e) => json_http_response(&RpcResponse {
                id: None,
                result: Err(e),
            }),
        }
    }
    async fn process_rpc_request(
        &self,
        mid: &mut Vec<DynMiddleware<Context>>,
        mut req: RpcRequest,
    ) -> RpcResponse {
        let metadata = Value::Object(
            self.inner
                .root_handler
                .metadata(
                    match self
                        .inner
                        .root_handler
                        .method_from_dots(req.method.as_str(), TypeId::of::<Context>())
                    {
                        Some(a) => a,
                        None => {
                            return RpcResponse {
                                id: req.id,
                                result: Err(yajrc::METHOD_NOT_FOUND_ERROR),
                            }
                        }
                    },
                    TypeId::of::<Context>(),
                )
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        );
        let mut res = async {
            for middleware in mid.iter_mut().rev() {
                if let Err(res) = middleware
                    .0
                    .process_rpc_request(metadata.clone(), &mut req)
                    .await
                {
                    return res;
                }
            }
            self.inner.handle_single_request(req).await
        }
        .await;
        for middleware in mid.iter_mut() {
            middleware.0.process_rpc_response(&mut res).await;
        }
        res
    }
    pub fn handle(&self, req: Request<Incoming>) -> BoxFuture<'static, Response<Bytes>> {
        let server = self.clone();
        async move {
            server
                .process_http_request(req.map(|b| BoxBody::new(b)))
                .await
        }
        .boxed()
    }
}

impl<Context: crate::Context> Service<Request<Incoming>> for HttpServer<Context> {
    type Response = Response<Bytes>;
    type Error = hyper::Error;
    type Future = futures::future::Map<
        BoxFuture<'static, Self::Response>,
        fn(Self::Response) -> Result<Self::Response, Self::Error>,
    >;
    fn call(&self, req: Request<Incoming>) -> Self::Future {
        self.handle(req).map(Ok)
    }
}
