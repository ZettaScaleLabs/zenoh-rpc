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
    handlers::{Callback, Dyn},
    payload::Deserialize,
    prelude::IntoHandler,
    sample::Sample,
};

// Pierre's magic
pub struct Typer<T, S, Handler>(pub PhantomData<T>, pub S, pub Handler);

impl<'a, T, S, Handler> IntoHandler<'a, Sample> for Typer<T, S, Handler>
where
    Handler: IntoHandler<'a, T>,
    for<'b> S: Send + Sync + Clone + Deserialize<'b, T> + 'a,
    T: 'a + Send + Sync,
    for<'b> <S as zenoh::payload::Deserialize<'b, T>>::Error: std::fmt::Debug,
{
    type Handler = Handler::Handler;
    fn into_handler(self) -> (Callback<'a, Sample>, Self::Handler) {
        let (cb, receiver) = self.2.into_handler();
        (
            Dyn::new(
                move |z: Sample| match self.1.clone().deserialize(z.payload()) {
                    Ok(d) => cb(d),
                    Err(e) => log::error!("Cannot deserialize: {e:?}"),
                },
            ),
            receiver,
        )
    }
}

// impl<T, S, Handler> IntoHandler<'static, Sample> for Typer<T, S, Handler>
// where
//     Handler: IntoHandler<'static, T>,
//     for<'b> S: Send + Sync + Clone + Deserialize<'b, T>,
//     T: Send + Sync + 'static,
// {
//     type Handler = Handler::Handler;
//     fn into_handler(self) -> (Callback<'static, Sample>, Self::Handler) {
//         let (cb, receiver) = self.2.into_handler();
//         let de = self.1.clone();
//         (
//             Dyn::new(
//                 move |z: Sample| match de.deserialize(z.payload()) {
//                     Ok(d) => cb(d),
//                     Err(_) => (),
//                 },
//             ),
//             receiver,
//         )
//     }
// }
