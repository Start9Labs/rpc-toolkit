use std::fmt::{Debug, Display};

use futures::future::{BoxFuture, FusedFuture};
use futures::stream::FusedStream;
use futures::{Future, FutureExt, Stream, StreamExt};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::ser::Error;
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

pub fn extract<T: DeserializeOwned>(value: &Value) -> Result<T, RpcError> {
    imbl_value::from_value(value.clone()).map_err(|e| RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_PARAMS_ERROR
    })
}

pub fn combine(v1: Value, v2: Value) -> Result<Value, imbl_value::Error> {
    let (Value::Object(mut v1), Value::Object(v2)) = (v1, v2) else {
        return Err(imbl_value::Error {
            kind: imbl_value::ErrorKind::Serialization,
            source: serde_json::Error::custom("params must be object"),
        });
    };
    for (key, value) in v2 {
        if v1.insert(key.clone(), value).is_some() {
            return Err(imbl_value::Error {
                kind: imbl_value::ErrorKind::Serialization,
                source: serde_json::Error::custom(lazy_format::lazy_format!(
                    "duplicate key: {key}"
                )),
            });
        }
    }
    Ok(Value::Object(v1))
}

pub fn invalid_params(e: imbl_value::Error) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_PARAMS_ERROR
    }
}

pub fn invalid_request(e: imbl_value::Error) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_REQUEST_ERROR
    }
}

pub fn parse_error(e: impl Display) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::PARSE_ERROR
    }
}

pub fn internal_error(e: impl Display) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    }
}

pub struct Flat<A, B>(pub A, pub B);
impl<'de, A, B> Deserialize<'de> for Flat<A, B>
where
    A: DeserializeOwned,
    B: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = Value::deserialize(deserializer)?;
        let a = imbl_value::from_value(v.clone()).map_err(serde::de::Error::custom)?;
        let b = imbl_value::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Flat(a, b))
    }
}
impl<A, B> Serialize for Flat<A, B>
where
    A: Serialize,
    B: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct FlatStruct<'a, A, B> {
            #[serde(flatten)]
            a: &'a A,
            #[serde(flatten)]
            b: &'a B,
        }
        FlatStruct {
            a: &self.0,
            b: &self.1,
        }
        .serialize(serializer)
    }
}

pub fn poll_select_all<'a, T>(
    futs: &mut Vec<BoxFuture<'a, T>>,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<T> {
    let item = futs
        .iter_mut()
        .enumerate()
        .find_map(|(i, f)| match f.poll_unpin(cx) {
            std::task::Poll::Pending => None,
            std::task::Poll::Ready(e) => Some((i, e)),
        });
    match item {
        Some((idx, res)) => {
            drop(futs.swap_remove(idx));
            std::task::Poll::Ready(res)
        }
        None => std::task::Poll::Pending,
    }
}

pub struct JobRunner<'a, T> {
    closed: bool,
    running: Vec<BoxFuture<'a, T>>,
}
impl<'a, T> JobRunner<'a, T> {
    pub fn new() -> Self {
        JobRunner {
            closed: false,
            running: Vec::new(),
        }
    }
    pub async fn next_result<
        Src: Stream<Item = Fut> + Unpin,
        Fut: Future<Output = T> + Send + 'a,
    >(
        &mut self,
        job_source: &mut Src,
    ) -> Option<T> {
        loop {
            tokio::select! {
                job = job_source.next() => {
                    if let Some(job) = job {
                        self.running.push(job.boxed());
                    } else {
                        self.closed = true;
                        if self.running.is_empty() {
                            return None;
                        }
                    }
                }
                res = self.next() => {
                    return res;
                }
            }
        }
    }
}
impl<'a, T> Stream for JobRunner<'a, T> {
    type Item = T;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match poll_select_all(&mut self.running, cx) {
            std::task::Poll::Pending if self.closed && self.running.is_empty() => {
                std::task::Poll::Ready(None)
            }
            a => a.map(Some),
        }
    }
}

#[pin_project::pin_project]
pub struct StreamUntil<S, F> {
    #[pin]
    stream: S,
    #[pin]
    until: F,
    done: bool,
}
impl<S, F> StreamUntil<S, F> {
    pub fn new(stream: S, until: F) -> Self {
        Self {
            stream,
            until,
            done: false,
        }
    }
}
impl<S, F> Stream for StreamUntil<S, F>
where
    S: Stream,
    F: Future,
{
    type Item = S::Item;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        *this.done = *this.done || this.until.poll(cx).is_ready();
        if *this.done {
            std::task::Poll::Ready(None)
        } else {
            this.stream.poll_next(cx)
        }
    }
}
impl<S, F> FusedStream for StreamUntil<S, F>
where
    S: FusedStream,
    F: FusedFuture,
{
    fn is_terminated(&self) -> bool {
        self.done || self.stream.is_terminated() || self.until.is_terminated()
    }
}

pub struct PhantomData<T>(std::marker::PhantomData<T>);
impl<T> PhantomData<T> {
    pub fn new() -> Self {
        PhantomData(std::marker::PhantomData)
    }
}
impl<T> Clone for PhantomData<T> {
    fn clone(&self) -> Self {
        PhantomData::new()
    }
}
impl<T> Debug for PhantomData<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
unsafe impl<T> Send for PhantomData<T> {}
unsafe impl<T> Sync for PhantomData<T> {}
