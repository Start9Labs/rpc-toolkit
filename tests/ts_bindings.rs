#![cfg(feature = "ts")]
#![allow(dead_code)]

use std::collections::BTreeMap;

use rpc_toolkit::ts::{TS, TSVisitor, handler_bindings};
use rpc_toolkit::{
    Context, Empty, HandlerExt, ParentHandler, from_fn, from_fn_async, impl_ts_enum, impl_ts_struct,
};
use serde::{Deserialize, Serialize};
use visit_rs::{EnumInfo, Visit, VisitFields, VisitVariants};
use yajrc::RpcError;

fn render<T: TS>() -> String {
    let mut v = TSVisitor::new();
    v.append_type::<T>();
    v.ts
}

#[derive(Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct Profile {
    user_name: String,
    age: u32,
    big_id: u64,
    nickname: Option<String>,
    #[serde(skip)]
    secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    maybe: Option<bool>,
    tags: Vec<String>,
}
impl_ts_struct!(Profile);

#[test]
fn struct_rename_option_skip_primitives() {
    // camelCase rename, u64 -> number (not bigint), Option -> T|null, skip dropped,
    // skip_serializing_if -> optional key, Vec -> (T)[].
    assert_eq!(
        render::<Profile>(),
        "{userName:string;age:number;bigId:number;nickname:string|null;maybe?:boolean|null;tags:(string)[];}"
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
enum External {
    Unit,
    NewType(u32),
    Tuple(u32, String),
    Struct { x: bool },
}
impl_ts_enum!(External);

#[test]
fn externally_tagged_enum() {
    // serde default: unit -> "Unit"; data variants -> {"Variant": payload}.
    assert_eq!(
        render::<External>(),
        r#"|"Unit"|{"NewType":number}|{"Tuple":[number,string,]}|{"Struct":{x:boolean;}}"#
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
#[serde(tag = "type")]
enum Internal {
    A { x: u32 },
    B { y: String },
}
impl_ts_enum!(Internal);

#[test]
fn internally_tagged_enum() {
    assert_eq!(
        render::<Internal>(),
        r#"|{"type":"A"}&{x:number;}|{"type":"B"}&{y:string;}"#
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Unit,
    Data(u32),
    Struct { z: bool },
}
impl_ts_enum!(Adjacent);

#[test]
fn adjacently_tagged_enum() {
    // unit -> {"t":"Unit"} (no content); data -> {"t":..;"c":payload}.
    assert_eq!(
        render::<Adjacent>(),
        r#"|{"t":"Unit"}|{"t":"Data";"c":number}|{"t":"Struct";"c":{z:boolean;}}"#
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
#[serde(untagged)]
enum Untagged {
    A(u32),
    B(String),
}
impl_ts_enum!(Untagged);

#[test]
fn untagged_enum() {
    assert_eq!(render::<Untagged>(), "|number|string");
}

#[derive(Serialize, Deserialize, VisitFields)]
struct Maps {
    by_str: BTreeMap<String, u32>,
    by_num: BTreeMap<u32, String>,
}
impl_ts_struct!(Maps);

#[test]
fn map_keys_are_strings() {
    // JSON object keys are always strings, including for BTreeMap<u32, _>.
    assert_eq!(
        render::<Maps>(),
        "{by_str:{[key:string]:number};by_num:{[key:string]:string};}"
    );
}

#[test]
fn option_of_collection() {
    assert_eq!(render::<Option<Vec<u32>>>(), "(number)[]|null");
}

#[derive(Serialize, Deserialize, VisitFields)]
struct Inner {
    a: u32,
}
impl_ts_struct!(Inner { define: "Inner" });

#[derive(Serialize, Deserialize, VisitFields)]
struct Outer {
    inner: Inner,
    list: Vec<Inner>,
}
impl_ts_struct!(Outer);

#[test]
fn named_definitions() {
    let mut v = TSVisitor::new();
    v.append_type::<Outer>();
    assert_eq!(v.ts, "{inner:Inner;list:(Inner)[];}");
    assert_eq!(
        v.definitions.get("Inner").map(String::as_str),
        Some("{a:number;}")
    );

    let module = v.into_module("Root");
    assert!(module.contains("export type Inner = {a:number;};"));
    assert!(module.contains("export type Root = {inner:Inner;list:(Inner)[];};"));
    // helper types are included
    assert!(module.contains("export type RpcParamType"));
}

#[derive(Clone)]
struct Ctx;
impl Context for Ctx {}

async fn echo(_: Ctx, p: Profile) -> Result<String, RpcError> {
    Ok(p.user_name)
}
fn ping(_: Ctx, _: Empty) -> Result<bool, RpcError> {
    Ok(true)
}

#[test]
fn handler_tree_bindings() {
    let root = ParentHandler::<Ctx, Empty, Empty>::new()
        .subcommand("echo", from_fn_async(echo).no_cli())
        .subcommand("ping", from_fn(ping).no_ts());

    let module = handler_bindings(&root, "Api").expect("root has ts");
    // echo is fully typed; ping opted out but must still be a well-formed leaf node.
    assert!(module.contains("\"echo\":{_PARAMS:"));
    assert!(module.contains("\"ping\":{_PARAMS:unknown;_RETURN:unknown}"));
    assert!(module.contains("export type Api ="));
}
