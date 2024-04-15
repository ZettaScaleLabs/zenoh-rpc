/*********************************************************************************
* Copyright (c) 2022 ZettaScale Technology
*
* This program and the accompanying materials are made available under the
* terms of the Eclipse Public License 2.0 which is available at
* http://www.eclipse.org/legal/epl-2.0, or the Apache Software License 2.0
* which is available at https://www.apache.org/licenses/LICENSE-2.0.
*
* SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
* Contributors:
*   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
*********************************************************************************/
#![allow(clippy::upper_case_acronyms)]
#![recursion_limit = "512"]

extern crate base64;
extern crate darling;
extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate serde;
extern crate syn;

use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    spanned::Spanned,
    token::Comma,
    Attribute, AttributeArgs, FnArg, Ident, Pat, PatType, Receiver, ReturnType, Token, Type,
    Visibility,
};
use syn_serde::json;

macro_rules! extend_errors {
    ($errors: ident, $e: expr) => {
        match $errors {
            Ok(_) => $errors = Err($e),
            Err(ref mut errors) => errors.extend($e),
        }
    };
}

#[derive(Debug, FromMeta)]
struct ServiceMacroArgs {
    timeout_s: u16,
}

struct Service {
    _attrs: Vec<Attribute>,
    vis: Visibility,
    ident: Ident,
    methods: Vec<RPCMethod>,
}

impl Parse for Service {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        input.parse::<Token![trait]>()?;
        let ident: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut methods = Vec::<RPCMethod>::new();
        while !content.is_empty() {
            methods.push(content.parse()?);
        }

        Ok(Self {
            _attrs,
            vis,
            ident,
            methods,
        })
    }
}

struct RPCMethod {
    attrs: Vec<Attribute>,
    ident: Ident,
    receiver: Receiver,
    args: Vec<PatType>,
    output: ReturnType,
}

impl Parse for RPCMethod {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        input.parse::<Token![async]>()?;
        input.parse::<Token![fn]>()?;
        let ident = input.parse()?;
        let content;
        let mut recv: Option<Receiver> = None;
        parenthesized!(content in input);
        let mut args = Vec::new();
        let mut errors = Ok(());
        for arg in content.parse_terminated::<FnArg, Comma>(FnArg::parse)? {
            match arg {
                FnArg::Typed(captured) if matches!(&*captured.pat, Pat::Ident(_)) => {
                    args.push(captured);
                }
                FnArg::Typed(captured) => {
                    extend_errors!(
                        errors,
                        syn::Error::new(captured.pat.span(), "patterns aren't allowed in RPC args")
                    );
                }
                FnArg::Receiver(mut receiver) => {
                    //We force no mut in the receiver, received cannot be `self`
                    receiver.mutability = None;
                    if receiver.reference.is_none() {
                        extend_errors!(
                            errors,
                            syn::Error::new(receiver.span(), "method args take ownership on self")
                        );
                    }
                    recv = Some(receiver)
                }
            }
        }
        match recv {
            None => extend_errors!(
                errors,
                syn::Error::new(
                    recv.span(),
                    "Missing any receiver in method declaration, please add one!"
                )
            ),
            Some(_) => (),
        }

        errors?;
        let output = input.parse()?;
        input.parse::<Token![;]>()?;
        let receiver = recv.unwrap();
        Ok(Self {
            attrs,
            ident,
            receiver,
            args,
            output,
        })
    }
}

#[proc_macro_derive(Ast)]
pub fn derive_ast(item: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(item).unwrap();
    let exp: syn::File = syn::parse_quote! {
        #ast
    };

    println!("{}", json::to_string_pretty(&exp));
    TokenStream::new()
}

/// This macros parses the service trait and converts it
/// into the one expected by Zenoh-RPC, it creates Request and Response
/// structures, a `Server` implementation, and, a Client implementation
///
#[proc_macro_attribute]
pub fn service(attr: TokenStream, input: TokenStream) -> TokenStream {
    //parsing the trait body
    let Service {
        _attrs: _,
        ref vis,
        ref ident,
        ref methods,
    } = parse_macro_input!(input as Service);

    //parsing the attributes to the macro
    let attr_args = parse_macro_input!(attr as AttributeArgs);
    let macro_args = match ServiceMacroArgs::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    //converts the functions names from snake_case to CamelCase
    let method_idents: Vec<_> = methods.iter().map(|m| &m.ident).cloned().collect();

    let server_ident = &format_ident!("{}Server", ident);

    let ts: TokenStream = ServiceGenerator {
        server_ident,
        service_ident: ident,
        method_idents: &method_idents,
        methods,
        vis,
        tout: &macro_args.timeout_s,
    }
    .into_token_stream()
    .into();
    ts
}

struct ServiceGenerator<'a> {
    service_ident: &'a Ident,
    server_ident: &'a Ident,
    method_idents: &'a [Ident],
    methods: &'a [RPCMethod],
    vis: &'a Visibility,
    tout: &'a u16,
}

impl<'a> ServiceGenerator<'a> {
    // parses the server trait
    fn trait_service(&self) -> TokenStream2 {
        let &Self {
            methods,
            vis,
            service_ident,
            ..
        } = self;

        let fns = methods.iter().map(
            |method| {

                let ident = &method.ident;
                let mut receiver = method.receiver.clone();
                receiver.mutability=None;


                let attrs = &method.attrs;
                let request_ident = format_ident!("{}Request", snake_to_camel(&ident.to_string()));
                let response_ident = format_ident!("{}Response", snake_to_camel(&ident.to_string()));
                quote! {

                    #(#attrs)*
                    async fn #ident(#receiver, request: zrpc::prelude::Request<#request_ident>) ->  std::result::Result<zrpc::prelude::Response<#response_ident>, zrpc::prelude::Status>;
                }
            },
        );

        quote! {

            // #(#attrs)*
            #[async_trait::async_trait]
            #vis trait #service_ident : Clone{
                #(#fns)*

            }
        }
    }

    fn struct_server(&self) -> TokenStream2 {
        let &Self {
            vis,
            server_ident,
            service_ident,
            ..
        } = self;

        quote! {

            #[derive(Debug, Clone)]
            #[automatically_derived]
            #vis struct #server_ident<T: #service_ident> {
               inner: std::sync::Arc<T>
            }

            #[automatically_derived]
            impl<T> #server_ident<T>
            where
                T: #service_ident
            {
                pub fn new(inner: T) -> Self {

                  Self{
                    inner: std::sync::Arc::new(inner)
                  }
                }
            }

        }
    }

    // implements Service for the server
    // this is where the actual user code is called
    fn impl_service_for_server(&self) -> TokenStream2 {
        let &Self {
            server_ident,
            service_ident,
            method_idents,
            ..
        } = self;

        let method_idents_str: Vec<_> = method_idents.iter().map(|i| i.to_string()).collect();

        let request_idents: Vec<_> = method_idents
            .iter()
            .map(|i| format_ident!("{}Request", snake_to_camel(&i.to_string())))
            .collect();
        let service_ident_str = service_ident.to_string();
        quote! {

            #[automatically_derived]
            impl<S> zrpc::prelude::Service for #server_ident<S>
            where S: #service_ident +'static
            {

                fn call(&self, req: zrpc::prelude::Message) -> zrpc::prelude::BoxFuture<zrpc::prelude::Message, zrpc::prelude::Status> {
                    match req.method.as_str() {
                        #(
                                #method_idents_str => {
                                let req = zrpc::prelude::deserialize::<zrpc::prelude::Request<#request_idents>>(&req.body).unwrap();
                                let inner = self.inner.clone();
                                let fut = async move {
                                    match inner.#method_idents(req).await {
                                        Ok(resp) => Ok(resp.into()),
                                        Err(s) => Err(s)
                                    }
                                };
                                Box::pin(fut)
                            }
                        )*
                        _ => Box::pin(async move { Err(zrpc::prelude::Status::new(zrpc::prelude::Code::Unvailable, "Unavailable")) }),
                    }
                }

                fn name(&self) -> String {

                    #service_ident_str.into()
                }

            }
        }
    }

    fn derive_requests(&self) -> TokenStream2 {
        let &Self { methods, .. } = self;
        let mut types = vec![];

        for m in methods {
            let ident = &m.ident;
            let args = &m.args;
            let ident = format_ident!("{}Request", snake_to_camel(&ident.to_string()));

            let tokens = quote! {
                #[automatically_derived]
                #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
                pub struct #ident {
                    #( pub #args ),*
                }
            };

            types.push(tokens);
        }

        let mut tt = quote! {};
        tt.extend(types);

        tt
    }

    fn derive_responses(&self) -> TokenStream2 {
        let &Self { methods, .. } = self;
        let mut types = vec![];
        let unit_type: &Type = &parse_quote!(());

        for m in methods {
            let ident = &m.ident;
            let output = match &m.output {
                ReturnType::Type(_, ref ty) => ty,
                ReturnType::Default => unit_type,
            };
            let ident = format_ident!("{}Response", snake_to_camel(&ident.to_string()));

            let tokens = quote! {
                #[automatically_derived]
                #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
                pub struct #ident(#output);


                #[automatically_derived]
                impl std::convert::From<#output> for #ident {
                    fn from(value: #output) -> Self {
                        Self(value)
                    }
                }
            };

            types.push(tokens);
        }

        let mut tt = quote! {};
        tt.extend(types);

        tt
    }

    // Generates the implentation of the client
    fn impl_client(&self) -> TokenStream2 {
        let &Self {
            vis,
            service_ident,
            methods,
            tout,
            ..
        } = self;

        let client_ident = format_ident!("{}Client", service_ident);
        let service_ident_str = service_ident.to_string();
        let rpc_ke = format!("@rpc/*/service/{service_ident_str}");
        let ke_fmt = "@rpc/${zid:*}/service/";
        let format_str = format!("{ke_fmt}{service_ident_str}");

        let fns = methods.iter().map(
            |method| {

                let ident = &method.ident;
                let mut receiver = method.receiver.clone();
                receiver.mutability=None;


                let attrs = &method.attrs;
                let request_ident = format_ident!("{}Request", snake_to_camel(&ident.to_string()));
                let response_ident = format_ident!("{}Response", snake_to_camel(&ident.to_string()));
                let ident_str = &method.ident.to_string();

                quote! {

                    #(#attrs)*
                    pub async fn #ident(#receiver, request: zrpc::prelude::Request<#request_ident>) ->  std::result::Result<zrpc::prelude::Response<#response_ident>, zrpc::prelude::Status> {
                        self.ch.call_fun(self.find_server().await, request, #ident_str, self.tout).await.into()
                    }
                }
            },
        );

        quote! {

            #[allow(unused)]
            #[derive(Clone, Debug)]
            #vis struct #client_ident<'a> {
                ch : zrpc::prelude::RPCClientChannel,
                z: Arc<zenoh::Session>,
                ke_format: zenoh::key_expr::format::KeFormat<'a>,
                tout: std::time::Duration,
            }

            impl<'a> #client_ident<'a> {
                #vis async fn new(
                    z : async_std::sync::Arc<zenoh::Session>,
                ) -> #client_ident<'a> {
                        let new_client = zrpc::prelude::RPCClientChannel::new(z.clone(), #service_ident_str.to_string());
                        let ke_format = zenoh::key_expr::format::KeFormat::new(#format_str).unwrap();
                        let tout = std::time::Duration::from_secs(#tout as u64);
                        #client_ident{
                            ch : new_client,
                            z,
                            ke_format,
                            tout,
                        }

                    }


                    #(#fns)*

                    async fn find_server(&self) -> zenoh::prelude::ZenohId {
                        let res = self
                            .z
                            .liveliness()
                            .get(#rpc_ke)
                            .res()
                            .await
                            .unwrap();

                        let mut ids: Vec<zenoh::prelude::ZenohId> = res
                            .into_iter()
                            .map(|e| self.extract_id_from_ke(e.sample.unwrap().key_expr()))
                            .collect();
                        ids.pop().unwrap()
                    }

                    fn extract_id_from_ke(&self, ke: &zenoh::key_expr::KeyExpr) -> zenoh::prelude::ZenohId {
                        use std::str::FromStr;
                        self.ke_format
                            .parse(ke)
                            .unwrap()
                            .get("zid")
                            .map(zenoh::prelude::ZenohId::from_str)
                            .unwrap()
                            .unwrap()
                    }
            }
        }
    }
}

/// Converts ServiceGenerator to actual code
impl<'a> ToTokens for ServiceGenerator<'a> {
    fn to_tokens(&self, output: &mut TokenStream2) {
        output.extend(vec![
            self.trait_service(),
            self.struct_server(),
            self.impl_service_for_server(),
            self.derive_requests(),
            self.derive_responses(),
            self.impl_client(),
            // self.impl_client(),
            // self.impl_client_eval_methods(),
        ])
    }
}

/// Converts to snake_case to CamelCase, is used to convert functions name
fn snake_to_camel(ident_str: &str) -> String {
    let mut camel_ty = String::with_capacity(ident_str.len());

    let mut last_char_was_underscore = true;
    for c in ident_str.chars() {
        match c {
            '_' => last_char_was_underscore = true,
            c if last_char_was_underscore => {
                camel_ty.extend(c.to_uppercase());
                last_char_was_underscore = false;
            }
            c => camel_ty.extend(c.to_lowercase()),
        }
    }

    camel_ty.shrink_to_fit();
    camel_ty
}
