use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;

use super::*;

impl Parse for RpcHandlerArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let args;
        braced!(args in input);
        let mut command = None;
        let mut ctx = None;
        let mut parent_data = None;
        let mut status_fn = None;
        let mut middleware = Punctuated::new();
        while !args.is_empty() {
            let arg_name: syn::Ident = args.parse()?;
            let _: token::Colon = args.parse()?;
            match arg_name.to_string().as_str() {
                "command" => {
                    command = Some(args.parse()?);
                }
                "context" => {
                    ctx = Some(args.parse()?);
                }
                "parent_data" => {
                    parent_data = Some(args.parse()?);
                }
                "status" => {
                    status_fn = Some(args.parse()?);
                }
                "middleware" => {
                    let middlewares;
                    bracketed!(middlewares in args);
                    middleware = middlewares.parse_terminated(Expr::parse)?;
                }
                _ => return Err(Error::new(arg_name.span(), "unknown argument")),
            }
            if !args.is_empty() {
                let _: token::Comma = args.parse()?;
            }
        }
        Ok(RpcHandlerArgs {
            command: command.expect("`command` is required"),
            ctx: ctx.expect("`context` is required"),
            parent_data,
            status_fn,
            middleware,
        })
    }
}
