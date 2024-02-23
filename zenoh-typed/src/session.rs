/*********************************************************************************
* Copyright (c) 2023 ZettaScale Technology
*
* This program and the accompanying materials are made available under the
* terms of the Eclipse Public License 2.0 which is available at
* http://www.eclipse.org/legal/epl-2.0, or the Apache Software License 2.0
* which is available at https://www.apache.org/licenses/LICENSE-2.0.
*
* SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
* Contributors:
*   ZettaScale PaaS Team, <paas@zettascale.tech>
*********************************************************************************/

use crate::handlers::Typer;
use crate::subscription::TypedSubscriberBuilder;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::marker::PhantomData;

use zenoh::encoding::iana::IanaEncoding;
use zenoh::encoding::{Decoder, Encoder};

use zenoh::handlers::DefaultHandler;
use zenoh::prelude::r#async::*;
use zenoh::publication::PutBuilder;
use zenoh::subscriber::{PushMode, SubscriberBuilder};
use zenoh::Session;
use zenoh_core::{bail, zerror};
use zenoh_protocol::network::declare::Mode;

pub struct CBOREncoder;

impl<T> Encoder<T> for CBOREncoder
where
    T: Serialize,
{
    fn encode(t: T) -> Value {
        let data = serde_cbor::to_vec(&t).expect("Unable to serialize");
        Value::new(data.into()).encoding(IanaEncoding::APPLICATION_CBOR)
    }
}

impl<T> Decoder<T> for CBOREncoder
where
    T: DeserializeOwned,
{
    fn decode(v: &Value) -> zenoh::Result<T> {
        if v.encoding != IanaEncoding::APPLICATION_CBOR {
            bail!(
                "Cannot decode! expecting {:?} found {:?}",
                IanaEncoding::APPLICATION_CBOR,
                v.encoding
            );
        }

        let data = serde_cbor::from_slice(&v.payload.contiguous()).expect("Cannot deserialize!");
        Ok(data)
    }
}

pub trait TypedSession<D, E> {
    // type MyEncode = E;
    // type MyDecoder;

    fn put<'a, 'b: 'a, TryIntoKeyExpr, T>(
        &'a self,
        key_expr: TryIntoKeyExpr,
        value: T,
    ) -> PutBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
        E: Encoder<T>;

    fn declare_subscriber<'a, 'b: 'a, TryIntoKeyExpr, T>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> TypedSubscriberBuilder<'a, 'b, PushMode, Typer<T, D, DefaultHandler>, T>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
        D: Decoder<T>,
        T: Send + Sync + 'static;

    // fn declare_publisher<'a, 'b: 'a, TryIntoKeyExpr, Codec, T>(
    //     &'a self,
    //     key_expr: TryIntoKeyExpr,
    //     codec: Codec
    // ) -> TypePublisherBuilder<'a, 'b, T, Codec>
    // where
    //     TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
    //     <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    //     Codec: WCodec<T, Self::TypedWriter>;

    // fn declare_publisher<'a, 'b, TryIntoKeyExpr, Serializable>(
    //     &'a self,
    //     key_expr: TryIntoKeyExpr,
    // ) -> TypePublisherBuilder<'a, 'b, Serializable>
    // where
    //     TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
    //     <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    //     Serializable: Serialize;
}

impl<E, D> TypedSession<D, E> for Session {
    fn put<'a, 'b: 'a, TryIntoKeyExpr, T>(
        &'a self,
        key_expr: TryIntoKeyExpr,
        value: T,
    ) -> PutBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
        E: Encoder<T>,
    {
        let raw_value = E::encode(value);
        self.put(key_expr, raw_value)
    }

    fn declare_subscriber<'a, 'b: 'a, TryIntoKeyExpr, T>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> TypedSubscriberBuilder<'a, 'b, PushMode, Typer<T, D, DefaultHandler>, T>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
        D: Decoder<T>,
        T: Send + Sync + 'static,
    {
        let handler = Typer(PhantomData, PhantomData, DefaultHandler);
        let inner = zenoh::SessionDeclarations::declare_subscriber(self, key_expr).with(handler);
        TypedSubscriberBuilder {
            inner,
            phantom_data: PhantomData,
        }
    }

    // fn declare_subscriber<'a, 'b: 'a, TryIntoKeyExpr, T>(
    //     &'a self,
    //     key_expr: TryIntoKeyExpr,
    // ) -> TypedSubscriberBuilder<'a, 'b, PushMode, DefaultTypedHandler, T>
    // where
    //     TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
    //     <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    //     T: Decoder<T>,
    // {
    //     todo!()
    // }

    // fn declare_publisher<'a, 'b: 'a, TryIntoKeyExpr, Codec, T>(
    //     &'a self,
    //     key_expr: TryIntoKeyExpr,
    //     codec: Codec
    // ) -> TypePublisherBuilder<'a, 'b, T, Codec>ss
    // where
    //     TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
    //     <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    //     Codec: WCodec<T, Self::TypedWriter> {
    //         todo!()
    //     }

    // fn declare_publisher<'a, 'b, TryIntoKeyExpr, Serializable>(
    //     &'a self,
    //     key_expr: TryIntoKeyExpr,
    // ) -> TypePublisherBuilder<'a, 'b, Serializable>
    // where
    //     TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
    //     <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    //     Serializable: Serialize {
    //         todo!()
    //     }
}
