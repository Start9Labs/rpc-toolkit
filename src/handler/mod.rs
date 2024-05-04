use std::any::TypeId;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

use clap::{ArgMatches, Command, Parser};
use futures::Future;
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

pub(crate) struct HandleAnyArgs<Inherited> {
    pub(crate) context: AnyContext,
    pub(crate) parent_method: VecDeque<&'static str>,
    pub(crate) method: VecDeque<&'static str>,
    pub(crate) params: Value,
    pub(crate) inherited: Inherited,
}
impl<Inherited: Send + Sync> HandleAnyArgs<Inherited> {
    fn downcast<Context: IntoContext, H>(
        self,
    ) -> Result<HandlerArgsFor<Context, H>, imbl_value::Error>
    where
        H: HandlerTypes<InheritedParams = Inherited>,
        H::Params: DeserializeOwned,
    {
        let Self {
            context,
            parent_method,
            method,
            params,
            inherited,
        } = self;
        Ok(HandlerArgs {
            context: Context::downcast(context).map_err(|_| imbl_value::Error {
                kind: imbl_value::ErrorKind::Deserialization,
                source: serde::ser::Error::custom("context does not match expected"),
            })?,
            parent_method,
            method,
            params: imbl_value::from_value(params.clone())?,
            inherited_params: inherited,
            raw_params: params,
        })
    }
}

#[async_trait::async_trait]
pub(crate) trait HandleAny: Send + Sync {
    type Inherited: Send;
    fn handle_sync(&self, handle_args: HandleAnyArgs<Self::Inherited>) -> Result<Value, RpcError>;
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Self::Inherited>,
    ) -> Result<Value, RpcError>;
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value>;
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>>;
    fn cli(&self) -> Option<&dyn CliBindingsAny<Inherited = Self::Inherited>>;
}
#[async_trait::async_trait]
impl<T: HandleAny> HandleAny for Arc<T> {
    type Inherited = T::Inherited;
    fn handle_sync(&self, handle_args: HandleAnyArgs<Self::Inherited>) -> Result<Value, RpcError> {
        self.deref().handle_sync(handle_args)
    }
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Self::Inherited>,
    ) -> Result<Value, RpcError> {
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
    fn cli(&self) -> Option<&dyn CliBindingsAny<Inherited = Self::Inherited>> {
        self.deref().cli()
    }
}

pub(crate) trait CliBindingsAny {
    type Inherited;
    fn cli_command(&self, ctx_ty: TypeId) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandleAnyArgs<Self::Inherited>,
        result: Value,
    ) -> Result<(), RpcError>;
}

pub trait CliBindings<Context: IntoContext>: HandlerTypes {
    const NO_CLI: bool = false;
    fn cli_command(&self, ctx_ty: TypeId) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

pub trait PrintCliResult<Context: IntoContext>: HandlerTypes {
    fn print(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

#[allow(private_interfaces)]
pub struct DynHandler<Inherited>(Arc<dyn HandleAny<Inherited = Inherited>>);
impl<Inherited> DynHandler<Inherited> {
    pub fn iter<C: IntoContext, H: Handler<C> + CliBindings<C>>(
        h: H,
    ) -> Option<impl IntoIterator<Item = (Option<TypeId>, Self)>> {
        iter_from_ctx_and_handler(ctx, handler)
    }
}
impl<Inherited> Clone for DynHandler<Inherited> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
#[async_trait::async_trait]
impl<Inherited: Send> HandleAny for DynHandler<Inherited> {
    type Inherited = Inherited;
    fn handle_sync(&self, handle_args: HandleAnyArgs<Self::Inherited>) -> Result<Value, RpcError> {
        self.0.handle_sync(handle_args)
    }
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Self::Inherited>,
    ) -> Result<Value, RpcError> {
        self.0.handle_async(handle_args).await
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
    fn cli(&self) -> Option<&dyn CliBindingsAny<Inherited = Self::Inherited>> {
        self.0.cli()
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

pub trait Handler<Context: IntoContext>: HandlerTypes + Clone + Send + Sync + 'static {
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        handle_args
            .context
            .runtime()
            .block_on(self.handle_async(handle_args))
    }
    fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send;
    fn handle_async_with_sync<'a>(
        &'a self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send + 'a {
        async move { self.handle_sync(handle_args) }
    }
    fn handle_async_with_sync_blocking<'a>(
        &'a self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send + 'a {
        async move {
            let s = self.clone();
            handle_args
                .context
                .runtime()
                .spawn_blocking(move || s.handle_sync(handle_args))
                .await
                .unwrap()
        }
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
        Context::type_ids()
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
    _phantom: PhantomData<Context>,
    handler: H,
}
impl<Context, H> AnyHandler<Context, H> {
    pub(crate) fn new(handler: H) -> Self {
        Self {
            _phantom: PhantomData,
            handler,
        }
    }
}
impl<Context, H: std::fmt::Debug> std::fmt::Debug for AnyHandler<Context, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyHandler")
            .field("handler", &self.handler)
            .finish()
    }
}

#[async_trait::async_trait]
impl<Context, H> HandleAny for AnyHandler<Context, H>
where
    Context: IntoContext,
    H: Handler<Context> + CliBindings<Context>,
    H::Params: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    type Inherited = H::InheritedParams;
    fn handle_sync(&self, handle_args: HandleAnyArgs<Self::Inherited>) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
                .handle_sync(handle_args.downcast::<_, H>().map_err(invalid_params)?)?,
        )
        .map_err(internal_error)
    }
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Self::Inherited>,
    ) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
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
        self.handler.metadata(method, ctx_ty)
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method, ctx_ty)
    }
    fn cli(&self) -> Option<&dyn CliBindingsAny<Inherited = Self::Inherited>> {
        if H::NO_CLI {
            None
        } else {
            Some(self)
        }
    }
}

impl<Context, H> CliBindingsAny for AnyHandler<Context, H>
where
    Context: IntoContext,
    H: CliBindings<Context>,
    H::Params: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    type Inherited = H::InheritedParams;
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
        handle_args: HandleAnyArgs<Self::Inherited>,
        result: Value,
    ) -> Result<(), RpcError> {
        self.handler
            .cli_display(
                handle_args.downcast::<_, H>().map_err(invalid_params)?,
                imbl_value::from_value(result).map_err(internal_error)?,
            )
            .map_err(RpcError::from)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
pub struct Empty {}

pub(crate) trait OrEmpty<T> {
    fn from_t(t: T) -> Self;
}
impl<T> OrEmpty<T> for T {
    fn from_t(t: T) -> Self {
        t
    }
}
impl<A, B> OrEmpty<Flat<A, B>> for Empty {
    fn from_t(t: Flat<A, B>) -> Self {
        Empty {}
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
pub enum Never {}
