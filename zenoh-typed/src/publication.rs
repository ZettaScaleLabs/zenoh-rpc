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

use std::future::Ready;
use std::marker::PhantomData;
use std::ops::Deref;

// use crate::traits::Serialize;
use zenoh::buffers::writer::Writer;
use zenoh::payload::{self, Deserialize as ZDeserialize, Payload, Serialize as ZSerialize};
use zenoh::prelude::QoSBuilderTrait;
use zenoh::publication::PublisherPutBuilder;
use zenoh::Result as ZResult;
use zenoh::{
    prelude::{sync::SyncResolve, Encoding, SampleKind},
    publication::{CongestionControl, Priority, Publisher, PublisherBuilder},
    sample::Locality,
};
use zenoh_codec::WCodec;
use zenoh_core::{AsyncResolve, Resolvable};

pub struct TypePublisherBuilder<'a, 'b: 'a, T, S> {
    pub(crate) inner: PublisherBuilder<'a, 'b>,
    pub(crate) phantom_data: PhantomData<T>,
    pub(crate) serde: S,
}

impl<'a, 'b, T, S> TypePublisherBuilder<'a, 'b, T, S>
where
    S: ZSerialize<T> + Clone,
{
    #[inline]
    pub fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.inner = self.inner.congestion_control(congestion_control);
        self
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn priority(mut self, priority: Priority) -> Self {
        self.inner = self.inner.priority(priority);
        self
    }

    // /// Restrict the matching subscribers that will receive the published data
    // /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    // #[zenoh_macros::unstable]
    // #[inline]
    // pub fn allowed_destination(mut self, destination: Locality) -> Self {
    //     self.inner = sel.finner.destination(destination);
    //     self
    // }
}

impl<'a, 'b, T, S> Resolvable for TypePublisherBuilder<'a, 'b, T, S>
where
    S: Send + Sync,
    T: Send + Sync,
{
    type To = ZResult<TypedPubliser<'a, T, S>>;
}

impl<'a, 'b, T, S> SyncResolve for TypePublisherBuilder<'a, 'b, T, S>
where
    S: Send + Sync,
    T: Send + Sync,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let inner = self.inner.res_sync()?;
        let publisher = TypedPubliser {
            inner,
            phantom_data: self.phantom_data,
            serde: self.serde,
        };
        Ok(publisher)
    }
}

impl<'a, 'b, T, S> AsyncResolve for TypePublisherBuilder<'a, 'b, T, S>
where
    S: Send + Sync,
    T: Send + Sync,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

pub struct TypedPubliser<'a, T, S> {
    inner: Publisher<'a>,
    phantom_data: PhantomData<T>,
    serde: S,
}

impl<'a, T, S> TypedPubliser<'a, T, S>
where
    S: ZSerialize<T, Output = Payload> + Send + Sync + Clone,
    T: Send + Sync,
{
    pub fn put(&self, payload: T) -> PublisherPutBuilder<'_> {
        let payload = self.serde.clone().serialize(payload);
        self.inner.put(payload)
    }
}
