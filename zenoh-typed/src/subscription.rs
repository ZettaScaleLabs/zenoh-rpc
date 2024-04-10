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

use std::{future::Ready, marker::PhantomData};

use zenoh::{
    handlers::IntoHandler,
    sample::Sample,
    subscriber::{Subscriber, SubscriberBuilder},
    Result as ZResult,
};
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};

#[derive(Debug)]
pub struct TypedSubscriberBuilder<'a, 'b, Handler, T> {
    pub inner: SubscriberBuilder<'a, 'b, Handler>,
    pub phantom_data: PhantomData<T>,
}

impl<'a, T, Handler> Resolvable for TypedSubscriberBuilder<'a, '_, Handler, T>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Subscriber<'a, Handler::Handler>>;
}

impl<'a, Handler, T> SyncResolve for TypedSubscriberBuilder<'a, '_, Handler, T>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.inner.res_sync()
    }
}

impl<'a, Handler, T> AsyncResolve for TypedSubscriberBuilder<'a, '_, Handler, T>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
