use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Display;
use std::marker::PhantomData;
use std::sync::Arc;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches};
use futures::Future;
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::marker::LeafHandler;
use crate::{
    intersect_type_ids, iter_from_ctx_and_handler, AnyContext, AnyHandler, CliBindings, DynHandler,
    HandleArgs, Handler, HandlerTypes, IntoContext, IntoHandlers, NoCli, NoParams, PrintCliResult,
};

pub struct FromFn<F, T, E, Args> {
    _phantom: PhantomData<(T, E, Args)>,
    function: F,
    blocking: bool,
    metadata: OrdMap<&'static str, Value>,
}
impl<F, T, E, Args> FromFn<F, T, E, Args> {
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<F: Clone, T, E, Args> Clone for FromFn<F, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            function: self.function.clone(),
            blocking: self.blocking,
            metadata: self.metadata.clone(),
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
impl<F, T, E, Args> LeafHandler for FromFn<F, T, E, Args> {}
impl<F, T, E, Args> PrintCliResult for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    type Context = AnyContext;
    fn print(&self, _: HandleArgs<Self::Context, Self>, result: Self::Ok) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}
impl<F, T, E, Args> IntoHandlers for FromFn<F, T, E, Args>
where
    Self: HandlerTypes + Handler + CliBindings,
    <Self as HandlerTypes>::Params: DeserializeOwned,
    <Self as HandlerTypes>::InheritedParams: DeserializeOwned,
    <Self as HandlerTypes>::Ok: Serialize + DeserializeOwned,
    RpcError: From<<Self as HandlerTypes>::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, crate::DynHandler)> {
        iter_from_ctx_and_handler(
            intersect_type_ids(self.contexts(), <Self as CliBindings>::Context::type_ids()),
            DynHandler::WithCli(Arc::new(AnyHandler::new(self))),
        )
    }
}
impl<F, T, E, Args> IntoHandlers for NoCli<FromFn<F, T, E, Args>>
where
    Self: HandlerTypes + Handler,
    <Self as HandlerTypes>::Params: DeserializeOwned,
    <Self as HandlerTypes>::InheritedParams: DeserializeOwned,
    <Self as HandlerTypes>::Ok: Serialize + DeserializeOwned,
    RpcError: From<<Self as HandlerTypes>::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, crate::DynHandler)> {
        iter_from_ctx_and_handler(
            self.contexts(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(self))),
        )
    }
}

pub fn from_fn<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args> {
    FromFn {
        function,
        _phantom: PhantomData,
        blocking: false,
        metadata: OrdMap::new(),
    }
}

pub fn from_fn_blocking<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args> {
    FromFn {
        function,
        _phantom: PhantomData,
        blocking: true,
        metadata: OrdMap::new(),
    }
}

pub struct FromFnAsync<F, Fut, T, E, Args> {
    _phantom: PhantomData<(Fut, T, E, Args)>,
    function: F,
    metadata: OrdMap<&'static str, Value>,
}
impl<F: Clone, Fut, T, E, Args> Clone for FromFnAsync<F, Fut, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            function: self.function.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
impl<F, Fut, T, E, Args> std::fmt::Debug for FromFnAsync<F, Fut, T, E, Args> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromFnAsync").finish()
    }
}
impl<F, Fut, T, E, Args> PrintCliResult for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    type Context = AnyContext;
    fn print(&self, _: HandleArgs<Self::Context, Self>, result: Self::Ok) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}
impl<F, Fut, T, E, Args> IntoHandlers for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes + Handler + CliBindings,
    <Self as HandlerTypes>::Params: DeserializeOwned,
    <Self as HandlerTypes>::InheritedParams: DeserializeOwned,
    <Self as HandlerTypes>::Ok: Serialize + DeserializeOwned,
    RpcError: From<<Self as HandlerTypes>::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, crate::DynHandler)> {
        iter_from_ctx_and_handler(
            intersect_type_ids(self.contexts(), <Self as CliBindings>::Context::type_ids()),
            DynHandler::WithCli(Arc::new(AnyHandler::new(self))),
        )
    }
}
impl<F, Fut, T, E, Args> IntoHandlers for NoCli<FromFnAsync<F, Fut, T, E, Args>>
where
    Self: HandlerTypes + Handler,
    <Self as HandlerTypes>::Params: DeserializeOwned,
    <Self as HandlerTypes>::InheritedParams: DeserializeOwned,
    <Self as HandlerTypes>::Ok: Serialize + DeserializeOwned,
    RpcError: From<<Self as HandlerTypes>::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, crate::DynHandler)> {
        iter_from_ctx_and_handler(
            self.contexts(),
            DynHandler::WithoutCli(Arc::new(AnyHandler::new(self))),
        )
    }
}

pub fn from_fn_async<F, Fut, T, E, Args>(function: F) -> FromFnAsync<F, Fut, T, E, Args> {
    FromFnAsync {
        function,
        _phantom: PhantomData,
        metadata: OrdMap::new(),
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
impl<F, T, E> Handler for FromFn<F, T, E, ()>
where
    F: Fn() -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = AnyContext;
    fn handle_sync(&self, _: HandleArgs<Self::Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)()
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
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
impl<F, Fut, T, E> Handler for FromFnAsync<F, Fut, T, E, ()>
where
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = AnyContext;
    async fn handle_async(
        &self,
        _: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)().await
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
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
impl<Context, F, T, E> Handler for FromFn<F, T, E, (Context,)>
where
    Context: IntoContext,
    F: Fn(Context) -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    fn handle_sync(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context)
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
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
impl<Context, F, Fut, T, E> Handler for FromFnAsync<F, Fut, T, E, (Context,)>
where
    Context: IntoContext,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context).await
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
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
impl<Context, F, T, E, Params> Handler for FromFn<F, T, E, (Context, Params)>
where
    Context: IntoContext,
    F: Fn(Context, Params) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    fn handle_sync(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params)
    }
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
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
impl<Context, F, Fut, T, E, Params> Handler for FromFnAsync<F, Fut, T, E, (Context, Params)>
where
    Context: IntoContext,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params).await
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
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
impl<Context, F, T, E, Params, InheritedParams> Handler
    for FromFn<F, T, E, (Context, Params, InheritedParams)>
where
    Context: IntoContext,
    F: Fn(Context, Params, InheritedParams) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    fn handle_sync(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
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
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
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
impl<Context, F, Fut, T, E, Params, InheritedParams> Handler
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
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context,
            params,
            inherited_params,
            ..
        } = handle_args;
        (self.function)(context, params, inherited_params).await
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<F, T, E, Args> CliBindings for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    Self::Params: FromArgMatches + CommandFactory + Serialize,
    Self: PrintCliResult,
{
    type Context = <Self as PrintCliResult>::Context;
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
        }: HandleArgs<Self::Context, Self>,
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

impl<F, Fut, T, E, Args> CliBindings for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Self::Params: FromArgMatches + CommandFactory + Serialize,
    Self: PrintCliResult,
{
    type Context = <Self as PrintCliResult>::Context;
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
        }: HandleArgs<Self::Context, Self>,
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
