/*********************************************************************************
* Copyright (c) 2018,2020 ADLINK Technology Inc.
*
* This program and the accompanying materials are made available under the
* terms of the Eclipse Public License 2.0 which is available at
* http://www.eclipse.org/legal/epl-2.0, or the Apache Software License 2.0
* which is available at https://www.apache.org/licenses/LICENSE-2.0.
*
* SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
* Contributors:
*   ADLINK fog05 team, <fog05@adlink-labs.tech>
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
use inflector::cases::snakecase::to_snake_case;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use std::str::FromStr;
use syn::visit_mut::VisitMut;
use syn::{
    braced,
    ext::IdentExt,
    parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    spanned::Spanned,
    token::Comma,
    Attribute, AttributeArgs, Block, FnArg, Ident, ImplItem, ImplItemMethod, ImplItemType,
    ItemImpl, Pat, PatIdent, PatType, Receiver, ReturnType, Signature, Token, Type, Visibility,
};
use syn_serde::json;

mod receiver;
use receiver::ReplaceReceiver;

macro_rules! extend_errors {
    ($errors: ident, $e: expr) => {
        match $errors {
            Ok(_) => $errors = Err($e),
            Err(ref mut errors) => errors.extend($e),
        }
    };
}

#[derive(Debug, FromMeta)]
struct ZServiceMacroArgs {
    timeout_s: u16,
    #[darling(default)]
    prefix: Option<String>,
    #[darling(default)]
    service_uuid: Option<String>,
}

struct ZService {
    attrs: Vec<Attribute>,
    vis: Visibility,
    ident: Ident,
    evals: Vec<EvalMethod>,
}

impl Parse for ZService {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        input.parse::<Token![trait]>()?;
        let ident: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut evals = Vec::<EvalMethod>::new();
        while !content.is_empty() {
            evals.push(content.parse()?);
        }
        let mut ident_errors = Ok(());
        for eval in &evals {
            if eval.ident == "new" {
                extend_errors!(
                    ident_errors,
                    syn::Error::new(
                        eval.ident.span(),
                        format!(
                            "method name conflicts with generated fn `{}Client::new`",
                            ident.unraw()
                        )
                    )
                );
            }
            if eval.ident == "serve" {
                extend_errors!(
                    ident_errors,
                    syn::Error::new(
                        eval.ident.span(),
                        format!("method name conflicts with generated fn `{}::serve`", ident)
                    )
                );
            }
        }
        ident_errors?;

        Ok(Self {
            attrs,
            vis,
            ident,
            evals,
        })
    }
}

struct EvalMethod {
    attrs: Vec<Attribute>,
    ident: Ident,
    receiver: Receiver,
    args: Vec<PatType>,
    output: ReturnType,
}

impl Parse for EvalMethod {
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
                FnArg::Receiver(receiver) => {
                    //Should take whatever used by the user and strip it for client
                    recv = Some(receiver)
                    // extend_errors!(
                    //     errors,
                    //     syn::Error::new(arg.span(), "method args cannot start with self, &mut self is added by the macro!")
                    // );
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

/// Generates:
/// - service trait
/// - serve fn
/// - client stub struct
/// - new_stub client factory fn
/// - Request and Response enums
#[proc_macro_attribute]
pub fn zservice(attr: TokenStream, input: TokenStream) -> TokenStream {
    let unit_type: &Type = &parse_quote!(());

    //parsing the trait body
    let ZService {
        ref attrs,
        ref vis,
        ref ident,
        ref evals,
    } = parse_macro_input!(input as ZService);

    //parsing the attributes to the macro
    let attr_args = parse_macro_input!(attr as AttributeArgs);
    let macro_args = match ZServiceMacroArgs::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    //converts the functions names from snake_case to CamelCase
    let camel_case_fn_names: &Vec<_> = &evals
        .iter()
        .map(|eval| snake_to_camel(&eval.ident.unraw().to_string()))
        .collect();

    let snake_case_ident = to_snake_case(&ident.unraw().to_string());

    // Collects the pattern for the types
    let args: &[&[PatType]] = &evals.iter().map(|eval| &*eval.args).collect::<Vec<_>>();

    let service_uuid = match macro_args.service_uuid {
        Some(u) => uuid::Uuid::from_str(&u).unwrap(),
        None => uuid::Uuid::new_v4(),
    };

    //service eval path
    let path = match macro_args.prefix {
        Some(prefix) => format!("{}/zservice/{}/{}/", prefix, ident, service_uuid),
        None => format!("zservice/{}/{}/", ident, service_uuid),
    };

    let service_name = format!("{}Service", ident);
    // Generates the code
    let ts: TokenStream = ZServiceGenerator {
        service_ident: ident,
        server_ident: &format_ident!("Serve{}", ident), //Server is called Serve<Trait Name>
        client_ident: &format_ident!("{}Client", ident), //Client is called <Trait Name>Client
        request_ident: &format_ident!("{}Request", ident), //Request type is called <Trait Name>Request
        response_ident: &format_ident!("{}Response", ident), //Response type is called <Trait Name>Response
        vis,
        args,
        method_attrs: &evals.iter().map(|eval| &*eval.attrs).collect::<Vec<_>>(), //getting evals attributes
        method_idents: &evals.iter().map(|eval| &eval.ident).collect::<Vec<_>>(), //getting evals names
        attrs,
        evals,
        return_types: &evals //getting evals return type, if non present using unit
            .iter()
            .map(|eval| match eval.output {
                ReturnType::Type(_, ref ty) => ty,
                ReturnType::Default => unit_type,
            })
            .collect::<Vec<_>>(),
        arg_pats: &args
            .iter()
            .map(|args| args.iter().map(|arg| &*arg.pat).collect())
            .collect::<Vec<_>>(),
        camel_case_idents: &evals
            .iter()
            .zip(camel_case_fn_names.iter())
            .map(|(eval, name)| Ident::new(name, eval.ident.span()))
            .collect::<Vec<_>>(),
        timeout: &macro_args.timeout_s,
        eval_path: &path,
        service_name: &service_name,
        service_get_server_ident: &format_ident!("get_{}_server", snake_case_ident),
    }
    .into_token_stream()
    .into();
    ts
}

/// Update the implementation of the trait
///
/// Modifies the method in a similar way to what async_trait does
/// This impl:
///
/// #[zserver]
/// impl Hello for Server {
///     async pub fn hello(&self, param : String) -> String {
///         format!("Hello {} from {}", param, self)
///     }
/// }
///
/// Should become something like:
///
/// impl Hello for Server {
///     pub fn hello(&self, param : String) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = String> + '_ >> {
///         async fn __hello(_self : &Server, param : String) -> String {
///             format!("Hello {} from {}", param, self)
///         }
///         Box::pin(__hello(self,param))
///     }
/// }
#[proc_macro_attribute]
pub fn zserver(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut item = syn::parse_macro_input!(input as ItemImpl);
    let span = item.span();

    // let attr_args = parse_macro_input!(_attr as AttributeArgs);

    let mut expected_non_async_types: Vec<(&ImplItemMethod, String)> = Vec::new();
    let mut found_non_async_types: Vec<&ImplItemType> = Vec::new();

    for inner in &mut item.items {
        match inner {
            ImplItem::Method(method) => {
                if method.sig.asyncness.is_some() {
                    // if this function is declared async, transform it into a regular function
                    //method.sig.asyncness = None;
                    // put the body inside an task::block_on(async {})
                    /*
                    let content = method.block.to_token_stream();

                    let updated_impl = TokenStream::from(quote! {
                        {
                            task::block_on(
                                async move
                                #content
                            )
                        }
                    });
                    // and add the  #[allow(unused,clippy::manual_async_fn)] attribute
                    method
                        .attrs
                        .push(parse_quote!(#[allow(unused,clippy::manual_async_fn)]));
                    method.block = parse_macro_input!(updated_impl as Block);
                    */
                    let sig = &mut method.sig;
                    let block = &mut method.block;
                    transform_server_method_block(sig, block, &item.self_ty);
                    transform_server_method_sig(sig);
                    method
                        .attrs
                        .push(parse_quote!(#[allow(unused,clippy::manual_async_fn)]));
                } else {
                    // If it's not async, keep track of all required associated types for better
                    // error reporting.
                    expected_non_async_types.push((method, associated_type_for_eval(method)));
                }
            }
            ImplItem::Type(typedecl) => found_non_async_types.push(typedecl),
            _ => {}
        }
    }

    if let Err(e) =
        verify_types_were_provided(span, &expected_non_async_types, &found_non_async_types)
    {
        return TokenStream::from(e.to_compile_error());
    }

    TokenStream::from(quote!(#item))
}

/// Transforms the method signature for the server
/// to return a Pin<Box<dyn Future<Output = #old_return_type > + Send + '_ >>
fn transform_server_method_sig(sig: &mut Signature) {
    let ret = match &sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ret) => quote!(#ret),
    };

    sig.output = parse_quote! {
        -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #ret> + core::marker::Send + '_ >>
    };
    sig.asyncness = None;
}

fn transform_server_method_block(sig: &mut Signature, block: &mut Block, receiver: &Type) {
    let inner_ident = format_ident!("__{}", sig.ident);

    let args = sig.inputs.iter().enumerate().map(|(i, arg)| match arg {
        FnArg::Receiver(Receiver { self_token, .. }) => quote!(#self_token),
        FnArg::Typed(arg) => {
            if let Pat::Ident(PatIdent { ident, .. }) = &*arg.pat {
                quote!(#ident)
            } else {
                format_ident!("__arg{}", i).into_token_stream()
            }
        }
    });

    let mut standalone = sig.clone();
    standalone.ident = inner_ident.clone();

    match standalone.inputs.iter_mut().next() {
        Some(
            arg @ FnArg::Receiver(Receiver {
                reference: Some(_), ..
            }),
        ) => {
            let (self_token, mutability) = match arg {
                FnArg::Receiver(Receiver {
                    mutability,
                    self_token,
                    ..
                }) => (self_token, mutability),
                _ => unreachable!(),
            };
            let under_self = Ident::new("_self", self_token.span);
            *arg = parse_quote! {
                #mutability #under_self: &#receiver
            };
        }
        Some(arg @ FnArg::Receiver(_)) => {
            let (self_token, mutability) = match arg {
                FnArg::Receiver(Receiver {
                    mutability,
                    self_token,
                    ..
                }) => (self_token, mutability),
                _ => unreachable!(),
            };
            let under_self = Ident::new("_self", self_token.span);
            *arg = parse_quote! {
                #mutability #under_self: #receiver
            };
        }
        Some(FnArg::Typed(arg)) => {
            if let Pat::Ident(arg) = &mut *arg.pat {
                if arg.ident == "self" {
                    arg.ident = Ident::new("_self", arg.ident.span());
                }
            }
        }
        _ => {}
    }

    let mut replace = ReplaceReceiver::with_as_trait(receiver.clone(), None);
    replace.visit_signature_mut(&mut standalone);
    replace.visit_block_mut(block);

    let brace = block.brace_token;
    let box_pin = quote_spanned!(
        brace.span => {
            #standalone #block
            Box::pin(#inner_ident(#(#args),*))
    });
    *block = parse_quote!(#box_pin);
    block.brace_token = brace;
}

/// Creates the type name for a future, to be removed...
fn associated_type_for_eval(method: &ImplItemMethod) -> String {
    snake_to_camel(&method.sig.ident.unraw().to_string()) + "Fut"
}

/// Verifies if the types are provide for each methods
fn verify_types_were_provided(
    span: Span,
    expected: &[(&ImplItemMethod, String)],
    provided: &[&ImplItemType],
) -> syn::Result<()> {
    let mut result = Ok(());
    for (method, expected) in expected {
        if !provided.iter().any(|typedecl| typedecl.ident == expected) {
            let mut e = syn::Error::new(
                span,
                format!("not all trait items implemented, missing: `{}`", expected),
            );
            let fn_span = method.sig.fn_token.span();
            e.extend(syn::Error::new(
                fn_span.join(method.sig.ident.span()).unwrap_or(fn_span),
                format!(
                    "hint: `#[zerver]` only rewrites async fns, and `fn {}` is not async",
                    method.sig.ident
                ),
            ));
            match result {
                Ok(_) => result = Err(e),
                Err(ref mut error) => error.extend(Some(e)),
            }
        }
    }
    result
}

/// Generator for the ZService
struct ZServiceGenerator<'a> {
    service_ident: &'a Ident,            //service type
    server_ident: &'a Ident,             //server type
    client_ident: &'a Ident,             //client type
    request_ident: &'a Ident,            //request type
    response_ident: &'a Ident,           //response type
    vis: &'a Visibility,                 //visibility
    attrs: &'a [Attribute],              //attributes
    evals: &'a [EvalMethod],             //functions to be exposed via evals
    camel_case_idents: &'a [Ident],      //camel case conversion of all names
    method_idents: &'a [&'a Ident],      //type of the methods
    method_attrs: &'a [&'a [Attribute]], //attributes of the methods
    args: &'a [&'a [PatType]],           // types description pattern
    return_types: &'a [&'a Type],        // return types of functions
    arg_pats: &'a [Vec<&'a Pat>],        // patterns for args
    timeout: &'a u16,                    //eval timeout
    eval_path: &'a String,               //path for evals
    service_name: &'a String,            //service name on zenoh
    service_get_server_ident: &'a Ident, //the ident for the get_<trait>_server
}

impl<'a> ZServiceGenerator<'a> {
    // crates the service trait
    fn trait_service(&self) -> TokenStream2 {
        let &Self {
            attrs,
            evals,
            vis,
            return_types,
            service_ident,
            server_ident,
            service_get_server_ident,
            ..
        } = self;

        let fns = evals.iter().zip(return_types.iter()).map(
            |(
                EvalMethod {
                    attrs,
                    ident,
                    receiver,
                    args,
                    ..
                },
                output,
            )| {
                quote! {

                    #(#attrs)*
                    //fn #ident(#receiver, #(#args),*) ->  #output;
                    fn #ident(#receiver, #(#args),*) ->  ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #output> + core::marker::Send + '_ >>;
                }
            },
        );

        quote! {

        #(#attrs)*
        #vis trait #service_ident : Clone{
            #(#fns)*

            /// Returns the server object
            fn #service_get_server_ident(self, z : async_std::sync::Arc<zenoh::Session>, id : Option<uuid::Uuid>) -> #server_ident<Self>{
                let id = id.unwrap_or_else(Uuid::new_v4);
                log::trace!("Getting Server with ID {}", id);
                #server_ident::new(z,self, id)
                }
            }

        }
    }

    //creates the server struct
    fn struct_server(&self) -> TokenStream2 {
        let &Self {
            vis,
            server_ident,
            service_name,
            ..
        } = self;

        quote! {
            #[derive(Clone)]
            #vis struct #server_ident<S> {
                z : async_std::sync::Arc<zenoh::Session>,
                server: S,
                instance_id: uuid::Uuid,
                state : async_std::sync::Arc<async_std::sync::RwLock<zrpc::ComponentState>>
            }

            impl<S> #server_ident<S> {
                pub fn new(z : async_std::sync::Arc<zenoh::Session>, server : S, id : uuid::Uuid) -> Self {

                    let ci = zrpc::ComponentState{
                        uuid : id,
                        name : format!("{}", #service_name),
                        routerid : "".to_string(),
                        peerid : "".to_string(),
                        status : zrpc::ComponentStatus::HALTED,
                    };

                    Self {
                        z,
                        server,
                        instance_id : id,
                        state : async_std::sync::Arc::new(async_std::sync::RwLock::new(ci))
                    }
                }
            }

        }
    }

    // implements ZServe for the server
    fn impl_serve_for_server(&self) -> TokenStream2 {
        let &Self {
            request_ident,
            server_ident,
            service_ident,
            response_ident,
            camel_case_idents,
            arg_pats,
            method_idents,
            eval_path,
            service_name,
            ..
        } = self;

        quote! {


            impl<S> zrpc::ZNServe<#request_ident> for #server_ident<S>
            where S: #service_ident + Send +'static
            {
                type Resp = #response_ident;

                fn instance_uuid(&self) -> uuid::Uuid {
                    self.instance_id
                }

                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn connect(&'_ self) ->
                ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<(async_std::channel::Sender<()>, async_std::task::JoinHandle<ZRPCResult<()>>)>> + '_>> {
                    log::trace!("Connect Service {} Instance {}", #service_name, self.instance_uuid());

                    async fn __connect<S>(_self: &#server_ident<S>) -> ZRPCResult<(async_std::channel::Sender<()>, async_std::task::JoinHandle<ZRPCResult<()>>)>
                    where
                        S: #service_ident + Send + 'static,
                    {
                        use futures::prelude::*;
                        use std::convert::TryInto;
                        use zenoh::prelude::r#async::*;
                        use zenoh::prelude::*;


                        let zinfo = _self.z.info();
                        let pid = zinfo.zid().res().await.to_string().to_uppercase();

                        let rid = match zinfo
                            .routers_zid()
                            .res()
                            .await
                            .collect::<Vec<ZenohId>>()
                            .first()
                        {
                            Some(head) => head.to_string().to_uppercase(),
                            None => "".to_string(),
                        };

                        let mut ci = _self.state.write().await;
                        ci.peerid = pid.clone().to_uppercase();
                        drop(ci);
                        let (s,r) = async_std::channel::bounded::<()>(1);

                        let zsession = async_std::sync::Arc::clone(&_self.z);

                        let state = _self.state.clone();
                        let path = format!("{}{}/state",#eval_path,_self.instance_uuid());

                        let run_loop = async move {
                            let mut queryable = zsession
                                .declare_queryable(&path)
                                .res()
                                .await?;

                            let kexpr: KeyExpr = (path.clone().try_into())
                                .map_err(|e| zrpc::zrpcresult::ZRPCError::ZenohError(format!("{:?}",e)))?;


                            loop {
                                let query = queryable
                                    .recv_async()
                                    .await
                                    .map_err(|_| zrpc::zrpcresult::ZRPCError::MissingValue)?;
                                let ci = state.read().await;
                                let data = zrpc::serialize::serialize_state(&*ci)?;
                                drop(ci);
                                let value = Value::new(data.into())
                                    .encoding(Encoding::APP_OCTET_STREAM);
                                let sample = Sample::new(kexpr.clone(), value);
                                query.reply(Ok(sample)).res().await.map_err(|e| {
                                    zrpc::zrpcresult::ZRPCError::ZenohError(format!("{:?}",e))
                                })?;
                            }
                        };


                        let (abort_handle, abort_registration) = zrpc::AbortHandle::new_pair();

                        log::trace!("Spawning state responder task");
                        let task_handle =
                            async_std::task::spawn(zrpc::Abortable::new(run_loop, abort_registration));

                        Ok((abort_handle, task_handle))
                    }
                    Box::pin(__connect(self))
                }


                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn initialize(&self) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
                    log::trace!("Initialize Service {} Instance {}", #service_name, self.instance_uuid());
                    async fn __initialize<S>(_self: &#server_ident<S>) -> ZRPCResult<()>
                    where
                        S: #service_ident + Send + 'static,
                    {
                        let mut ci = _self.state.write().await;
                        match ci.status {
                            zrpc::ComponentStatus::HALTED =>{
                                ci.status = zrpc::ComponentStatus::INITIALIZING;
                                Ok(())
                            },
                            _ => Err(ZRPCError::StateTransitionNotAllowed("Cannot initialize a component in a state different than HALTED".to_string())),
                        }

                    }
                    Box::pin(__initialize(self))
                }


                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn register(&self) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>{
                    log::trace!("Register Service {} Instance {}", #service_name, self.instance_uuid());
                    async fn __register<S>(_self: &#server_ident<S>) -> ZRPCResult<()>
                    where
                    S: #service_ident + Send + 'static,
                    {
                        let mut ci = _self.state.write().await;
                        match ci.status {
                            zrpc::ComponentStatus::INITIALIZING => {
                                ci.status = zrpc::ComponentStatus::REGISTERED;
                                Ok(())
                            },
                            _ => Err(ZRPCError::StateTransitionNotAllowed("Cannot register a component in a state different than INITIALIZING".to_string())),
                        }
                    }
                    Box::pin(__register(self))
                }

                #[allow(clippy::type_complexity,clippy::manual_async_fn, clippy::needless_question_mark)]
                fn start(
                    &self,
                ) -> ::core::pin::Pin<
                    Box<
                        dyn std::future::Future<
                                Output = ZRPCResult<(
                                    async_std::channel::Sender<()>,
                                    async_std::task::JoinHandle<ZRPCResult<()>>
                                )>> + '_>>
                    {

                    log::trace!("Start Service {} Instance {}", #service_name, self.instance_uuid());

                    async fn __start<S>(
                        _self: &#server_ident<S>,
                    ) -> ZRPCResult<(
                        async_std::channel::Sender<()>,
                        async_std::task::JoinHandle<ZRPCResult<()>>,
                    )>
                    where
                        S: #service_ident + Send + 'static,
                    {
                            let (s, r) = async_std::channel::bounded::<()>(1);
                            let barrier = async_std::sync::Arc::new(async_std::sync::Barrier::new(2));
                            let ci = _self.state.read().await;
                            match ci.status {
                                zrpc::ComponentStatus::REGISTERED => {
                                    drop(ci);


                                    let server = _self.clone();
                                    let b =  barrier.clone();

                                    let h = async_std::task::spawn_blocking( move || {
                                        async_std::task::block_on(async {
                                            server.serve(r, b).await
                                        })
                                    });
                                    log::trace!("Waiting for serving loop to be ready");
                                    barrier.wait().await;

                                    // Updating status, using barrier to avoid race condition
                                    let mut ci = _self.state.write().await;
                                    ci.status = zrpc::ComponentStatus::SERVING;
                                    drop(ci);

                                    Ok((s,h))

                                }
                                _ => Err(ZRPCError::StateTransitionNotAllowed("Cannot start a component in a state different than REGISTERED".to_string())),
                            }

                    }
                    Box::pin(__start(self))
                }

                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn run(&self) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
                    log::trace!("Run Service {} Instance {}", #service_name, self.instance_uuid());
                    async fn __run<S>(_self: &#server_ident<S>) -> ZRPCResult<()>
                    where
                        S: #service_ident + Send + 'static,
                    {
                        use std::convert::TryInto;
                        use zenoh::prelude::r#async::*;
                        use zenoh::prelude::*;

                        let path = format!("{}{}/eval",#eval_path, _self.instance_uuid());
                        log::trace!("Registering eval on {:?}", path);
                        let mut queryable = _self
                            .z
                            .declare_queryable(&path)
                            .res()
                            .await?;

                        let kexpr: KeyExpr = (path.clone().try_into())
                            .map_err(|e| zrpc::zrpcresult::ZRPCError::ZenohError(format!("{:?}",e)))?;

                        log::trace!("Registered on {:?}", path);
                        loop {
                            let query = queryable.recv_async().await.map_err(|_| zrpc::zrpcresult::ZRPCError::MissingValue)?;
                            log::trace!("Received query {:?}", query);
                            let query_selector = query.selector();
                            let parsed_selector = query_selector.parameters_cowmap()?;
                            let base64_req = parsed_selector.get("req").ok_or(zrpc::zrpcresult::ZRPCError::MissingValue)?;
                            let b64_bytes = base64::decode(base64_req.as_bytes())?;
                            let req = zrpc::serialize::deserialize_request::<#request_ident>(&b64_bytes)?;
                            log::trace!("Received on {:?} {:?}", path, req);

                            let mut ser = _self.server.clone();

                            let encoded_resp  = match req.clone() {
                                #(
                                    #request_ident::#camel_case_idents{#(#arg_pats),*} => {
                                        let resp = #response_ident::#camel_case_idents(ser.#method_idents( #(#arg_pats),*).await);
                                        log::trace!("Reply to {:?} {:?} with {:?}", path, req, resp);
                                        zrpc::serialize::serialize_response(&resp)
                                    }
                                )*
                            }?;
                            let value = Value::new(encoded_resp.into()).encoding(Encoding::APP_OCTET_STREAM);
                            let sample = Sample::new(kexpr.clone(), value);
                            query
                                .reply(Ok(sample))
                                .res()
                                .await
                                .map_err(|e| zrpc::zrpcresult::ZRPCError::ZenohError(format!("{:?}",e)))?;
                        }
                    }
                    Box::pin( __run(self))
                }

                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn serve(
                    &self,
                    stop: async_std::channel::Receiver<()>,
                    barrier : async_std::sync::Arc<async_std::sync::Barrier>,
                ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
                    log::trace!("Serve Service {} Instance {}", #service_name, self.instance_uuid());
                    async fn __serve<S>(_self: &#server_ident<S>, _stop: async_std::channel::Receiver<()>, _barrier : async_std::sync::Arc<async_std::sync::Barrier>) -> ZRPCResult<()>
                    where
                        S: #service_ident + Send + 'static,
                    {
                        use futures::prelude::*;
                        use async_std::prelude::FutureExt;

                        let ci = _self.state.read().await;
                        match ci.status {
                            zrpc::ComponentStatus::REGISTERED => {
                                drop(ci);

                                _barrier.wait().await;

                                log::trace!("RPC Receiver loop started...");
                                loop {
                                    let run = async {
                                        match _self.run().await {
                                            Ok(_) => zrpc::RunResultAction::Restart(None),
                                            Err(e) => zrpc::RunResultAction::Restart(Some(e)),
                                        }
                                    };
                                    let stopper = async {
                                        match _stop.recv().await {
                                            Ok(_) => zrpc::RunResultAction::Stop,
                                            Err(e) => zrpc::RunResultAction::StopError(ZRPCError::Error(format!("{}", e)))
                                        }
                                    };

                                    match run.race(stopper).await {
                                        zrpc::RunResultAction::Restart(e) => {
                                            log::error!("The run loop existed with {:?}, restaring...", e);
                                            continue;
                                        }
                                        zrpc::RunResultAction::Stop => {
                                            log::trace!("Received kill command, killing runner");
                                            break Ok(());
                                        }
                                        zrpc::RunResultAction::StopError(e) => {
                                            log::error!("The ZRPC stopper recv got an error: {}, exiting... maybe the sender was dropped?", e);
                                            break Err(e);
                                        }

                                    }
                                }
                            }
                            _ => Err(ZRPCError::StateTransitionNotAllowed("State is not WORK, serve called directly? serve is called by calling work!".to_string())),
                        }
                    }
                    let res  = __serve(self, stop, barrier);
                    Box::pin(res)
                }

                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn stop(
                    &self,
                    stop: async_std::channel::Sender<()>,
                ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
                    log::trace!("Stop Service {} Instance {}", #service_name, self.instance_uuid());
                    async fn __stop<S>(_self: &#server_ident<S>, _stop: async_std::channel::Sender<()>) -> ZRPCResult<()>
                    where
                        S: #service_ident + Send + 'static,
                    {
                        let mut ci = _self.state.write().await;
                        match ci.status {
                            zrpc::ComponentStatus::SERVING => {
                                ci.status = zrpc::ComponentStatus::REGISTERED;
                                drop(ci);
                                _stop.abort();
                                Ok(())
                            },
                            _ => Err(ZRPCError::StateTransitionNotAllowed("Cannot stop a component in a state different than WORK".to_string())),
                        }
                    }
                    Box::pin(__stop(self, stop))
                }

                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn unregister(&self) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
                    log::trace!("Unregister Service {} Instance {}", #service_name, self.instance_uuid());
                    async fn __unregister<S>(_self: &#server_ident<S>) -> ZRPCResult<()>
                    where
                        S: #service_ident + Send + 'static,
                    {
                        let mut ci = _self.state.write().await;
                        match ci.status {
                            zrpc::ComponentStatus::REGISTERED =>{
                                ci.status = zrpc::ComponentStatus::HALTED;
                                Ok(())
                            },
                            _ => Err(ZRPCError::StateTransitionNotAllowed("Cannot unregister a component in a state different than REGISTERED".to_string())),
                        }
                    }
                    Box::pin(__unregister(self))
                }

                #[allow(clippy::type_complexity,clippy::manual_async_fn)]
                fn disconnect(&self, stop: async_std::channel::Sender<()>) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
                    log::trace!("Disconnect Service {} Instance {}", #service_name, self.instance_uuid());
                    async fn __disconnect<S>(_self: &#server_ident<S>, _stop: async_std::channel::Sender<()>) -> ZRPCResult<()>
                    where
                        S: #service_ident + Send + 'static,
                        {
                            let mut ci = _self.state.write().await;
                            match ci.status {
                                zrpc::ComponentStatus::HALTED => {
                                    ci.status = zrpc::ComponentStatus::HALTED;
                                    drop(ci);
                                    _stop.abort();
                                    Ok(())
                                },
                                _ => Err(ZRPCError::StateTransitionNotAllowed("Cannot disconnect a component in a state different than HALTED".to_string())),
                            }
                        }
                    Box::pin(__disconnect(self,stop))
                }

            }
        }
    }

    // Generates the request enum type, and makes it derive Debug, Serialize and Deserialize
    fn enum_request(&self) -> TokenStream2 {
        let &Self {
            vis,
            request_ident,
            camel_case_idents,
            args,
            ..
        } = self;

        quote! {
            /// The request sent over the wire from the client to the server.
            #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
            #vis enum #request_ident {
                #( #camel_case_idents{ #( #args ),* } ),*
            }
        }
    }

    // Generates the response enum type, and makes it derive Debug, Serialize and Deserialize
    fn enum_response(&self) -> TokenStream2 {
        let &Self {
            vis,
            response_ident,
            camel_case_idents,
            return_types,
            ..
        } = self;

        quote! {
            /// The response sent over the wire from the server to the client.
            #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
            #vis enum #response_ident {
                #( #camel_case_idents(#return_types) ),*
            }
        }
    }

    // Generates the client struct
    fn struct_client(&self) -> TokenStream2 {
        let &Self {
            vis,
            client_ident,
            request_ident,
            response_ident,
            ..
        } = self;

        quote! {
            #[allow(unused)]
            #[derive(Clone, Debug)]
            #vis struct #client_ident<C = zrpc::ZNClientChannel<#request_ident, #response_ident>>{
                ch : C,
                server_uuid : Uuid,
            }
        }
    }

    // Generates the implentation of the client
    fn impl_client_new_find_servers(&self) -> TokenStream2 {
        let &Self {
            client_ident,
            vis,
            eval_path,
            ..
        } = self;

        quote! {
            impl #client_ident {
                #vis fn new(
                    z : async_std::sync::Arc<zenoh::Session>,
                    instance_id : uuid::Uuid
                ) -> #client_ident {
                        let new_client = zrpc::ZNClientChannel::new(z, format!("{}",#eval_path), Some(instance_id));
                        #client_ident{
                            ch : new_client,
                            server_uuid : instance_id,
                        }

                    }

                #vis fn get_server_uuid(&self) -> Uuid {
                    self.server_uuid
                }

                #vis fn find_servers(
                    z : async_std::sync::Arc<zenoh::Session>
                ) -> impl std::future::Future<Output = ZRPCResult<Vec<uuid::Uuid>>> + 'static
                {
                    async move {
                        use zenoh::prelude::r#async::*;
                        use zenoh::query::*;
                        use zenoh::prelude::*;


                        let selector = format!("{}*/state",#eval_path);
                        log::trace!("Find servers selector {}", selector);
                        let mut servers = Vec::new();

                        let replies = z.get(&selector).target(QueryTarget::All).res().await?;

                        while let Ok(d) = replies.recv_async().await {
                            match d.sample {
                                Ok(sample) => match sample.value.encoding {
                                    Encoding::APP_OCTET_STREAM => {
                                        let ca = zrpc::serialize::deserialize_state::<zrpc::ComponentState>(
                                            &sample.value.payload.contiguous(),
                                        )?;
                                        servers.push(ca.uuid);
                                    }
                                    _ => {
                                        return Err(ZRPCError::ZenohError(
                                            "Server information is not correctly encoded".to_string(),
                                        ))
                                    }
                                },
                                Err(e) => {
                                    return Err(ZRPCError::ZenohError(format!(
                                        "Unable to get sample from {:?}",e
                                    )))
                                }
                            }
                        }
                        Ok(servers)
                    }
                }

                #vis fn find_servers_info(
                    z : async_std::sync::Arc<zenoh::Session>
                ) -> impl std::future::Future<Output = ZRPCResult<Vec<zrpc::ComponentState>>> + 'static
                {
                    async move {
                        use zenoh::prelude::r#async::*;
                        use zenoh::query::*;
                        use zenoh::prelude::*;

                        let selector = format!("{}*/state",#eval_path);
                        log::trace!("Find servers selector {}", selector);
                        let mut servers = Vec::new();

                        let replies = z.get(&selector).target(QueryTarget::All).res().await?;

                        while let Ok(d) = replies.recv_async().await {
                            match d.sample {
                                Ok(sample) => match sample.value.encoding {
                                    Encoding::APP_OCTET_STREAM => {
                                        let ca = zrpc::serialize::deserialize_state::<zrpc::ComponentState>(
                                            &sample.value.payload.contiguous(),
                                        )?;
                                        servers.push(ca);
                                    }
                                    _ => {
                                        return Err(ZRPCError::ZenohError(
                                            "Server information is not correctly encoded".to_string(),
                                        ))
                                    }
                                },
                                Err(e) => {
                                    return Err(ZRPCError::ZenohError(format!(
                                        "Unable to get sample from {:?}",e
                                    )))
                                }
                            }
                        }
                        Ok(servers)
                    }
                }

                #vis fn find_local_servers(
                    z : async_std::sync::Arc<zenoh::Session>
                ) -> impl std::future::Future<Output = ZRPCResult<Vec<uuid::Uuid>>> + 'static
                {
                    async move {
                        use zenoh::prelude::r#async::*;
                        use zenoh::query::*;
                        use zenoh::prelude::*;
                        use zrpc::zrpcresult::ZRPCError;


                        let servers = Self::find_servers_info(async_std::sync::Arc::clone(&z)).await?;

                        let zinfo = z.info();

                        let rid = match zinfo
                            .routers_zid()
                            .res()
                            .await
                            .collect::<Vec<ZenohId>>()
                            .first()
                        {
                            Some(head) => head.to_string().to_uppercase(),
                            None => "".to_string(),
                        };
                        if rid == "" {
                            return Ok(vec![])
                        }
                        log::trace!("Router ID is {}", rid);

                        // This is a get from in the Router Admin space
                        let selector = format!("@/router/{}", rid);

                        let mut rdata: Vec<Reply> = z.get(&selector).res().await?.into_iter().collect();

                        if rdata.is_empty() {
                            return Err(ZRPCError::NotFound);
                        }

                        let router_data = rdata.remove(0);
                        match router_data.sample {
                            Ok(sample) => match sample.value.encoding {
                                Encoding::APP_JSON => {
                                    let ri = zrpc::serialize::deserialize_router_info(
                                        &sample.value.payload.contiguous(),
                                    )?;
                                    let r: Vec<Uuid> = servers
                                        .into_iter()
                                        .filter_map(|ci| {
                                            let pid = String::from(&ci.peerid).to_uppercase();
                                            let mut it = ri.clone().sessions.into_iter();
                                            let f = it.find(|x| x.peer == pid.clone());
                                            if f.is_none() {
                                                None
                                            } else {
                                                Some(ci.uuid)
                                            }
                                        })
                                        .collect();

                                    Ok(r)
                                }
                                _ => Err(ZRPCError::ZenohError(
                                    "Router information is not encoded in JSON".to_string(),
                                )),
                            },
                            Err(e) => Err(ZRPCError::ZenohError(format!(
                                "Unable to get sample from {:?}",e
                            ))),
                        }
                    }
                }
            }
        }
    }

    // Generates the implementation of the client methods that maps to evals
    fn impl_client_eval_methods(&self) -> TokenStream2 {
        let &Self {
            client_ident,
            request_ident,
            response_ident,
            method_attrs,
            vis,
            method_idents,
            args,
            return_types,
            arg_pats,
            camel_case_idents,
            timeout,
            ..
        } = self;

        quote! {

            impl #client_ident {
                #vis fn verify_server(&self
                ) -> impl std::future::Future<Output = ZRPCResult<bool>> + '_ {
                    async move {
                        self.ch.verify_server().await
                    }
                }

                #(

                    #[allow(unused,clippy::manual_async_fn)]
                    #( #method_attrs )*
                    #vis fn #method_idents(&self, #( #args ),*)
                        -> impl std::future::Future<Output = ZRPCResult<#return_types>> + '_ {
                        let request = #request_ident::#camel_case_idents { #( #arg_pats ),* };
                        log::trace!("Sending {:?}", request);
                        async move {
                            let resp = self.ch.call_fun(request);
                            let dur = std::time::Duration::from_secs(#timeout as u64);
                            match async_std::future::timeout(dur, resp).await {
                                Ok(r) => match r {
                                    Ok(zr) => match zr {
                                            #response_ident::#camel_case_idents(msg) => std::result::Result::Ok(msg),
                                            _ => Err(ZRPCError::Unreachable),
                                        },
                                    Err(e) => Err(e),
                                },
                                Err(e) => Err(ZRPCError::TimedOut),
                            }
                        }
                    }
                )*
            }
        }
    }
}

//Converts ZServiceGenerator to actual code
impl<'a> ToTokens for ZServiceGenerator<'a> {
    fn to_tokens(&self, output: &mut TokenStream2) {
        output.extend(vec![
            self.trait_service(),
            self.struct_server(),
            self.impl_serve_for_server(),
            self.enum_request(),
            self.enum_response(),
            self.struct_client(),
            self.impl_client_new_find_servers(),
            self.impl_client_eval_methods(),
        ])
    }
}

//converts to snake_case to CamelCase, is used to convert functions name
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
