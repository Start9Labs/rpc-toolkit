use std::fmt::Display;
use std::io::Stdin;
use std::marker::PhantomData;
use std::str::FromStr;

use clap::ArgMatches;
use hyper::Method;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use yajrc::{GenericRpcMethod, Id, RpcError, RpcRequest, RpcResponse};

use crate::Context;

pub mod prelude {
    pub use std::borrow::Cow;
    pub use std::marker::PhantomData;

    pub use clap::{App, AppSettings, Arg, ArgMatches};
    pub use hyper::http::request::Parts as RequestParts;
    pub use hyper::http::response::Parts as ResponseParts;
    pub use serde::{Deserialize, Serialize};
    pub use serde_json::{from_value, to_value, Value};
    pub use tokio::runtime::Runtime;
    pub use tokio::task::spawn_blocking;
    pub use yajrc::{self, RpcError};

    pub use super::{
        call_remote, default_arg_parser, default_display, default_stdin_parser, make_phantom,
        match_types,
    };
    pub use crate::Context;
}

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("JSON Error: {0}")]
    JSON(#[from] serde_json::Error),
    #[cfg(feature = "cbor")]
    #[error("CBOR Error: {0}")]
    CBOR(#[from] serde_cbor::Error),
    #[error("HTTP Error: {0}")]
    HTTP(#[from] reqwest::Error),
    #[error("Missing Content-Type")]
    MissingContentType,
}

pub fn make_phantom<T>(_actual: T) -> PhantomData<T> {
    PhantomData
}

pub fn match_types<T>(_: &T, _: &T) {}

pub async fn call_remote<Ctx: Context, Params: Serialize, Res: for<'de> Deserialize<'de>>(
    ctx: Ctx,
    method: &str,
    params: Params,
    _return_ty: PhantomData<Res>,
) -> Result<RpcResponse<GenericRpcMethod<&str, Params, Res>>, RequestError> {
    let rpc_req: RpcRequest<GenericRpcMethod<&str, Params, Res>> = RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::new(method),
        params,
    };
    let mut req = ctx.client().request(Method::POST, ctx.url());
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
    Ok(
        match res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
        {
            Some("application/json") => serde_json::from_slice(&*res.bytes().await?)?,
            #[cfg(feature = "cbor")]
            Some("application/cbor") => serde_cbor::from_slice(&*res.bytes().await?)?,
            _ => return Err(RequestError::MissingContentType),
        },
    )
}

pub fn default_arg_parser<T: FromStr<Err = E>, E: Display>(
    arg: &str,
    _: &ArgMatches<'_>,
) -> Result<T, RpcError> {
    arg.parse().map_err(|e| RpcError {
        data: Some(format!("{}", e).into()),
        ..yajrc::INVALID_PARAMS_ERROR
    })
}

pub fn default_stdin_parser<T: FromStr<Err = E>, E: Display>(
    stdin: &mut Stdin,
    _: &ArgMatches<'_>,
) -> Result<T, RpcError> {
    let mut s = String::new();
    stdin.read_line(&mut s).map_err(|e| RpcError {
        data: Some(format!("{}", e).into()),
        ..yajrc::INVALID_PARAMS_ERROR
    })?;
    if let Some(s) = s.strip_suffix("\n") {
        s
    } else {
        &s
    }
    .parse()
    .map_err(|e| RpcError {
        data: Some(format!("{}", e).into()),
        ..yajrc::INVALID_PARAMS_ERROR
    })
}

pub fn default_display<T: Display>(t: T, _: &ArgMatches<'_>) {
    println!("{}", t)
}
