use std::any::TypeId;
use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches, Parser};
use imbl_value::imbl::{OrdMap, OrdSet};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

use crate::context::{AnyContext, IntoContext};
use crate::util::{internal_error, invalid_params, Flat};

pub mod adapters;
pub mod from_fn;
pub mod parent;

pub use adapters::*;
pub use from_fn::*;
pub use parent::*;

pub(crate) struct HandleAnyArgs {
    pub(crate) context: AnyContext,
    pub(crate) parent_method: VecDeque<&'static str>,
    pub(crate) method: VecDeque<&'static str>,
    pub(crate) params: Value,
}
impl HandleAnyArgs {
    fn downcast<Context: IntoContext, H>(
        self,
    ) -> Result<HandlerArgsFor<Context, H>, imbl_value::Error>
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
        Ok(HandlerArgs {
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
pub(crate) trait HandleAny: Send + Sync {
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError>;
    async fn handle_async(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError>;
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value>;
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
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        self.deref().metadata(method, ctx_ty)
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

pub trait CliBindings: HandlerTypes {
    type Context: IntoContext;
    fn cli_command(&self, ctx_ty: TypeId) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

pub trait PrintCliResult: HandlerTypes {
    type Context: IntoContext;
    fn print(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

impl<T> CliBindings for T
where
    T: HandlerTypes,
    T::Params: CommandFactory + FromArgMatches + Serialize,
    T: PrintCliResult,
{
    type Context = T::Context;
    fn cli_command(&self, _: TypeId) -> clap::Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
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
        self.print(
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

pub(crate) trait HandleAnyWithCli: HandleAny + CliBindingsAny {}
impl<T: HandleAny + CliBindingsAny> HandleAnyWithCli for T {}

#[derive(Clone)]
#[allow(private_interfaces)]
pub enum DynHandler {
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
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        match self {
            DynHandler::WithoutCli(h) => h.metadata(method, ctx_ty),
            DynHandler::WithCli(h) => h.metadata(method, ctx_ty),
        }
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        match self {
            DynHandler::WithoutCli(h) => h.method_from_dots(method, ctx_ty),
            DynHandler::WithCli(h) => h.method_from_dots(method, ctx_ty),
        }
    }
}

#[allow(type_alias_bounds)]
pub type HandlerArgsFor<Context: IntoContext, H: HandlerTypes + ?Sized> =
    HandlerArgs<Context, H::Params, H::InheritedParams>;

#[derive(Debug, Clone)]
pub struct HandlerArgs<
    Context: IntoContext,
    Params: Send + Sync = Empty,
    InheritedParams: Send + Sync = Empty,
> {
    pub context: Context,
    pub parent_method: VecDeque<&'static str>,
    pub method: VecDeque<&'static str>,
    pub params: Params,
    pub inherited_params: InheritedParams,
    pub raw_params: Value,
}

pub trait HandlerTypes {
    type Params: Send + Sync;
    type InheritedParams: Send + Sync;
    type Ok: Send + Sync;
    type Err: Send + Sync;
}

#[async_trait::async_trait]
pub trait Handler: HandlerTypes + Clone + Send + Sync + 'static {
    type Context: IntoContext;
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        handle_args
            .context
            .runtime()
            .block_on(self.handle_async(handle_args))
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err>;
    async fn handle_async_with_sync(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handle_sync(handle_args)
    }
    async fn handle_async_with_sync_blocking(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let s = self.clone();
        handle_args
            .context
            .runtime()
            .spawn_blocking(move || s.handle_sync(handle_args))
            .await
            .unwrap()
    }
    #[allow(unused_variables)]
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        OrdMap::new()
    }
    fn contexts(&self) -> Option<OrdSet<TypeId>> {
        Self::Context::type_ids()
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

pub(crate) struct AnyHandler<H>(H);
impl<H> AnyHandler<H> {
    pub(crate) fn new(handler: H) -> Self {
        Self(handler)
    }
}
impl<H: std::fmt::Debug> std::fmt::Debug for AnyHandler<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AnyHandler").field(&self.0).finish()
    }
}

#[async_trait::async_trait]
impl<H: Handler> HandleAny for AnyHandler<H>
where
    H::Params: DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize,
    RpcError: From<H::Err>,
{
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .0
                .handle_sync(handle_args.downcast::<_, H>().map_err(invalid_params)?)?,
        )
        .map_err(internal_error)
    }
    async fn handle_async(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .0
                .handle_async(handle_args.downcast::<_, H>().map_err(invalid_params)?)
                .await?,
        )
        .map_err(internal_error)
    }
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        self.0.metadata(method, ctx_ty)
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.0.method_from_dots(method, ctx_ty)
    }
}

impl<H: CliBindings> CliBindingsAny for AnyHandler<H>
where
    H: CliBindings,
    H::Params: DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        self.0.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.0.cli_parse(matches, ctx_ty)
    }
    fn cli_display(&self, handle_args: HandleAnyArgs, result: Value) -> Result<(), RpcError> {
        self.0
            .cli_display(
                handle_args.downcast::<_, H>().map_err(invalid_params)?,
                imbl_value::from_value(result).map_err(internal_error)?,
            )
            .map_err(RpcError::from)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
pub struct Empty {}

pub(crate) trait OrEmpty<T> {}
impl<T> OrEmpty<T> for T {}
impl<A, B> OrEmpty<Flat<A, B>> for Empty {}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
pub enum Never {}
