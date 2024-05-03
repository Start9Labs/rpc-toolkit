use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Display;

use futures::Future;
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::de::DeserializeOwned;

use crate::util::PhantomData;
use crate::{
    AnyContext, Empty, Handler, HandlerArgs, HandlerArgsFor, HandlerTypes, IntoContext,
    PrintCliResult,
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
            _phantom: PhantomData::new(),
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
impl<F, T, E, Args> PrintCliResult for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    type Context = AnyContext;
    fn print(
        &self,
        _: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}

pub fn from_fn<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args>
where
    FromFn<F, T, E, Args>: HandlerTypes,
{
    FromFn {
        function,
        _phantom: PhantomData::new(),
        blocking: false,
        metadata: OrdMap::new(),
    }
}

pub fn from_fn_blocking<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args>
where
    FromFn<F, T, E, Args>: HandlerTypes,
{
    FromFn {
        function,
        _phantom: PhantomData::new(),
        blocking: true,
        metadata: OrdMap::new(),
    }
}

pub struct FromFnAsync<F, Fut, T, E, Args> {
    _phantom: PhantomData<(Fut, T, E, Args)>,
    function: F,
    metadata: OrdMap<&'static str, Value>,
}
impl<F, Fut, T, E, Args> FromFnAsync<F, Fut, T, E, Args> {
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<F: Clone, Fut, T, E, Args> Clone for FromFnAsync<F, Fut, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
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
    fn print(
        &self,
        _: HandlerArgsFor<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}

pub fn from_fn_async<F, Fut, T, E, Args>(function: F) -> FromFnAsync<F, Fut, T, E, Args>
where
    FromFnAsync<F, Fut, T, E, Args>: HandlerTypes,
{
    FromFnAsync {
        function,
        _phantom: PhantomData::new(),
        metadata: OrdMap::new(),
    }
}

impl<F, T, E, Context, Params, InheritedParams> HandlerTypes
    for FromFn<F, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Result<T, E>
        + Send
        + Sync
        + Clone
        + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: IntoContext,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<F, T, E, Context, Params, InheritedParams> Handler
    for FromFn<F, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Result<T, E>
        + Send
        + Sync
        + Clone
        + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: IntoContext,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    type Context = Context;
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
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
impl<F, Fut, T, E, Context, Params, InheritedParams> HandlerTypes
    for FromFnAsync<F, Fut, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: IntoContext,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<F, Fut, T, E, Context, Params, InheritedParams> Handler
    for FromFnAsync<F, Fut, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: IntoContext,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args).await
    }
    fn metadata(&self, _: VecDeque<&'static str>, _: TypeId) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<F, T, E> HandlerTypes for FromFn<F, T, E, ()>
where
    F: Fn() -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<F, T, E> Handler for FromFn<F, T, E, ()>
where
    F: Fn() -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = AnyContext;
    fn handle_sync(&self, _: HandlerArgsFor<Self::Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)()
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
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
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<F, Fut, T, E> Handler for FromFnAsync<F, Fut, T, E, ()>
where
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = AnyContext;
    async fn handle_async(
        &self,
        _: HandlerArgsFor<Self::Context, Self>,
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
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

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
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
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
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E> Handler for FromFnAsync<F, Fut, T, E, (Context,)>
where
    Context: IntoContext,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
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
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

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
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
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
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E, Params> Handler for FromFnAsync<F, Fut, T, E, (Context, Params)>
where
    Context: IntoContext,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
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
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<Context, F, T, E, Params, InheritedParams> Handler
    for FromFn<F, T, E, (Context, Params, InheritedParams)>
where
    Context: IntoContext,
    F: Fn(Context, Params, InheritedParams) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
            context,
            params,
            inherited_params,
            ..
        } = handle_args;
        (self.function)(context, params, inherited_params)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
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
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E, Params, InheritedParams> Handler
    for FromFnAsync<F, Fut, T, E, (Context, Params, InheritedParams)>
where
    Context: IntoContext,
    F: Fn(Context, Params, InheritedParams) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Context = Context;
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
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
