extern crate websocket;

use self::websocket::WebSocketError;
use futures::future::Future;
use futures::stream::Stream;

use std::cell::RefCell;
use std::rc::Rc;

use self::websocket::server::upgrade::async::IntoWs;

use super::ws_peer::{Mode1, PeerForWs, WsReadWrapper, WsWriteWrapper};
use super::{box_up_err, io_other_error, BoxedNewPeerFuture, Peer};
use super::{Handle, Options, PeerConstructor, ProgramState, Specifier};

#[derive(Debug)]
pub struct WsUpgrade<T: Specifier>(pub T);
impl<T: Specifier> Specifier for WsUpgrade<T> {
    fn construct(&self, h: &Handle, ps: &mut ProgramState, opts: Rc<Options>) -> PeerConstructor {
        let mode1 = if opts.websocket_text_mode {
            Mode1::Text
        } else {
            Mode1::Binary
        };
        let inner = self.0.construct(h, ps, opts);
        inner.map(move |p| ws_upgrade_peer(p, mode1))
    }
    specifier_boilerplate!(typ=Other noglobalstate has_subspec);
    self_0_is_subspecifier!(proxy_is_multiconnect);
}

pub fn ws_upgrade_peer(inner_peer: Peer, mode1: Mode1) -> BoxedNewPeerFuture {
    let step1 = PeerForWs(inner_peer);
    let step2: Box<
        Future<Item = self::websocket::server::upgrade::async::Upgrade<_>, Error = _>,
    > = step1.into_ws();
    let step3 = step2
        .map_err(|(_, _, _, e)| WebSocketError::IoError(io_other_error(e)))
        .and_then(move |x| {
            info!("Incoming connection to websocket: {}", x.request.subject.1);
            debug!("{:?}", x.request);
            debug!("{:?}", x.headers);
            x.accept().map(move |(y, headers)| {
                debug!("{:?}", headers);
                info!("Upgraded");
                let (sink, stream) = y.split();
                let mpsink = Rc::new(RefCell::new(sink));

                let ws_str = WsReadWrapper {
                    s: stream,
                    pingreply: mpsink.clone(),
                    debt: Default::default(),
                };
                let ws_sin = WsWriteWrapper(mpsink, mode1);

                let ws = Peer::new(ws_str, ws_sin);
                ws
            })
        });
    let step4 = step3.map_err(box_up_err);
    Box::new(step4) as BoxedNewPeerFuture
}
