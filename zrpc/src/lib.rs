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
//#![feature(associated_type_bounds)]
#![allow(clippy::upper_case_acronyms)]

pub use futures::stream::{AbortHandle, AbortRegistration, Abortable, Aborted};

pub mod zchannel;
pub mod zrchannel;
pub use zchannel::ZClientChannel;

pub mod types;
pub use types::*;

pub mod request;
pub mod response;
pub mod serialize;
pub mod status;
pub mod zrpcresult;
pub mod result;

use zenoh::prelude::ZenohId;
use zrpcresult::ZRPCResult;

/// Trait to be implemented by services
pub trait ZServe<Req>: Sized + Clone {
    /// Type of the response
    type Resp;

    fn instance_uuid(&self) -> ZenohId;

    /// Connects to Zenoh, do nothing in this case, state is HALTED
    #[allow(clippy::type_complexity)]
    fn connect(
        &self,
    ) -> ::core::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = ZRPCResult<(
                        futures::stream::AbortHandle,
                        async_std::task::JoinHandle<Result<ZRPCResult<()>, Aborted>>,
                    )>,
                > + '_,
        >,
    >;

    /// Authenticates to Zenoh, state changes to INITIALIZING
    #[allow(clippy::type_complexity)]
    fn initialize(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    // Registers, state changes to REGISTERED
    #[allow(clippy::type_complexity)]
    fn register(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    // // // Announce, state changes to ANNOUNCED
    // // //fn announce(&self);

    /// The actual run loop serving the queriable
    #[allow(clippy::type_complexity)]
    fn run(&self) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    /// State changes to SERVING, calls serve on a task::spawn, returns a stop sender and the serve task handle
    #[allow(clippy::type_complexity)]
    fn start(
        &self,
    ) -> ::core::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = ZRPCResult<(
                        futures::stream::AbortHandle,
                        async_std::task::JoinHandle<Result<ZRPCResult<()>, Aborted>>,
                    )>,
                > + '_,
        >,
    >;

    /// Starts serving all requests
    #[allow(clippy::type_complexity)]
    fn serve(
        &self,
        barrier: async_std::sync::Arc<async_std::sync::Barrier>,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    /// State changes to REGISTERED, will stop serve/work
    #[allow(clippy::type_complexity)]
    fn stop(
        &self,
        stop: futures::stream::AbortHandle,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    // state changes to HALTED
    #[allow(clippy::type_complexity)]
    fn unregister(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    /// removes state from Zenoh
    #[allow(clippy::type_complexity)]
    fn disconnect(
        &self,
        stop: futures::stream::AbortHandle,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;
}
