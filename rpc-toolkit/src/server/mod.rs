use std::sync::Arc;

use futures::future::{join_all, BoxFuture};
use futures::stream::{BoxStream, Fuse};
use futures::{Future, FutureExt, Stream, StreamExt, TryStreamExt};
use imbl_value::Value;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use yajrc::{AnyParams, RpcError, RpcMethod, RpcRequest, RpcResponse, SingleOrBatchRpcRequest};

use crate::util::{invalid_request, parse_error};
use crate::DynCommand;

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
        let handle = (|| {
            Ok::<_, RpcError>(self.handle_command(
                method.as_str(),
                match params {
                    AnyParams::Named(a) => serde_json::Value::Object(a).into(),
                    _ => {
                        return Err(RpcError {
                            data: Some("positional parameters unsupported".into()),
                            ..yajrc::INVALID_PARAMS_ERROR
                        })
                    }
                },
            ))
        })();
        async move {
            RpcResponse {
                id,
                result: match handle {
                    Ok(handle) => handle.await.map(serde_json::Value::from),
                    Err(e) => Err(e),
                },
            }
        }
    }

    pub fn handle(&self, request: Value) -> BoxFuture<'static, Result<Value, RpcError>> {
        let request =
            imbl_value::from_value::<SingleOrBatchRpcRequest>(request).map_err(invalid_request);
        match request {
            Ok(SingleOrBatchRpcRequest::Single(req)) => {
                let fut = self.handle_single_request(req);
                async { imbl_value::to_value(&fut.await).map_err(parse_error) }.boxed()
            }
            Ok(SingleOrBatchRpcRequest::Batch(reqs)) => {
                let futs: Vec<_> = reqs
                    .into_iter()
                    .map(|req| self.handle_single_request(req))
                    .collect();
                async { imbl_value::to_value(&join_all(futs).await).map_err(parse_error) }.boxed()
            }
            Err(e) => async { Err(e) }.boxed(),
        }
    }

    pub fn stream<'a>(
        &'a self,
        requests: impl Stream<Item = Result<Value, RpcError>> + Send + 'a,
    ) -> impl Stream<Item = Result<Value, RpcError>> + 'a {
        let mut running = RunningCommands::default();
        let mut requests = requests.boxed().fuse();
        async fn next<'a, Context: crate::Context>(
            server: &'a Server<Context>,
            running: &mut RunningCommands,
            requests: &mut Fuse<BoxStream<'a, Result<Value, RpcError>>>,
        ) -> Result<Option<Value>, RpcError> {
            loop {
                tokio::select! {
                    req = requests.try_next() => {
                        let req = req?;
                        if let Some(req) = req {
                            running.running.push(tokio::spawn(server.handle(req)));
                        } else {
                            running.closed = true;
                        }
                    }
                    res = running.try_next() => {
                        return res;
                    }
                }
            }
        }
        async_stream::try_stream! {
            while let Some(res) = next(self, &mut running, &mut requests).await? {
                yield res;
            }
        }
    }
}

#[derive(Default)]
struct RunningCommands {
    closed: bool,
    running: Vec<JoinHandle<Result<Value, RpcError>>>,
}

impl Stream for RunningCommands {
    type Item = Result<Value, RpcError>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let item = self
            .running
            .iter_mut()
            .enumerate()
            .find_map(|(i, f)| match f.poll_unpin(cx) {
                std::task::Poll::Pending => None,
                std::task::Poll::Ready(e) => Some((
                    i,
                    e.map_err(|e| RpcError {
                        data: Some(e.to_string().into()),
                        ..yajrc::INTERNAL_ERROR
                    })
                    .and_then(|a| a),
                )),
            });
        match item {
            Some((idx, res)) => {
                drop(self.running.swap_remove(idx));
                std::task::Poll::Ready(Some(res))
            }
            None => {
                if !self.closed || !self.running.is_empty() {
                    std::task::Poll::Pending
                } else {
                    std::task::Poll::Ready(None)
                }
            }
        }
    }
}
impl Drop for RunningCommands {
    fn drop(&mut self) {
        for hdl in &self.running {
            hdl.abort();
        }
        if let Ok(rt) = Handle::try_current() {
            rt.block_on(join_all(std::mem::take(&mut self.running).into_iter()));
        }
    }
}
