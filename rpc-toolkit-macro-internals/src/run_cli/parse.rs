use super::*;
use syn::parse::{Parse, ParseStream};

impl Parse for MakeSeed {
    fn parse(input: ParseStream) -> Result<Self> {
        let matches_ident = input.parse()?;
        let _: token::FatArrow = input.parse()?;
        let body = input.parse()?;
        Ok(MakeSeed {
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
        let command = input.parse()?;
        if !input.is_empty() {
            let _: token::Comma = input.parse()?;
        }
        let mut_app = if !input.is_empty() {
            Some(input.parse()?)
        } else {
            None
        };
        if !input.is_empty() {
            let _: token::Comma = input.parse()?;
        }
        let make_seed = if !input.is_empty() {
            Some(input.parse()?)
        } else {
            None
        };
        if !input.is_empty() {
            let _: token::Comma = input.parse()?;
        }
        let exit_fn = if !input.is_empty() {
            Some(input.parse()?)
        } else {
            None
        };
        Ok(RunCliArgs {
            command,
            mut_app,
            make_seed,
            exit_fn,
        })
    }
}
