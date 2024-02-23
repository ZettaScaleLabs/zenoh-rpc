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

use std::marker::PhantomData;

// use crate::traits::Serialize;
use zenoh::buffers::writer::Writer;
use zenoh::Result as ZResult;
use zenoh::{
    prelude::{sync::SyncResolve, Encoding, SampleKind},
    publication::{
        CongestionControl, Priority, Publication, Publisher, PublisherBuilder, PutBuilder,
    },
    sample::Locality,
};
use zenoh_codec::WCodec;
use zenoh_core::Resolvable;

// pub struct TypedPutBuilder<'a, 'b, T, C> {
//     publisher: PublisherBuilder<'a, 'b>,
//     value: T,
//     codec: C,
//     kind: SampleKind,
//     encoding: Encoding,
// }

// impl<T, C> TypedPutBuilder<'_, '_, T, C> {
//     #[inline]
//     pub fn encoding<IntoEncoding>(mut self, encoding: IntoEncoding) -> Self
//     where
//         IntoEncoding: Into<Encoding>,
//     {
//         self.encoding = encoding.into();
//         self
//     }
//     /// Change the `congestion_control` to apply when routing the data.
//     #[inline]
//     pub fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
//         self.publisher = self.publisher.congestion_control(congestion_control);
//         self
//     }

//     /// Change the priority of the written data.
//     #[inline]
//     pub fn priority(mut self, priority: Priority) -> Self {
//         self.publisher = self.publisher.priority(priority);
//         self
//     }

//     /// Restrict the matching subscribers that will receive the published data
//     /// to the ones that have the given [`Locality`](crate::prelude::Locality).
//     #[zenoh_macros::unstable]
//     #[inline]
//     pub fn allowed_destination(mut self, destination: Locality) -> Self {
//         self.publisher = self.publisher.allowed_destination(destination);
//         self
//     }

//     pub fn kind(mut self, kind: SampleKind) -> Self {
//         self.kind = kind;
//         self
//     }
// }

// impl<T, C> Resolvable for TypedPutBuilder<'_, '_, T, C> {
//     type To = ZResult<()>;
// }

// impl<T, C> SyncResolve for TypedPutBuilder<'_, '_, T, C> {
//     #[inline]
//     fn res_sync(self) -> <Self as Resolvable>::To {
//         let TypedPutBuilder {
//             publisher,
//             value,
//             codec,
//             kind,
//             encoding,
//         } = self;

//         let key_expr = publisher.key_expr?;
//         log::trace!("write({:?}, [...])", &key_expr);
//         let primitives = zread!(publisher.session.state)
//             .primitives
//             .as_ref()
//             .unwrap()
//             .clone();
//         let timestamp = publisher.session.runtime.new_timestamp();

//         if publisher.destination != Locality::SessionLocal {
//             primitives.send_push(Push {
//                 wire_expr: key_expr.to_wire(&publisher.session).to_owned(),
//                 ext_qos: ext::QoSType::new(
//                     publisher.priority.into(),
//                     publisher.congestion_control,
//                     false,
//                 ),
//                 ext_tstamp: None,
//                 ext_nodeid: ext::NodeIdType::default(),
//                 payload: match kind {
//                     SampleKind::Put => PushBody::Put(Put {
//                         timestamp,
//                         encoding: value.encoding.clone(),
//                         ext_sinfo: None,
//                         #[cfg(feature = "shared-memory")]
//                         ext_shm: None,
//                         ext_unknown: vec![],
//                         payload: value.payload.clone(),
//                     }),
//                     SampleKind::Delete => PushBody::Del(Del {
//                         timestamp,
//                         ext_sinfo: None,
//                         ext_unknown: vec![],
//                     }),
//                 },
//             });
//         }
//         if publisher.destination != Locality::Remote {
//             let data_info = DataInfo {
//                 kind,
//                 encoding: Some(value.encoding),
//                 timestamp,
//                 ..Default::default()
//             };

//             publisher.session.handle_data(
//                 true,
//                 &key_expr.to_wire(&publisher.session),
//                 Some(data_info),
//                 value.payload,
//             );
//         }
//         Ok(())
//     }
// }

// impl<T, C> AsyncResolve for TypedPutBuilder<'_, '_, T, C> {
//     type Future = Ready<Self::To>;

//     fn res_async(self) -> Self::Future {
//         std::future::ready(self.res_sync())
//     }
// }

pub struct TypePublisherBuilder<'a, 'b: 'a, T, C> {
    inner: PublisherBuilder<'a, 'b>,
    phantom_data: PhantomData<T>,
    codec: C,
}

impl<'a, 'b, T, C> TypePublisherBuilder<'a, 'b, T, C> {
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

// impl SyncResolve for TypePublisherBuilder {
//     fn res_sync(self) -> <Self as Resolvable>::To {

//     }
// }

pub struct TypedPublisher<'a, T, C> {
    inner: Publisher<'a>,
    phantom_data: PhantomData<T>,
    codec: C,
}

// impl<'a, T, C> TypedPublisher<'a, T, C>
// where
//     C: WCodec<T>,
// {
//     pub fn put(&self, value: T) -> zenoh::Result<Publication> {
//         let serialized_data = value.try_serialize()?;
//         Ok(self.inner.put(serialized_data))
//     }

//     pub fn write(&self, kind: SampleKind, value: T) -> zenoh::Result<Publication> {
//         let serialized_data = value.try_serialize()?;
//         Ok(self.inner.write(kind, serialized_data))
//     }
// }
