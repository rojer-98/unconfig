use std::{env::var, path::Path};

use quote::ToTokens;
use syn::{
    parse::{Parse, ParseStream, Result},
    punctuated::Punctuated,
    Ident, Lit, Path as SynPath, Token,
};

mod kw {
    syn::custom_keyword!(path);
    syn::custom_keyword!(parse);
}

pub struct ConfigArgs {
    pub config_idents: Vec<Ident>,
    pub path: Option<SynPath>,
}

impl Parse for ConfigArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let path = input
            .parse::<kw::path>()
            .and_then(|_| input.parse::<Token![=]>())
            .and_then(|_| {
                let path: SynPath = input.parse()?;

                Ok(path)
            })
            .ok();
        let _ = input
            .parse::<Token![,]>()
            .and_then(|_| input.parse::<kw::parse>())
            .and_then(|_| input.parse::<Token![=]>());
        let config_idents = Punctuated::<Ident, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();

        Ok(Self {
            config_idents,
            path,
        })
    }
}

pub struct PathArgsLogger {
    pub rt_cp: proc_macro2::TokenStream,
    pub ct_cp: proc_macro2::TokenStream,
    pub env_cp: Option<proc_macro2::TokenStream>,
}

pub struct PathArgsConfigurable {
    pub rt_cp: proc_macro2::TokenStream,
    pub ct_cp: proc_macro2::TokenStream,
    pub env_cp: Option<proc_macro2::TokenStream>,
}

// Replace slashes
impl Parse for PathArgsConfigurable {
    fn parse(input: ParseStream) -> Result<Self> {
        let root_dir = var("CARGO_MANIFEST_DIR").unwrap().to_string();
        let (cp, ep) = parse(input);
        let parsed = cp.unwrap_or("config.yml".to_string());

        let cp = Path::new(&root_dir).join(parsed);
        let (rt_cp, ct_cp) = if cp.exists() {
            let cp = cp.to_str().into_token_stream();
            (cp.clone(), cp)
        } else {
            let ct_cp = Path::new(&root_dir)
                .join("config.yml")
                .to_str()
                .into_token_stream();
            let rt_cp = cp.to_str().into_token_stream();

            (rt_cp, ct_cp)
        };
        let env_cp = ep.map(ToTokens::into_token_stream);

        Ok(Self {
            ct_cp,
            rt_cp,
            env_cp,
        })
    }
}

impl Parse for PathArgsLogger {
    fn parse(input: ParseStream) -> Result<Self> {
        let root_dir = var("CARGO_MANIFEST_DIR").unwrap().to_string();
        let (cp, ep) = parse(input);
        let parsed = cp.unwrap_or("logger.yml".to_string());

        let cp = Path::new(&root_dir).join(parsed);
        let (rt_cp, ct_cp) = if cp.exists() {
            let cp = cp.to_str().into_token_stream();
            (cp.clone(), cp)
        } else {
            let ct_cp = Path::new(&root_dir)
                .join("logger.yml")
                .to_str()
                .into_token_stream();
            let rt_cp = cp.to_str().into_token_stream();

            (rt_cp, ct_cp)
        };
        let env_cp = ep.map(ToTokens::into_token_stream);

        Ok(Self {
            ct_cp,
            rt_cp,
            env_cp,
        })
    }
}

// Return compile and runtime path
fn parse(input: ParseStream) -> (Option<String>, Option<String>) {
    input
        .parse::<Lit>()
        .ok()
        .and_then(|config_path| {
            if let Lit::Str(cp) = config_path {
                Some(cp.value())
            } else {
                None
            }
        })
        .filter(|parsed| parsed.contains("${"))
        .and_then(|parsed| {
            let last_curly = parsed.find('}')?;
            let env_var_s = parsed[2..last_curly].to_string();

            match var(&env_var_s) {
                Ok(value) => Some((Some(value), Some(env_var_s))),
                Err(_) if env_var_s.contains(':') => match env_var_s.split_once(':') {
                    Some((varname, tail)) => match var(varname) {
                        Ok(value) => Some((Some(value), Some(varname.to_string()))),
                        _ => Some((Some(tail.to_string()), Some(varname.to_string()))),
                    },
                    _ => Some((Some(parsed), None)),
                },
                _ => Some((None, Some(env_var_s))),
            }
        })
        .unwrap_or((None, None))
}
