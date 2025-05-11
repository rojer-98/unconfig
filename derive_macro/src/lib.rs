mod args;

use convert_case::{Case, Casing};
use darling::FromMeta;
use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{parse_macro_input, ItemFn, ItemStruct, Type};

use args::{ConfigArgs, PathArgsConfigurable, PathArgsLogger};

#[proc_macro_attribute]
pub fn implicate(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let args = parse_macro_input!(args as ConfigArgs);

    let prev_fn_body = input.block.stmts.iter().fold(quote! {}, |acc, stmt| {
        quote! { #acc #stmt }
    });
    let prev_attrs = input.attrs.iter().fold(quote! {}, |acc, attr| {
        quote! { #acc #attr }
    });
    let vis = input.vis.to_token_stream();
    let sig = input.sig.to_token_stream();

    let impl_idents = args
        .config_idents
        .into_iter()
        .fold(quote! {}, |acc, ident| {
            let config_macro =
                format_ident!("{}__config__macro", ident.to_string().to_case(Case::Snake));

            if let Some(path) = args.path.as_ref() {
                quote! {
                    #acc

                    impl #path::#config_macro::#ident {
                        #prev_attrs
                        #vis #sig {
                            #prev_fn_body
                        }
                    }
                }
            } else {
                quote! {
                    #acc

                    impl self::#config_macro::#ident {
                        #prev_attrs
                        #vis #sig {
                            #prev_fn_body
                        }
                    }
                }
            }
        });

    impl_idents.into()
}

#[proc_macro_attribute]
pub fn config(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let args = parse_macro_input!(args as ConfigArgs);

    let prev_fn_body = input.block.stmts.iter().fold(quote! {}, |acc, stmt| {
        quote! { #acc #stmt }
    });
    let prev_attrs = input.attrs.iter().fold(quote! {}, |acc, attr| {
        quote! { #acc #attr }
    });
    let vis = input.vis.to_token_stream();
    let sig = input.sig.to_token_stream();

    let config_idents = args
        .config_idents
        .into_iter()
        .fold(quote! {}, |acc, ident| {
            let upper_ident = format_ident!("Upper{ident}");
            let config_ident_name = format_ident!("CONFIG_{}", ident.to_string().to_case(Case::UpperSnake));
            let config_macro = format_ident!("{}__config__macro", ident.to_string().to_case(Case::Snake));

            if let Some(path) = args.path.as_ref() {
               quote! {
                    #acc

                    static #config_ident_name: std::sync::LazyLock<#path::#config_macro::#ident> = std::sync::LazyLock::new(#path::#config_macro::#upper_ident::init);
                }

            } else {
                quote! {
                    #acc

                    static #config_ident_name: std::sync::LazyLock<self::#config_macro::#ident> = std::sync::LazyLock::new(self::#config_macro::#upper_ident::init);
                }
            }
        });

    quote! {
        #config_idents

        #prev_attrs
        #vis #sig {
            #prev_fn_body
        }
    }
    .into()
}

// Config
#[proc_macro_attribute]
pub fn configurable(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let args = parse_macro_input!(args as PathArgsConfigurable);

    let ident = input.ident;
    let upper_ident = format_ident!("Upper{ident}");
    let prev_ident = format_ident!("{}", ident.to_string().to_case(Case::Snake));

    let PathArgsConfigurable {
        rt_cp,
        ct_cp,
        env_cp,
    } = args;

    let init_runtime = if let Some(env_var) = env_cp {
        quote! {
            if let Ok(config_rt) = <#upper_ident as unconfig::Config>::load_env(#env_var, #rt_cp) {
                let merged = config_ct.#prev_ident.merge(config_rt.#prev_ident);

                merged
            } else {
                config_ct.#prev_ident
            }
        }
    } else {
        quote! {
            if let Ok(config_rt) = <#upper_ident as unconfig::Config>::load_path(#rt_cp) {
                let merged = config_ct.#prev_ident.merge(config_rt.#prev_ident);

                merged
            } else {
                config_ct.#prev_ident
            }

        }
    };

    let mut merge_func = quote! {};
    let mut getters_func = quote! {};

    let prev_struct_fields = input.fields.iter().fold(quote! {}, |acc, field| {
        let vis = &field.vis;
        let attrs = field.attrs.iter().fold(quote! {}, |acc, attr| {
            quote! { #acc #attr }
        });
        let ty = &field.ty;
        let colon = field.colon_token.as_ref().unwrap();
        let ident = field.ident.as_ref().unwrap();

        merge_func = quote! {#merge_func #ident: rhs.#ident.or(self.#ident),};

        let get_f = format_ident!("get_{ident}");
        let set_f = format_ident!("set_{ident}");
        getters_func = quote! {
            #getters_func

            pub fn #get_f(&self) -> #ty {
                self.#ident
                    .clone()
                    .unwrap_or_default()
            }

            pub fn #set_f(&mut self, #ident: #ty) {
                self.#ident = Some(#ident);
            }
        };

        quote! { #acc #attrs #vis #ident #colon Option<#ty>,}
    });
    let prev_struct_attrs = input.attrs.iter().fold(quote! {}, |acc, attr| {
        let attr_parsed = attr.meta.to_token_stream().to_string();
        if let Some((_, attr_name)) = attr_parsed.split_once("derive(") {
            let attr_idents = &attr_name[0..attr_name.len() - 1].split(',').fold(
                quote! {},
                |attr_derive_acc, attr_derive_name| {
                    let attr_derive_ident = Type::from_string(attr_derive_name).unwrap();

                    quote! { #attr_derive_acc #attr_derive_ident,}
                },
            );

            quote! { #acc #attr_idents }
        } else {
            quote! { #acc #attr }
        }
    });
    let struct_token = input.struct_token;
    let prev_struct_generics = input.generics;
    let config_macro = format_ident!("{}__config__macro", ident.to_string().to_case(Case::Snake));

    quote! {
        pub(crate) mod #config_macro {
            #[derive(#prev_struct_attrs unconfig::serde::Deserialize)]
            #[serde(crate = "unconfig::serde")]
            pub #struct_token #ident #prev_struct_generics {
                #prev_struct_fields
            }

            impl #ident {
                fn merge(self, rhs: Self) -> Self
                where
                    Self: Sized,
                {
                    Self {
                        #merge_func
                    }
                }

                #getters_func
            }

            #[derive(#prev_struct_attrs unconfig::serde::Deserialize)]
            #[serde(crate = "unconfig::serde")]
            #[serde(rename_all = "snake_case")]
            pub #struct_token #upper_ident #prev_struct_generics {
                #prev_ident: #ident,
            }

            impl #upper_ident {
                pub fn init() -> #ident {
                    // Compile time config
                    let config_ct = <#upper_ident as unconfig::Config>::load_str(include_str!(#ct_cp)).unwrap();

                    // Runtime config
                    #init_runtime
                }
            }
        }
    }.into()
}

// Logger
#[proc_macro_attribute]
pub fn logger(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let args = parse_macro_input!(args as PathArgsLogger);

    let prev_fn_body = input.block.stmts.iter().fold(quote! {}, |acc, stmt| {
        quote! { #acc #stmt }
    });
    let prev_attrs = input.attrs.iter().fold(quote! {}, |acc, attr| {
        quote! { #acc #attr }
    });
    let vis = input.vis.to_token_stream();
    let sig = input.sig.to_token_stream();

    let PathArgsLogger {
        rt_cp,
        ct_cp,
        env_cp,
    } = args;

    let init_runtime = if let Some(env_var) = env_cp {
        quote! {
            if let Ok(ulp_rt) =
                <unconfig::UpperLoggerParams as unconfig::Config>::load_env(#env_var, #rt_cp)
            {
                unconfig::Logger::init(&ulp_rt.merge(ulp_ct))?
            } else {
                unconfig::Logger::init(&ulp_ct)?
            };
        }
    } else {
        quote! {
            if let Ok(ulp_rt) = <unconfig::UpperLoggerParams as unconfig::Config>::load_path(#rt_cp) {
                unconfig::Logger::init(&ulp_rt.merge(ulp_ct))?
            } else {
                unconfig::Logger::init(&ulp_ct)?
            };

        }
    };

    quote! {
        #prev_attrs
        #vis #sig {
            // Compile time logger
            let ulp_ct = <unconfig::UpperLoggerParams as unconfig::Config>::load_str(include_str!(#ct_cp)).unwrap();

            // Runtime logger
            let _logger = #init_runtime

            #prev_fn_body
        }
    }
    .into()
}
