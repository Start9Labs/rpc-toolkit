use std::borrow::Cow;
use std::sync::Arc;

use futures::future::{join_all, BoxFuture};
use futures::{Future, FutureExt, Stream, StreamExt};
use imbl_value::Value;
use yajrc::{AnyParams, AnyRpcMethod, RpcError, RpcMethod};

use crate::util::{invalid_request, JobRunner};
use crate::DynCommand;

type GenericRpcMethod = yajrc::GenericRpcMethod<String, Value, Value>;
type RpcRequest = yajrc::RpcRequest<GenericRpcMethod>;
type RpcResponse = yajrc::RpcResponse<GenericRpcMethod>;
type SingleOrBatchRpcRequest = yajrc::SingleOrBatchRpcRequest<GenericRpcMethod>;

mod http;
mod socket;

pub use http::*;
pub use socket::*;

impl<Context: crate::Context> DynCommand<Context> {
    fn cmd_from_method(
        &self,
        method: &[&str],
        parent_method: Vec<&'static str>,
    ) -> Result<(Vec<&'static str>, &DynCommand<Context>), RpcError> {
        let mut ret_method = parent_method;
        ret_method.push(self.name);
        if let Some((cmd, rest)) = method.split_first() {
            self.subcommands
                .iter()
                .find(|c| c.name == *cmd)
                .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                .cmd_from_method(rest, ret_method)
        } else {
            Ok((ret_method, self))
        }
    }
}

pub struct Server<Context: crate::Context> {
    commands: Vec<DynCommand<Context>>,
    make_ctx: Arc<dyn Fn() -> BoxFuture<'static, Result<Context, RpcError>> + Send + Sync>,
}
impl<Context: crate::Context> Server<Context> {
    pub fn new<
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Context, RpcError>> + Send + 'static,
    >(
        commands: Vec<DynCommand<Context>>,
        make_ctx: F,
    ) -> Self {
        Server {
            commands,
            make_ctx: Arc::new(move || make_ctx().boxed()),
        }
    }

    pub fn handle_command(
        &self,
        method: &str,
        params: Value,
    ) -> impl Future<Output = Result<Value, RpcError>> + Send + 'static {
        let from_self = (|| {
            let method: Vec<_> = method.split(".").collect();
            let (cmd, rest) = method.split_first().ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?;
            let (method, cmd) = self
                .commands
                .iter()
                .find(|c| c.name == *cmd)
                .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                .cmd_from_method(rest, Vec::new())?;
            Ok::<_, RpcError>((
                cmd.implementation
                    .as_ref()
                    .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                    .async_impl
                    .clone(),
                self.make_ctx.clone(),
                method,
                params,
            ))
        })();

        async move {
            let (implementation, make_ctx, method, params) = from_self?;
            implementation(make_ctx().await?, method, params).await
        }
    }

    fn handle_single_request(
        &self,
        RpcRequest { id, method, params }: RpcRequest,
    ) -> impl Future<Output = RpcResponse> + Send + 'static {
        let handle = (|| Ok::<_, RpcError>(self.handle_command(method.as_str(), params)))();
        async move {
            RpcResponse {
                id,
                result: match handle {
                    Ok(handle) => handle.await,
                    Err(e) => Err(e),
                },
            }
        }
    }

    pub fn handle(
        &self,
        request: Result<Value, RpcError>,
    ) -> BoxFuture<'static, Result<Value, imbl_value::Error>> {
        match request.and_then(|request| {
            imbl_value::from_value::<SingleOrBatchRpcRequest>(request).map_err(invalid_request)
        }) {
            Ok(SingleOrBatchRpcRequest::Single(req)) => {
                let fut = self.handle_single_request(req);
                async { imbl_value::to_value(&fut.await) }.boxed()
            }
            Ok(SingleOrBatchRpcRequest::Batch(reqs)) => {
                let futs: Vec<_> = reqs
                    .into_iter()
                    .map(|req| self.handle_single_request(req))
                    .collect();
                async { imbl_value::to_value(&join_all(futs).await) }.boxed()
            }
            Err(e) => async {
                imbl_value::to_value(&RpcResponse {
                    id: None,
                    result: Err(e),
                })
            }
            .boxed(),
        }
    }

    pub fn stream<'a>(
        &'a self,
        requests: impl Stream<Item = Result<Value, RpcError>> + Send + 'a,
    ) -> impl Stream<Item = Result<Value, imbl_value::Error>> + 'a {
        async_stream::try_stream! {
            let mut runner = JobRunner::new();
            let requests = requests.fuse().map(|req| self.handle(req));
            tokio::pin!(requests);

            while let Some(res) = runner.next_result(&mut requests).await.transpose()? {
                yield res;
            }
        }
    }
}
