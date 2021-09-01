use syn::parse::{Parse, ParseStream};

use super::*;

impl Parse for MakeCtx {
    fn parse(input: ParseStream) -> Result<Self> {
        let matches_ident = input.parse()?;
        let _: token::FatArrow = input.parse()?;
        let body = input.parse()?;
        Ok(MakeCtx {
            matches_ident,
            body,
        })
    }
}

impl Parse for MutApp {
    fn parse(input: ParseStream) -> Result<Self> {
        let app_ident = input.parse()?;
        let _: token::FatArrow = input.parse()?;
        let body = input.parse()?;
        Ok(MutApp { app_ident, body })
    }
}

impl Parse for RunCliArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let args;
        braced!(args in input);
        let mut command = None;
        let mut mut_app = None;
        let mut make_ctx = None;
        let mut parent_data = None;
        let mut exit_fn = None;
        while !args.is_empty() {
            let arg_name: syn::Ident = args.parse()?;
            let _: token::Colon = args.parse()?;
            match arg_name.to_string().as_str() {
                "command" => {
                    command = Some(args.parse()?);
                }
                "app" => {
                    mut_app = Some(args.parse()?);
                }
                "context" => {
                    make_ctx = Some(args.parse()?);
                }
                "parent_data" => {
                    parent_data = Some(args.parse()?);
                }
                "exit" => {
                    exit_fn = Some(args.parse()?);
                }
                _ => return Err(Error::new(arg_name.span(), "unknown argument")),
            }
            if !args.is_empty() {
                let _: token::Comma = args.parse()?;
            }
        }
        Ok(RunCliArgs {
            command: command.expect("`command` is required"),
            mut_app,
            make_ctx,
            parent_data,
            exit_fn,
        })
    }
}
