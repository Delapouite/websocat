//! Note: library usage is not semver/API-stable
//!
//! Type evolution of a websocat run:
//!
//! 1. `&str` - string as passed to command line
//! 2. `Specifier` - more organized representation, maybe nested
//! 3. `PeerConstructor` - a future or stream that returns one or more connections
//! 4. `Peer` - one active connection
//! 5. `Transfer` - two peers recombine into two (if bidirectional) transfers
//! 6. `Session` - a running websocat connection from one specifier to another

extern crate futures;
extern crate tokio_core;
#[macro_use]
extern crate tokio_io;
extern crate websocket;

#[macro_use]
extern crate log;

use futures::future::Future;
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};

use futures::Stream;

use std::rc::Rc;

type Result<T> = std::result::Result<T, Box<std::error::Error>>;

fn wouldblock<T>() -> std::io::Result<T> {
    Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, ""))
}
fn brokenpipe<T>() -> std::io::Result<T> {
    Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, ""))
}
fn io_other_error<E: std::error::Error + Send + Sync + 'static>(e: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}

pub use lints::ConfigurationConcern;

pub struct WebsocatConfiguration {
    pub opts: Options,
    pub s1: Rc<Specifier>,
    pub s2: Rc<Specifier>,
}

impl WebsocatConfiguration {
    pub fn serve<OE>(
        self,
        h: Handle,
        onerror: std::rc::Rc<OE>,
    ) -> Box<Future<Item = (), Error = ()>>
    where
        OE: Fn(Box<std::error::Error>) -> () + 'static,
    {
        serve(h, self.s1, self.s2, self.opts, onerror)
    }
}

#[derive(Default, Debug, Clone)]
pub struct Options {
    pub websocket_text_mode: bool,
    pub websocket_protocol: Option<String>,
    pub udp_oneshot_mode: bool,
    pub unidirectional: bool,
    pub unidirectional_reverse: bool,
    pub exit_on_eof: bool,
    pub oneshot: bool,
    pub unlink_unix_socket: bool,
    pub exec_args: Vec<String>,
    pub ws_c_uri: String,
}

#[derive(Default)]
pub struct ProgramState {
    #[cfg(all(unix, not(feature = "no_unix_stdio")))]
    stdio: stdio_peer::GlobalState,

    reuser: connection_reuse_peer::GlobalState,
}

pub struct Peer(Box<AsyncRead>, Box<AsyncWrite>);

pub type BoxedNewPeerFuture = Box<Future<Item = Peer, Error = Box<std::error::Error>>>;
pub type BoxedNewPeerStream = Box<Stream<Item = Peer, Error = Box<std::error::Error>>>;

/// For checking specifier combinations for problems
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum SpecifierType {
    Stdio,
    Reuser,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct OneSpecifierInfo {
    pub multiconnect: bool,
    pub uses_global_state: bool,
    pub typ: SpecifierType,
}

#[derive(Debug, Clone)]
pub struct SpecifierInfo {
    pub this: OneSpecifierInfo,
    pub subspecifier: Option<Box<SpecifierInfo>>,
}

impl SpecifierInfo {
    fn collect(&self) -> Vec<OneSpecifierInfo> {
        let mut r = vec![];
        r.push(self.this);
        // on newer Rust can do without cloning
        let mut ss = self.clone().subspecifier;
        while let Some(sub) = ss {
            r.push(sub.this);
            ss = sub.subspecifier;
        }
        r
    }
}

/// A parsed command line argument.
/// For example, `ws-listen:tcp-l:127.0.0.1:8080` gets parsed into
/// a `WsUpgrade(TcpListen(SocketAddr))`.
pub trait Specifier: std::fmt::Debug {
    /// Apply the specifier for constructing a "socket" or other connecting device.
    fn construct(&self, h: &Handle, ps: &mut ProgramState, opts: Rc<Options>) -> PeerConstructor;

    // Specified by `specifier_boilerplate!`:
    fn is_multiconnect(&self) -> bool;
    fn uses_global_state(&self) -> bool;
    fn get_type(&self) -> SpecifierType;

    // May be overridden by `self_0_is_subspecifier`:
    fn get_info(&self) -> SpecifierInfo {
        SpecifierInfo {
            this: self.get_info_without_subspecs(),
            subspecifier: None,
        }
    }

    // Provided:
    fn get_info_without_subspecs(&self) -> OneSpecifierInfo {
        OneSpecifierInfo {
            multiconnect: self.is_multiconnect(),
            uses_global_state: self.uses_global_state(),
            typ: self.get_type(),
        }
    }
}

impl Specifier for Rc<Specifier> {
    fn construct(&self, h: &Handle, ps: &mut ProgramState, opts: Rc<Options>) -> PeerConstructor {
        (**self).construct(h, ps, opts)
    }

    fn is_multiconnect(&self) -> bool {
        (**self).is_multiconnect()
    }
    fn get_type(&self) -> SpecifierType {
        (**self).get_type()
    }
    fn uses_global_state(&self) -> bool {
        (**self).uses_global_state()
    }

    fn get_info_without_subspecs(&self) -> OneSpecifierInfo {
        (**self).get_info_without_subspecs()
    }
    fn get_info(&self) -> SpecifierInfo {
        (**self).get_info()
    }
}

macro_rules! specifier_boilerplate {
    (singleconnect $($e:tt)*) => {
        fn is_multiconnect(&self) -> bool { false }
        specifier_boilerplate!($($e)*);
    };
    (multiconnect $($e:tt)*) => {
        fn is_multiconnect(&self) -> bool { true }
        specifier_boilerplate!($($e)*);
    };
    (no_subspec $($e:tt)*) => {
        specifier_boilerplate!($($e)*);
    };
    (has_subspec $($e:tt)*) => {
        specifier_boilerplate!($($e)*);
    };
    (typ=$tn:ident $($e:tt)*) => {
        fn get_type(&self) -> $crate::SpecifierType { $crate::SpecifierType::$tn }
        specifier_boilerplate!($($e)*);
    };
    () => {
    };
    (globalstate $($e:tt)*) => {
        fn uses_global_state(&self) -> bool { true }
        specifier_boilerplate!($($e)*);
    };
    (noglobalstate $($e:tt)*) => {
        fn uses_global_state(&self) -> bool { false }
        specifier_boilerplate!($($e)*);
    };
}

macro_rules! self_0_is_subspecifier {
    (...) => {
        fn get_info(&self) -> $crate::SpecifierInfo {
            $crate::SpecifierInfo {
                this: self.get_info_without_subspecs(),
                subspecifier: Some(Box::new(self.0.get_info())),
            }
        }
    };
    (proxy_is_multiconnect) => {
        self_0_is_subspecifier!(...);
        fn is_multiconnect(&self) -> bool { self.0.is_multiconnect() }
    };
}

pub mod lints;
mod my_copy;

#[cfg(all(unix, not(feature = "no_unix_stdio")))]
pub mod stdio_peer;

pub mod connection_reuse_peer;
pub mod file_peer;
pub mod mirror_peer;
pub mod net_peer;
pub mod reconnect_peer;
pub mod stdio_threaded_peer;
pub mod trivial_peer;
pub mod ws_client_peer;
pub mod ws_peer;
pub mod ws_server_peer;

#[cfg(feature = "tokio-process")]
pub mod process_peer;

#[cfg(unix)]
pub mod unix_peer;

pub mod specparse;

pub enum PeerConstructor {
    ServeOnce(BoxedNewPeerFuture),
    ServeMultipleTimes(BoxedNewPeerStream),
}

impl PeerConstructor {
    pub fn map<F: 'static>(self, f: F) -> Self
    where
        F: FnMut(Peer) -> BoxedNewPeerFuture,
    {
        use PeerConstructor::*;
        match self {
            ServeOnce(x) => ServeOnce(Box::new(x.and_then(f)) as BoxedNewPeerFuture),
            ServeMultipleTimes(s) => {
                ServeMultipleTimes(Box::new(s.and_then(f)) as BoxedNewPeerStream)
            }
        }
    }

    pub fn get_only_first_conn(self) -> BoxedNewPeerFuture {
        use PeerConstructor::*;
        match self {
            ServeMultipleTimes(stre) => Box::new(
                stre.into_future()
                    .map(move |(std_peer, _)| {
                        let peer2 = std_peer.expect("Nowhere to connect it");
                        peer2
                    })
                    .map_err(|(e, _)| e),
            ) as BoxedNewPeerFuture,
            ServeOnce(future) => future,
        }
    }
}

/// A `Read` utility to deal with partial reads
#[derive(Default)]
pub struct ReadDebt(pub Option<Vec<u8>>);

impl ReadDebt {
    pub fn process_message(
        &mut self,
        buf: &mut [u8],
        buf_in: &[u8],
    ) -> std::result::Result<usize, std::io::Error> {
        assert_eq!(self.0, None);
        let l = buf_in.len().min(buf.len());
        buf[..l].copy_from_slice(&buf_in[..l]);

        if l < buf_in.len() {
            self.0 = Some(buf_in[l..].to_vec());
        }

        Ok(l)
    }
    pub fn check_debt(
        &mut self,
        buf: &mut [u8],
    ) -> Option<std::result::Result<usize, std::io::Error>> {
        if let Some(debt) = self.0.take() {
            Some(self.process_message(buf, debt.as_slice()))
        } else {
            None
        }
    }
}

pub fn once(x: BoxedNewPeerFuture) -> PeerConstructor {
    PeerConstructor::ServeOnce(x)
}
pub fn multi(x: BoxedNewPeerStream) -> PeerConstructor {
    PeerConstructor::ServeMultipleTimes(x)
}

pub fn peer_err<E: std::error::Error + 'static>(e: E) -> BoxedNewPeerFuture {
    Box::new(futures::future::err(Box::new(e) as Box<std::error::Error>)) as BoxedNewPeerFuture
}
pub fn peer_err_s<E: std::error::Error + 'static>(e: E) -> BoxedNewPeerStream {
    Box::new(futures::stream::iter_result(vec![
        Err(Box::new(e) as Box<std::error::Error>),
    ])) as BoxedNewPeerStream
}
pub fn peer_strerr(e: &str) -> BoxedNewPeerFuture {
    let q: Box<std::error::Error> = From::from(e);
    Box::new(futures::future::err(q)) as BoxedNewPeerFuture
}
pub fn box_up_err<E: std::error::Error + 'static>(e: E) -> Box<std::error::Error> {
    Box::new(e) as Box<std::error::Error>
}

impl Peer {
    fn new<R: AsyncRead + 'static, W: AsyncWrite + 'static>(r: R, w: W) -> Self {
        Peer(
            Box::new(r) as Box<AsyncRead>,
            Box::new(w) as Box<AsyncWrite>,
        )
    }
}

pub use specparse::boxup;
pub use specparse::spec;

pub fn peer_from_str(
    ps: &mut ProgramState,
    handle: &Handle,
    opts: Rc<Options>,
    s: &str,
) -> PeerConstructor {
    let spec = match spec(s) {
        Ok(x) => x,
        Err(e) => return once(Box::new(futures::future::err(e)) as BoxedNewPeerFuture),
    };
    spec.construct(handle, ps, opts)
}

pub struct Transfer {
    from: Box<AsyncRead>,
    to: Box<AsyncWrite>,
}
pub struct Session(Transfer, Transfer, Rc<Options>);

impl Session {
    pub fn run(self) -> Box<Future<Item = (), Error = Box<std::error::Error>>> {
        let f1 = my_copy::copy(self.0.from, self.0.to, true);
        let f2 = my_copy::copy(self.1.from, self.1.to, true);
        let f1 = f1.map(|(_, r, mut w)| {
            info!("Forward finished");
            let _ = w.shutdown();
            std::mem::drop(r);
            std::mem::drop(w);
        });
        let f2 = f2.map(|(_, r, mut w)| {
            info!("Reverse finished");
            let _ = w.shutdown();
            std::mem::drop(r);
            std::mem::drop(w);
        });
        let (unif, unir, eeof) = (
            self.2.unidirectional,
            self.2.unidirectional_reverse,
            self.2.exit_on_eof,
        );
        type Ret = Box<Future<Item = (), Error = Box<std::error::Error>>>;
        match (unif, unir, eeof) {
            (false, false, false) => Box::new(
                f1.join(f2)
                    .map(|(_, _)| {
                        info!("Finished");
                    })
                    .map_err(|x| Box::new(x) as Box<std::error::Error>),
            ) as Ret,
            (false, false, true) => Box::new(
                f1.select(f2)
                    .map(|(_, _)| {
                        info!("One of directions finished");
                    })
                    .map_err(|(x, _)| Box::new(x) as Box<std::error::Error>),
            ) as Ret,
            (true, false, _) => Box::new({
                ::std::mem::drop(f2);
                f1.map_err(|x| Box::new(x) as Box<std::error::Error>)
            }) as Ret,
            (false, true, _) => Box::new({
                ::std::mem::drop(f1);
                f2.map_err(|x| Box::new(x) as Box<std::error::Error>)
            }) as Ret,
            (true, true, _) => Box::new({
                // Just open connection and close it.
                ::std::mem::drop(f1);
                ::std::mem::drop(f2);
                futures::future::ok(())
            }) as Ret,
        }
    }
    pub fn new(peer1: Peer, peer2: Peer, opts: Rc<Options>) -> Self {
        Session(
            Transfer {
                from: peer1.0,
                to: peer2.1,
            },
            Transfer {
                from: peer2.0,
                to: peer1.1,
            },
            opts,
        )
    }
}

pub fn serve<S1, S2, OE>(
    h: Handle,
    s1: S1,
    s2: S2,
    opts: Options,
    onerror: std::rc::Rc<OE>,
) -> Box<Future<Item = (), Error = ()>>
where
    S1: Specifier + 'static,
    S2: Specifier + 'static,
    OE: Fn(Box<std::error::Error>) -> () + 'static,
{
    info!("Serving {:?} to {:?} with {:?}", s1, s2, opts);
    let mut ps = ProgramState::default();

    use PeerConstructor::{ServeMultipleTimes, ServeOnce};

    let h1 = h.clone();
    let h2 = h.clone();

    let e1 = onerror.clone();
    let e2 = onerror.clone();
    let e3 = onerror.clone();

    let opts1 = Rc::new(opts);
    let opts2 = opts1.clone();

    let mut left = s1.construct(&h, &mut ps, opts1);

    if opts2.oneshot {
        left = PeerConstructor::ServeOnce(left.get_only_first_conn());
    }

    let prog = match left {
        ServeMultipleTimes(stream) => {
            let runner = stream
                .map(move |peer1| {
                    let opts3 = opts2.clone();
                    let e1_1 = e1.clone();
                    h1.spawn(
                        s2.construct(&h1, &mut ps, opts2.clone())
                            .get_only_first_conn()
                            .and_then(move |peer2| {
                                let s = Session::new(peer1, peer2, opts3);
                                s.run()
                            })
                            .map_err(move |e| e1_1(e)),
                    )
                })
                .for_each(|()| futures::future::ok(()));
            Box::new(runner.map_err(move |e| e2(e))) as Box<Future<Item = (), Error = ()>>
        }
        ServeOnce(peer1c) => {
            let runner = peer1c.and_then(move |peer1| {
                let right = s2.construct(&h2, &mut ps, opts2.clone());
                let fut = right.get_only_first_conn();
                fut.and_then(move |peer2| {
                    let s = Session::new(peer1, peer2, opts2);
                    s.run().map(|()| {
                        ::std::mem::drop(ps)
                        // otherwise ps will be dropped sooner
                        // and stdin/stdout may become blocking sooner
                    })
                })
            });
            Box::new(runner.map_err(move |e| e3(e))) as Box<Future<Item = (), Error = ()>>
        }
    };
    prog
}
