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

use zenoh::{
    encoding::Decoder,
    handlers::{Callback, Dyn, IntoCallbackReceiverPair},
    sample::Sample,
    value::Value,
};

// Pierre's magic
pub struct Typer<T: Send + Sync, D, Handler>(pub PhantomData<T>, pub PhantomData<D>, pub Handler);

impl<'a, T: 'a, D, Handler> IntoCallbackReceiverPair<'a, Sample> for Typer<T, D, Handler>
where
    Handler: IntoCallbackReceiverPair<'a, T>,
    D: Decoder<T>,
    T: Send + Sync,
{
    type Receiver = Handler::Receiver;
    fn into_cb_receiver_pair(self) -> (Callback<'a, Sample>, Self::Receiver) {
        let (cb, receiver) = self.2.into_cb_receiver_pair();
        (
            Dyn::new(move |z| cb(D::decode(&z.value).expect("Unable to decode"))),
            receiver,
        )
    }
}
