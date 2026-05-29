//! Generate a TypeScript bindings module for a handler tree:
//! `cargo run --example generate_ts --features ts` (gated by `required-features`).

use rpc_toolkit::ts::handler_bindings;
use rpc_toolkit::{
    Context, Empty, HandlerExt, ParentHandler, from_fn_async, impl_ts_enum, impl_ts_struct,
};
use serde::{Deserialize, Serialize};
// `VisitVariants`-derived enums need these traits in scope (the derive uses method-call syntax).
use visit_rs::{EnumInfo, Visit, VisitFields, VisitVariants};
use yajrc::RpcError;

#[derive(Clone)]
struct Ctx;
impl Context for Ctx {}

#[derive(Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct CreateUser {
    user_name: String,
    age: u32,
    email: Option<String>,
}
impl_ts_struct!(CreateUser);

#[derive(Serialize, Deserialize, VisitVariants)]
#[serde(tag = "status", content = "data")]
enum JobResult {
    Pending,
    Done { id: u64 },
    Failed(String),
}
impl_ts_enum!(JobResult);

async fn create_user(_: Ctx, p: CreateUser) -> Result<u64, RpcError> {
    Ok(p.age as u64)
}
async fn job(_: Ctx, _: Empty) -> Result<JobResult, RpcError> {
    Ok(JobResult::Pending)
}

fn main() {
    let root = ParentHandler::<Ctx, Empty, Empty>::new()
        .subcommand("createUser", from_fn_async(create_user).no_cli())
        .subcommand(
            "jobs",
            ParentHandler::<Ctx, Empty, Empty>::new()
                .subcommand("run", from_fn_async(job).no_cli()),
        );

    match handler_bindings(&root, "Api") {
        Some(module) => print!("{module}"),
        None => eprintln!("root handler opted out of TS"),
    }
}
