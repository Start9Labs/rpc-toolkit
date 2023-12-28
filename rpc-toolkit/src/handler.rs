use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Display;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches, Parser};
use futures::Future;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

use crate::context::{AnyContext, IntoContext};
use crate::util::{combine, internal_error, invalid_params, Flat};
use crate::{CallRemote, CallRemoteHandler};

pub(crate) struct HandleAnyArgs {
    pub(crate) context: AnyContext,
    pub(crate) parent_method: Vec<&'static str>,
    pub(crate) method: VecDeque<&'static str>,
    pub(crate) params: Value,
}
impl HandleAnyArgs {
    fn downcast<Context: IntoContext, H>(self) -> Result<HandleArgs<Context, H>, imbl_value::Error>
    where
        H: HandlerTypes,
        H::Params: DeserializeOwned,
        H::InheritedParams: DeserializeOwned,
    {
        let Self {
            context,
            parent_method,
            method,
            params,
        } = self;
        Ok(HandleArgs {
            context: Context::downcast(context).map_err(|_| imbl_value::Error {
                kind: imbl_value::ErrorKind::Deserialization,
                source: serde::ser::Error::custom("context does not match expected"),
            })?,
            parent_method,
            method,
            params: imbl_value::from_value(params.clone())?,
            inherited_params: imbl_value::from_value(params.clone())?,
            raw_params: params,
        })
    }
}

#[async_trait::async_trait]
pub(crate) trait HandleAny: std::fmt::Debug + Send + Sync {
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError>;
    async fn handle_async(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError>;
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>>;
}
#[async_trait::async_trait]
impl<T: HandleAny> HandleAny for Arc<T> {
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        self.deref().handle_sync(handle_args)
    }
    async fn handle_async(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        self.deref().handle_async(handle_args).await
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.deref().method_from_dots(method, ctx_ty)
    }
}

pub(crate) trait CliBindingsAny {
    fn cli_command(&self, ctx_ty: TypeId) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(&self, handle_args: HandleAnyArgs, result: Value) -> Result<(), RpcError>;
}

pub trait CliBindings<Context: IntoContext>: HandlerTypes {
    fn cli_command(&self, ctx_ty: TypeId) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

pub trait PrintCliResult<Context: IntoContext>: HandlerTypes {
    fn print(
        &self,
        handle_args: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

pub(crate) trait HandleAnyWithCli: HandleAny + CliBindingsAny {}
impl<T: HandleAny + CliBindingsAny> HandleAnyWithCli for T {}

#[derive(Debug, Clone)]
pub(crate) enum DynHandler {
    WithoutCli(Arc<dyn HandleAny>),
    WithCli(Arc<dyn HandleAnyWithCli>),
}
#[async_trait::async_trait]
impl HandleAny for DynHandler {
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        match self {
            DynHandler::WithoutCli(h) => h.handle_sync(handle_args),
            DynHandler::WithCli(h) => h.handle_sync(handle_args),
        }
    }
    async fn handle_async(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        match self {
            DynHandler::WithoutCli(h) => h.handle_async(handle_args).await,
            DynHandler::WithCli(h) => h.handle_async(handle_args).await,
        }
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        match self {
            DynHandler::WithoutCli(h) => h.method_from_dots(method, ctx_ty),
            DynHandler::WithCli(h) => h.method_from_dots(method, ctx_ty),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HandleArgs<Context: IntoContext, H: HandlerTypes + ?Sized> {
    pub context: Context,
    pub parent_method: Vec<&'static str>,
    pub method: VecDeque<&'static str>,
    pub params: H::Params,
    pub inherited_params: H::InheritedParams,
    pub raw_params: Value,
}

pub trait HandlerTypes {
    type Params: Send + Sync;
    type InheritedParams: Send + Sync;
    type Ok: Send + Sync;
    type Err: Send + Sync;
}

#[async_trait::async_trait]
pub trait Handler<Context: IntoContext>:
    HandlerTypes + std::fmt::Debug + Clone + Send + Sync + 'static
{
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        handle_args
            .context
            .runtime()
            .block_on(self.handle_async(handle_args))
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err>;
    async fn handle_async_with_sync(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handle_sync(handle_args)
    }
    async fn handle_async_with_sync_blocking(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let s = self.clone();
        handle_args
            .context
            .runtime()
            .spawn_blocking(move || s.handle_sync(handle_args))
            .await
            .unwrap()
    }
    fn contexts(&self) -> Option<BTreeSet<TypeId>> {
        Context::type_ids_for(self)
    }
    #[allow(unused_variables)]
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        if method.is_empty() {
            Some(VecDeque::new())
        } else {
            None
        }
    }
}

pub(crate) struct AnyHandler<Context, H> {
    _ctx: PhantomData<Context>,
    handler: H,
}
impl<Context, H> AnyHandler<Context, H> {
    pub(crate) fn new(handler: H) -> Self {
        Self {
            _ctx: PhantomData,
            handler,
        }
    }
}
impl<Context, H: std::fmt::Debug> std::fmt::Debug for AnyHandler<Context, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AnyHandler").field(&self.handler).finish()
    }
}

#[async_trait::async_trait]
impl<Context: IntoContext, H: Handler<Context>> HandleAny for AnyHandler<Context, H>
where
    H::Params: DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize,
    RpcError: From<H::Err>,
{
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
                .handle_sync(handle_args.downcast().map_err(invalid_params)?)?,
        )
        .map_err(internal_error)
    }
    async fn handle_async(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
                .handle_async(handle_args.downcast().map_err(invalid_params)?)
                .await?,
        )
        .map_err(internal_error)
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method, ctx_ty)
    }
}

impl<Context: IntoContext, H: CliBindings<Context>> CliBindingsAny for AnyHandler<Context, H>
where
    H: CliBindings<Context>,
    H::Params: DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches, ctx_ty)
    }
    fn cli_display(&self, handle_args: HandleAnyArgs, result: Value) -> Result<(), RpcError> {
        self.handler
            .cli_display(
                handle_args.downcast().map_err(invalid_params)?,
                imbl_value::from_value(result).map_err(internal_error)?,
            )
            .map_err(RpcError::from)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
pub struct NoParams {}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
pub enum Never {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Name(pub(crate) Option<&'static str>);
impl<'a> std::borrow::Borrow<Option<&'a str>> for Name {
    fn borrow(&self) -> &Option<&'a str> {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubcommandMap(pub(crate) BTreeMap<Name, BTreeMap<Option<TypeId>, DynHandler>>);
impl SubcommandMap {
    fn insert(
        &mut self,
        ctx_tys: Option<BTreeSet<TypeId>>,
        name: Option<&'static str>,
        handler: DynHandler,
    ) {
        let mut for_name = self.0.remove(&name).unwrap_or_default();
        if let Some(ctx_tys) = ctx_tys {
            for ctx_ty in ctx_tys {
                for_name.insert(Some(ctx_ty), handler.clone());
            }
        } else {
            for_name.insert(None, handler);
        }
        self.0.insert(Name(name), for_name);
    }

    fn get<'a>(&'a self, ctx_ty: TypeId, name: Option<&str>) -> Option<(Name, &'a DynHandler)> {
        if let Some((name, for_name)) = self.0.get_key_value(&name) {
            if let Some(for_ctx) = for_name.get(&Some(ctx_ty)) {
                Some((*name, for_ctx))
            } else {
                for_name.get(&None).map(|h| (*name, h))
            }
        } else {
            None
        }
    }
}

pub struct ParentHandler<Params = NoParams, InheritedParams = NoParams> {
    _phantom: PhantomData<(Params, InheritedParams)>,
    pub(crate) subcommands: SubcommandMap,
}
impl<Params, InheritedParams> ParentHandler<Params, InheritedParams> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
            subcommands: SubcommandMap(BTreeMap::new()),
        }
    }
}
impl<Params, InheritedParams> Clone for ParentHandler<Params, InheritedParams> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            subcommands: self.subcommands.clone(),
        }
    }
}
impl<Params, InheritedParams> std::fmt::Debug for ParentHandler<Params, InheritedParams> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParentHandler")
            .field(&self.subcommands)
            .finish()
    }
}

struct InheritanceHandler<Context, Params, InheritedParams, H, F> {
    _phantom: PhantomData<(Context, Params, InheritedParams)>,
    handler: H,
    inherit: F,
}
impl<Context, Params, InheritedParams, H: Clone, F: Clone> Clone
    for InheritanceHandler<Context, Params, InheritedParams, H, F>
{
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            handler: self.handler.clone(),
            inherit: self.inherit.clone(),
        }
    }
}
impl<Context, Params, InheritedParams, H: std::fmt::Debug, F> std::fmt::Debug
    for InheritanceHandler<Context, Params, InheritedParams, H, F>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("InheritanceHandler")
            .field(&self.handler)
            .finish()
    }
}
impl<Context, Params, InheritedParams, H, F> HandlerTypes
    for InheritanceHandler<Context, Params, InheritedParams, H, F>
where
    Context: IntoContext,
    H: HandlerTypes,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = H::Params;
    type InheritedParams = Flat<Params, InheritedParams>;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<Context, Params, InheritedParams, H, F> Handler<Context>
    for InheritanceHandler<Context, Params, InheritedParams, H, F>
where
    Context: IntoContext,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: Handler<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
            raw_params,
        })
    }
    async fn handle_async(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
            raw_params,
        })
    }
}

impl<Context, Params, InheritedParams, H, F> CliBindings<Context>
    for InheritanceHandler<Context, Params, InheritedParams, H, F>
where
    Context: IntoContext,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: CliBindings<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches, ctx_ty)
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.cli_display(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
                raw_params,
            },
            result,
        )
    }
}

impl<Params: Send + Sync, InheritedParams: Send + Sync> ParentHandler<Params, InheritedParams> {
    pub fn subcommand<Context, H>(mut self, name: &'static str, handler: H) -> Self
    where
        Context: IntoContext,
        H: HandlerTypes<InheritedParams = NoParams>
            + Handler<Context>
            + CliBindings<Context>
            + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize + DeserializeOwned,
        RpcError: From<H::Err>,
    {
        self.subcommands.insert(
            handler.contexts(),
            name.into(),
            DynHandler::WithCli(Arc::new(AnyHandler::new(handler))),
        );
        self
    }
    pub fn subcommand_remote_cli<CliContext, ServerContext, H>(
        mut self,
        name: &'static str,
        handler: H,
    ) -> Self
    where
        ServerContext: IntoContext,
        CliContext: IntoContext + CallRemote,
        H: HandlerTypes<InheritedParams = NoParams>
            + Handler<ServerContext>
            + CliBindings<CliContext>
            + 'static,
        H::Params: Serialize + DeserializeOwned,
        H::Ok: Serialize + DeserializeOwned,
        RpcError: From<H::Err>,
        H::Err: From<RpcError>,
        CallRemoteHandler<ServerContext, H>: Handler<CliContext>,
        <CallRemoteHandler<ServerContext, H> as HandlerTypes>::Ok: Serialize + DeserializeOwned,
        <CallRemoteHandler<ServerContext, H> as HandlerTypes>::Params: Serialize + DeserializeOwned,
        <CallRemoteHandler<ServerContext, H> as HandlerTypes>::InheritedParams: DeserializeOwned,
        RpcError: From<<CallRemoteHandler<ServerContext, H> as HandlerTypes>::Err>,
    {
        self.subcommands.insert(
            handler.contexts(),
            name.into(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(handler.clone()))),
        );
        let call_remote = CallRemoteHandler::<ServerContext, H>::new(handler);
        self.subcommands.insert(
            call_remote.contexts(),
            name.into(),
            DynHandler::WithCli(Arc::new(AnyHandler::new(call_remote))),
        );
        self
    }
    pub fn subcommand_no_cli<Context, H>(mut self, name: &'static str, handler: H) -> Self
    where
        Context: IntoContext,
        H: Handler<Context, InheritedParams = NoParams> + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize,
        RpcError: From<H::Err>,
    {
        self.subcommands.insert(
            handler.contexts(),
            name.into(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(handler))),
        );
        self
    }
}
impl<Params: Send + Sync, InheritedParams: Send + Sync> ParentHandler<Params, InheritedParams>
where
    Params: DeserializeOwned + 'static,
    InheritedParams: DeserializeOwned + 'static,
{
    pub fn subcommand_with_inherited<Context, H, F>(
        mut self,
        name: &'static str,
        handler: H,
        inherit: F,
    ) -> Self
    where
        Context: IntoContext,
        H: Handler<Context> + CliBindings<Context> + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize + DeserializeOwned,
        RpcError: From<H::Err>,
        F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
    {
        self.subcommands.insert(
            handler.contexts(),
            name.into(),
            DynHandler::WithCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler: InheritanceHandler::<Context, Params, InheritedParams, H, F> {
                    _phantom: PhantomData,
                    handler,
                    inherit,
                },
            })),
        );
        self
    }
    pub fn subcommand_with_inherited_remote_cli<CliContext, ServerContext, H, F>(
        mut self,
        name: &'static str,
        handler: H,
        inherit: F,
    ) -> Self
    where
        ServerContext: IntoContext,
        CliContext: IntoContext + CallRemote,
        H: HandlerTypes + Handler<ServerContext> + CliBindings<CliContext> + 'static,
        H::Params: Serialize + DeserializeOwned,
        H::Ok: Serialize + DeserializeOwned,
        RpcError: From<H::Err>,
        H::Err: From<RpcError>,
        CallRemoteHandler<ServerContext, H>:
            Handler<CliContext, InheritedParams = H::InheritedParams>,
        <CallRemoteHandler<ServerContext, H> as HandlerTypes>::Ok: Serialize + DeserializeOwned,
        <CallRemoteHandler<ServerContext, H> as HandlerTypes>::Params: Serialize + DeserializeOwned,
        <CallRemoteHandler<ServerContext, H> as HandlerTypes>::InheritedParams:
            Serialize + DeserializeOwned,
        RpcError: From<<CallRemoteHandler<ServerContext, H> as HandlerTypes>::Err>,
        F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
    {
        self.subcommands.insert(
            handler.contexts(),
            name.into(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(InheritanceHandler::<
                ServerContext,
                Params,
                InheritedParams,
                H,
                F,
            > {
                _phantom: PhantomData,
                handler: handler.clone(),
                inherit: inherit.clone(),
            }))),
        );
        let call_remote = CallRemoteHandler::<ServerContext, H>::new(handler);
        self.subcommands.insert(
            call_remote.contexts(),
            name.into(),
            DynHandler::WithCli(Arc::new(AnyHandler::new(InheritanceHandler::<
                CliContext,
                Params,
                InheritedParams,
                CallRemoteHandler<ServerContext, H>,
                F,
            > {
                _phantom: PhantomData,
                handler: call_remote,
                inherit,
            }))),
        );
        self
    }
    pub fn subcommand_with_inherited_no_cli<Context, H, F>(
        mut self,
        name: &'static str,
        handler: H,
        inherit: F,
    ) -> Self
    where
        Context: IntoContext,
        H: Handler<Context> + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize,
        RpcError: From<H::Err>,
        F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
    {
        self.subcommands.insert(
            handler.contexts(),
            name.into(),
            DynHandler::WithoutCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler: InheritanceHandler::<Context, Params, InheritedParams, H, F> {
                    _phantom: PhantomData,
                    handler,
                    inherit,
                },
            })),
        );
        self
    }
    pub fn root_handler<Context, H, F>(mut self, handler: H, inherit: F) -> Self
    where
        Context: IntoContext,
        H: HandlerTypes<Params = NoParams> + Handler<Context> + CliBindings<Context> + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize + DeserializeOwned,
        RpcError: From<H::Err>,
        F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
    {
        self.subcommands.insert(
            handler.contexts(),
            None,
            DynHandler::WithCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler: InheritanceHandler::<Context, Params, InheritedParams, H, F> {
                    _phantom: PhantomData,
                    handler,
                    inherit,
                },
            })),
        );
        self
    }
    pub fn root_handler_no_cli<Context, H, F>(mut self, handler: H, inherit: F) -> Self
    where
        Context: IntoContext,
        H: Handler<Context, Params = NoParams> + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize,
        RpcError: From<H::Err>,
        F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
    {
        self.subcommands.insert(
            handler.contexts(),
            None,
            DynHandler::WithoutCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler: InheritanceHandler::<Context, Params, InheritedParams, H, F> {
                    _phantom: PhantomData,
                    handler,
                    inherit,
                },
            })),
        );
        self
    }
}

impl<Params, InheritedParams> HandlerTypes for ParentHandler<Params, InheritedParams>
where
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = Value;
    type Err = RpcError;
}
#[async_trait::async_trait]
impl<Context, Params, InheritedParams> Handler<Context> for ParentHandler<Params, InheritedParams>
where
    Context: IntoContext,
    Params: Serialize + Send + Sync + 'static,
    InheritedParams: Serialize + Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            raw_params,
            ..
        }: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push(cmd);
        }
        if let Some((_, sub_handler)) = &self.subcommands.get(context.inner_type_id(), cmd) {
            sub_handler.handle_sync(HandleAnyArgs {
                context: context.upcast(),
                parent_method,
                method,
                params: raw_params,
            })
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    async fn handle_async(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            raw_params,
            ..
        }: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push(cmd);
        }
        if let Some((_, sub_handler)) = self.subcommands.get(context.inner_type_id(), cmd) {
            sub_handler
                .handle_async(HandleAnyArgs {
                    context: context.upcast(),
                    parent_method,
                    method,
                    params: raw_params,
                })
                .await
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    fn contexts(&self) -> Option<BTreeSet<TypeId>> {
        let mut set = BTreeSet::new();
        for ctx_ty in self.subcommands.0.values().flat_map(|c| c.keys()) {
            set.insert((*ctx_ty)?);
        }
        Some(set)
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        let (head, tail) = if method.is_empty() {
            (None, None)
        } else {
            method
                .split_once(".")
                .map(|(head, tail)| (Some(head), Some(tail)))
                .unwrap_or((Some(method), None))
        };
        let (Name(name), h) = self.subcommands.get(ctx_ty, head)?;
        let mut res = VecDeque::new();
        if let Some(name) = name {
            res.push_back(name);
        }
        if let Some(tail) = tail {
            res.append(&mut h.method_from_dots(tail, ctx_ty)?);
        }
        Some(res)
    }
}

impl<Params, InheritedParams> CliBindings<AnyContext> for ParentHandler<Params, InheritedParams>
where
    Params: FromArgMatches + CommandFactory + Serialize + Send + Sync + 'static,
    InheritedParams: Serialize + Send + Sync + 'static,
{
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        let mut base = Params::command();
        for (name, handlers) in &self.subcommands.0 {
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
                base = base.subcommand(handler.cli_command(ctx_ty).name(name));
            }
        }
        base
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        let root_params = imbl_value::to_value(&Params::from_arg_matches(matches)?)
            .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?;
        let (name, matches) = match matches.subcommand() {
            Some((name, matches)) => (Some(name), matches),
            None => (None, matches),
        };
        if let Some((Name(Some(name)), DynHandler::WithCli(handler))) =
            self.subcommands.get(ctx_ty, name)
        {
            let (mut method, params) = handler.cli_parse(matches, ctx_ty)?;
            method.push_front(name);

            Ok((
                method,
                combine(root_params, params)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ArgumentConflict, e))?,
            ))
        } else {
            Ok((VecDeque::new(), root_params))
        }
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            raw_params,
            ..
        }: HandleArgs<AnyContext, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push(cmd);
        }
        if let Some((_, DynHandler::WithCli(sub_handler))) =
            self.subcommands.get(context.inner_type_id(), cmd)
        {
            sub_handler.cli_display(
                HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: raw_params,
                },
                result,
            )
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
}

pub struct FromFn<F, T, E, Args> {
    _phantom: PhantomData<(T, E, Args)>,
    function: F,
    blocking: bool,
}
impl<F: Clone, T, E, Args> Clone for FromFn<F, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            function: self.function.clone(),
            blocking: self.blocking,
        }
    }
}
impl<F, T, E, Args> std::fmt::Debug for FromFn<F, T, E, Args> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromFn")
            .field("blocking", &self.blocking)
            .finish()
    }
}
impl<Context, F, T, E, Args> PrintCliResult<Context> for FromFn<F, T, E, Args>
where
    Context: IntoContext,
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    fn print(&self, _: HandleArgs<Context, Self>, result: Self::Ok) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}

pub fn from_fn<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args> {
    FromFn {
        function,
        _phantom: PhantomData,
        blocking: false,
    }
}

pub fn from_fn_blocking<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args> {
    FromFn {
        function,
        _phantom: PhantomData,
        blocking: true,
    }
}

pub struct FromFnAsync<F, Fut, T, E, Args> {
    _phantom: PhantomData<(Fut, T, E, Args)>,
    function: F,
}
impl<F: Clone, Fut, T, E, Args> Clone for FromFnAsync<F, Fut, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            function: self.function.clone(),
        }
    }
}
impl<F, Fut, T, E, Args> std::fmt::Debug for FromFnAsync<F, Fut, T, E, Args> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromFnAsync").finish()
    }
}
impl<Context, F, Fut, T, E, Args> PrintCliResult<Context> for FromFnAsync<F, Fut, T, E, Args>
where
    Context: IntoContext,
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    fn print(&self, _: HandleArgs<Context, Self>, result: Self::Ok) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}

pub fn from_fn_async<F, Fut, T, E, Args>(function: F) -> FromFnAsync<F, Fut, T, E, Args> {
    FromFnAsync {
        function,
        _phantom: PhantomData,
    }
}

impl<F, T, E> HandlerTypes for FromFn<F, T, E, ()>
where
    F: Fn() -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = NoParams;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, T, E> Handler<Context> for FromFn<F, T, E, ()>
where
    Context: IntoContext,
    F: Fn() -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(&self, _: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)()
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
}
impl<F, Fut, T, E> HandlerTypes for FromFnAsync<F, Fut, T, E, ()>
where
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = NoParams;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, Fut, T, E> Handler<Context> for FromFnAsync<F, Fut, T, E, ()>
where
    Context: IntoContext,
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(&self, _: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)().await
    }
}

impl<Context, F, T, E> HandlerTypes for FromFn<F, T, E, (Context,)>
where
    Context: IntoContext,
    F: Fn(Context) -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = NoParams;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, T, E> Handler<Context> for FromFn<F, T, E, (Context,)>
where
    Context: IntoContext,
    F: Fn(Context) -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context)
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
}
impl<Context, F, Fut, T, E> HandlerTypes for FromFnAsync<F, Fut, T, E, (Context,)>
where
    Context: IntoContext,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = NoParams;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, Fut, T, E> Handler<Context> for FromFnAsync<F, Fut, T, E, (Context,)>
where
    Context: IntoContext,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context).await
    }
}

impl<Context, F, T, E, Params> HandlerTypes for FromFn<F, T, E, (Context, Params)>
where
    Context: IntoContext,
    F: Fn(Context, Params) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, T, E, Params> Handler<Context> for FromFn<F, T, E, (Context, Params)>
where
    Context: IntoContext,
    F: Fn(Context, Params) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params)
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
}

impl<Context, F, Fut, T, E, Params> HandlerTypes for FromFnAsync<F, Fut, T, E, (Context, Params)>
where
    Context: IntoContext,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, Fut, T, E, Params> Handler<Context>
    for FromFnAsync<F, Fut, T, E, (Context, Params)>
where
    Context: IntoContext,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params).await
    }
}

impl<Context, F, T, E, Params, InheritedParams> HandlerTypes
    for FromFn<F, T, E, (Context, Params, InheritedParams)>
where
    Context: IntoContext,
    F: Fn(Context, Params, InheritedParams) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, T, E, Params, InheritedParams> Handler<Context>
    for FromFn<F, T, E, (Context, Params, InheritedParams)>
where
    Context: IntoContext,
    F: Fn(Context, Params, InheritedParams) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context,
            params,
            inherited_params,
            ..
        } = handle_args;
        (self.function)(context, params, inherited_params)
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
}

impl<Context, F, Fut, T, E, Params, InheritedParams> HandlerTypes
    for FromFnAsync<F, Fut, T, E, (Context, Params, InheritedParams)>
where
    Context: IntoContext,
    F: Fn(Context, Params, InheritedParams) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}
#[async_trait::async_trait]
impl<Context, F, Fut, T, E, Params, InheritedParams> Handler<Context>
    for FromFnAsync<F, Fut, T, E, (Context, Params, InheritedParams)>
where
    Context: IntoContext,
    F: Fn(Context, Params, InheritedParams) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context,
            params,
            inherited_params,
            ..
        } = handle_args;
        (self.function)(context, params, inherited_params).await
    }
}

impl<Context, F, T, E, Args> CliBindings<Context> for FromFn<F, T, E, Args>
where
    Context: IntoContext,
    Self: HandlerTypes,
    Self::Params: FromArgMatches + CommandFactory + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self, _: TypeId) -> Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        _: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(
            HandleArgs {
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

impl<Context, F, Fut, T, E, Args> CliBindings<Context> for FromFnAsync<F, Fut, T, E, Args>
where
    Context: IntoContext,
    Self: HandlerTypes,
    Self::Params: FromArgMatches + CommandFactory + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self, _: TypeId) -> Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        _: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(
            HandleArgs {
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
