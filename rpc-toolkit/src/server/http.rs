use std::task::Context;

use futures::future::BoxFuture;
use http::request::Parts;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};
use yajrc::{RpcRequest, RpcResponse};

type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

#[async_trait::async_trait]
pub trait Middleware<Context: crate::Context> {
    type ProcessHttpRequestResult;
    async fn process_http_request(
        &self,
        req: &mut Request<BoxBody>,
    ) -> Result<Self::ProcessHttpRequestResult, hyper::Result<Response<Bytes>>>;
    type ProcessRpcRequestResult;
    async fn process_rpc_request(
        &self,
        prev: Self::ProcessHttpRequestResult,
        // metadata: &Context::Metadata,
        req: &mut RpcRequest,
    ) -> Result<Self::ProcessRpcRequestResult, RpcResponse>;
    type ProcessRpcResponseResult;
    async fn process_rpc_response(
        &self,
        prev: Self::ProcessRpcRequestResult,
        res: &mut RpcResponse,
    ) -> Self::ProcessRpcResponseResult;
    async fn process_http_response(
        &self,
        prev: Self::ProcessRpcResponseResult,
        res: &mut Response<Bytes>,
    );
}

// pub struct DynMiddleware<Context: crate::Context> {
//     process_http_request: Box<
//         dyn for<'a> Fn(
//                 &'a mut Request<BoxBody>,
//             ) -> BoxFuture<
//                 'a,
//                 Result<DynProcessRpcRequest<Context>, hyper::Result<Response<Bytes>>>,
//             > + Send
//             + Sync,
//     >,
// }
// type DynProcessRpcRequest<'m, Context: crate::Context> = Box<
//     dyn for<'a> FnOnce(
//             &'a Context::Metadata,
//             &'a mut RpcRequest,
//         )
//             -> BoxFuture<'a, Result<DynProcessRpcResponse<'m>, DynSkipHandler<'m>>>
//         + Send
//         + Sync
//         + 'm,
// >;
// type DynProcessRpcResponse<'m> =
//     Box<dyn for<'a> FnOnce(&'a mut RpcResponse) -> BoxFuture<'a, DynProcessHttpResponse<'m>>>;
