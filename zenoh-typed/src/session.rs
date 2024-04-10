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
use crate::publication::TypePublisherBuilder;
use crate::subscription::TypedSubscriberBuilder;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::Read;
use std::marker::PhantomData;
use std::sync::Arc;
use zenoh::publication::SessionPutBuilder;

use zenoh::payload::{self, Deserialize as ZDeserialize, Serialize as ZSerialize};
use zenoh::prelude::Encoding;

use zenoh::handlers::DefaultHandler;
use zenoh::prelude::r#async::*;
// use zenoh::publication::PutBuilder;
// use zenoh::subscriber::{PushMode, SubscriberBuilder};
use zenoh::Session;
use zenoh_core::{bail, zerror};

#[derive(Clone)]
pub struct CBOR;

impl<T> ZSerialize<T> for CBOR
where
    T: Serialize,
{
    type Output = Payload;

    fn serialize(self, t: T) -> Self::Output {
        let data = serde_cbor::to_vec(&t).unwrap();
        Payload::new(data)
    }
}

impl<'a, T> ZDeserialize<'a, T> for CBOR
where
    T: DeserializeOwned,
{
    type Error = serde_cbor::Error;

    fn deserialize(self, v: &'a Payload) -> Result<T, Self::Error> {
        // let mut buff = vec![];
        // let read = v.reader().read_to_end(&mut buff);
        // println!("Read: {read:?}");
        let data = serde_cbor::from_reader(v.reader())?;
        Ok(data)
    }
}

#[derive(Clone)]
pub struct SerdeSession<S>
where
    S: Send + Sync + Clone,
{
    session: Arc<Session>,
    serde: S,
}

impl<'a, S: Send + Sync + Clone> SerdeSession<S> {
    pub fn new(session: Session, serde: S) -> Self {
        Self {
            session: Arc::new(session),
            serde,
        }
    }

    pub fn put<'b: 'a, TryIntoKeyExpr, T>(
        &'a self,
        key_expr: TryIntoKeyExpr,
        value: T,
    ) -> SessionPutBuilder<'a, 'b>
    where
        S: ZSerialize<T, Output = Payload>,
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    {
        let payload = self.serde.clone().serialize(value);
        self.session
            .put(key_expr, payload)
            .encoding(Encoding::APPLICATION_CBOR)
    }

    pub fn declare_publisher<'b: 'static, TryIntoKeyExpr, T>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> TypePublisherBuilder<'a, 'b, T, S>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
        S: ZSerialize<T, Output = Payload>,
    {
        TypePublisherBuilder {
            inner: zenoh::SessionDeclarations::declare_publisher(&self.session, key_expr),
            phantom_data: PhantomData,
            serde: self.serde.clone(),
        }
    }

    pub fn declare_subscriber<'b: 'a, TryIntoKeyExpr, T>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> TypedSubscriberBuilder<'a, 'b, Typer<T, S, DefaultHandler>, T>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
        S: ZDeserialize<'a, T>,
        T: Send + Sync + 'static,
        Typer<T, S, DefaultHandler>: IntoHandler<'static, Sample>,
    {
        let handler = Typer(PhantomData, self.serde.clone(), DefaultHandler);
        let inner =
            zenoh::SessionDeclarations::declare_subscriber(&self.session, key_expr).with(handler);
        TypedSubscriberBuilder {
            inner,
            phantom_data: PhantomData,
        }
    }
}
