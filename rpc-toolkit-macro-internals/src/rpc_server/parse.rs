use syn::parse::{Parse, ParseStream};

use super::*;

impl Parse for RpcServerArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let command = input.parse()?;
        let _: token::Comma = input.parse()?;
        let ctx = input.parse()?;
        if !input.is_empty() {
            let _: token::Comma = input.parse()?;
        }
        let status_fn = if !input.is_empty() {
            Some(input.parse()?)
        } else {
            None
        };
        Ok(RpcServerArgs {
            command,
            ctx,
            status_fn,
        })
    }
}
