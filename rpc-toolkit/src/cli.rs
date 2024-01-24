use std::any::TypeId;
use std::collections::VecDeque;
use std::ffi::OsString;

use clap::{CommandFactory, FromArgMatches};
use imbl_value::Value;
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use url::Url;
use yajrc::{Id, RpcError};

use crate::util::{internal_error, parse_error, Flat, PhantomData};
use crate::{
    AnyHandler, CliBindingsAny, DynHandler, HandleAny, HandleAnyArgs, Handler, HandlerArgs,
    HandlerArgsFor, HandlerTypes, IntoContext, Name, ParentHandler, PrintCliResult,
};

type GenericRpcMethod<'a> = yajrc::GenericRpcMethod<&'a str, Value, Value>;
type RpcRequest<'a> = yajrc::RpcRequest<GenericRpcMethod<'a>>;
type RpcResponse<'a> = yajrc::RpcResponse<GenericRpcMethod<'static>>;

pub struct CliApp<Context: crate::Context + Clone, Config: CommandFactory + FromArgMatches> {
    _phantom: PhantomData<(Context, Config)>,
    make_ctx: Box<dyn FnOnce(Config) -> Result<Context, RpcError> + Send + Sync>,
    root_handler: ParentHandler,
}
impl<Context: crate::Context + Clone, Config: CommandFactory + FromArgMatches>
    CliApp<Context, Config>
{
    pub fn new<MakeCtx: FnOnce(Config) -> Result<Context, RpcError> + Send + Sync + 'static>(
        make_ctx: MakeCtx,
        root_handler: ParentHandler,
    ) -> Self {
        Self {
            _phantom: PhantomData::new(),
            make_ctx: Box::new(make_ctx),
            root_handler,
        }
    }
    pub fn run(self, args: impl IntoIterator<Item = OsString>) -> Result<(), RpcError> {
        let ctx_ty = TypeId::of::<Context>();
        let mut cmd = Config::command();
        for (name, handlers) in &self.root_handler.subcommands.0 {
            if let (Name(Some(name)), Some(DynHandler::WithCli(handler))) = (
                name,
                if let Some(handler) = handlers.get(&Some(ctx_ty)) {
                    Some(handler)
                } else if let Some(handler) = handlers.get(&None) {
                    Some(handler)
                } else {
                    None
                },
            ) {
                cmd = cmd.subcommand(handler.cli_command(ctx_ty).name(name));
            }
        }
        let matches = cmd.get_matches_from(args);
        let config = Config::from_arg_matches(&matches)?;
        let ctx = (self.make_ctx)(config)?;
        let root_handler = AnyHandler::new(self.root_handler);
        let (method, params) = root_handler.cli_parse(&matches, ctx_ty)?;
        let res = root_handler.handle_sync(HandleAnyArgs {
            context: ctx.clone().upcast(),
            parent_method: VecDeque::new(),
            method: method.clone(),
            params: params.clone(),
        })?;
        root_handler.cli_display(
            HandleAnyArgs {
                context: ctx.upcast(),
                parent_method: VecDeque::new(),
                method,
                params,
            },
            res,
        )?;
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait CallRemote: crate::Context {
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

pub async fn call_remote_http(
    client: &Client,
    url: Url,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let rpc_req = RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::new(method),
        params,
    };
    let mut req = client.request(Method::POST, url);
    let body;
    #[cfg(feature = "cbor")]
    {
        req = req.header("content-type", "application/cbor");
        req = req.header("accept", "application/cbor, application/json");
        body = serde_cbor::to_vec(&rpc_req)?;
    }
    #[cfg(not(feature = "cbor"))]
    {
        req = req.header("content-type", "application/json");
        req = req.header("accept", "application/json");
        body = serde_json::to_vec(&req)?;
    }
    let res = req
        .header("content-length", body.len())
        .body(body)
        .send()
        .await?;

    match res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
    {
        Some("application/json") => {
            serde_json::from_slice::<RpcResponse>(&*res.bytes().await.map_err(internal_error)?)
                .map_err(parse_error)?
                .result
        }
        #[cfg(feature = "cbor")]
        Some("application/cbor") => {
            serde_cbor::from_slice::<RpcResponse>(&*res.bytes().await.map_err(internal_error)?)
                .map_err(parse_error)?
                .result
        }
        _ => Err(internal_error("missing content type")),
    }
}

pub async fn call_remote_socket(
    connection: impl AsyncRead + AsyncWrite,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let rpc_req = RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::new(method),
        params,
    };
    let conn = connection;
    tokio::pin!(conn);
    let mut buf = serde_json::to_vec(&rpc_req).map_err(|e| RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    })?;
    buf.push(b'\n');
    conn.write_all(&buf).await.map_err(|e| RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    })?;
    let mut line = String::new();
    BufReader::new(conn).read_line(&mut line).await?;
    serde_json::from_str::<RpcResponse>(&line)
        .map_err(parse_error)?
        .result
}

pub struct CallRemoteHandler<Context, RemoteHandler> {
    _phantom: PhantomData<Context>,
    handler: RemoteHandler,
}
impl<Context, RemoteHandler> CallRemoteHandler<Context, RemoteHandler> {
    pub fn new(handler: RemoteHandler) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: handler,
        }
    }
}
impl<Context, RemoteHandler: Clone> Clone for CallRemoteHandler<Context, RemoteHandler> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, RemoteHandler> std::fmt::Debug for CallRemoteHandler<Context, RemoteHandler> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CallRemoteHandler").finish()
    }
}

impl<Context, RemoteHandler> HandlerTypes for CallRemoteHandler<Context, RemoteHandler>
where
    RemoteHandler: HandlerTypes,
    RemoteHandler::Params: Serialize,
    RemoteHandler::InheritedParams: Serialize,
    RemoteHandler::Ok: DeserializeOwned,
    RemoteHandler::Err: From<RpcError>,
{
    type Params = RemoteHandler::Params;
    type InheritedParams = RemoteHandler::InheritedParams;
    type Ok = RemoteHandler::Ok;
    type Err = RemoteHandler::Err;
}
#[async_trait::async_trait]
impl<Context: CallRemote, RemoteHandler> Handler for CallRemoteHandler<Context, RemoteHandler>
where
    RemoteHandler: Handler,
    RemoteHandler::Params: Serialize,
    RemoteHandler::InheritedParams: Serialize,
    RemoteHandler::Ok: DeserializeOwned,
    RemoteHandler::Err: From<RpcError>,
{
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let full_method = handle_args
            .parent_method
            .into_iter()
            .chain(handle_args.method)
            .collect::<Vec<_>>();
        match handle_args
            .context
            .call_remote(
                &full_method.join("."),
                imbl_value::to_value(&Flat(handle_args.params, handle_args.inherited_params))
                    .map_err(parse_error)?,
            )
            .await
        {
            Ok(a) => imbl_value::from_value(a)
                .map_err(internal_error)
                .map_err(Self::Err::from),
            Err(e) => Err(Self::Err::from(e)),
        }
    }
}
impl<Context: CallRemote, RemoteHandler> PrintCliResult
    for CallRemoteHandler<Context, RemoteHandler>
where
    RemoteHandler: PrintCliResult<Context = Context>,
    RemoteHandler::Params: Serialize,
    RemoteHandler::InheritedParams: Serialize,
    RemoteHandler::Ok: DeserializeOwned,
    RemoteHandler::Err: From<RpcError>,
{
    type Context = Context;
    fn print(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.print(
            HandlerArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}
