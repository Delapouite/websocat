pub use super::socks5_peer::SocksSocketAddr;

use super::readdebt::DebtHandling;

use std::ffi::OsString;
use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct StaticFile {
    pub uri: String,
    pub file: ::std::path::PathBuf,
    pub content_type: String,
}

extern crate http_bytes;
use http_bytes::http;

#[derive(SmartDefault, Derivative)]
#[derivative(Debug)]
pub struct Options {
    pub websocket_text_mode: bool,
    pub websocket_protocol: Option<String>,
    pub websocket_reply_protocol: Option<String>,
    pub udp_oneshot_mode: bool,
    pub udp_broadcast: bool,
    pub udp_multicast_loop: bool,
    pub udp_ttl: Option<u32>,
    pub udp_join_multicast_addr: Vec<std::net::IpAddr>,
    pub udp_join_multicast_iface_v4: Vec<std::net::Ipv4Addr>,
    pub udp_join_multicast_iface_v6: Vec<u32>,
    pub udp_reuseaddr: bool,
    pub unidirectional: bool,
    pub unidirectional_reverse: bool,
    pub max_messages: Option<usize>,
    pub max_messages_rev: Option<usize>,
    pub exit_on_eof: bool,
    pub oneshot: bool,
    pub unlink_unix_socket: bool,
    pub unix_socket_accept_from_fd: bool,
    pub exec_args: Vec<String>,
    pub ws_c_uri: String, // TODO: delete this
    pub linemode_strip_newlines: bool,
    pub linemode_strict: bool,
    pub origin: Option<String>,
    pub custom_headers: Vec<(String, Vec<u8>)>,
    pub custom_reply_headers: Vec<(String, Vec<u8>)>,
    pub websocket_version: Option<String>,
    pub websocket_dont_close: bool,
    pub websocket_ignore_zeromsg: bool,
    pub one_message: bool,
    pub no_auto_linemode: bool,
    #[default = 65536]
    pub buffer_size: usize,
    #[default = 16]
    pub broadcast_queue_len: usize,
    #[default(DebtHandling::Silent)]
    pub read_debt_handling: DebtHandling,
    pub linemode_zero_terminated: bool,
    pub restrict_uri: Option<String>,
    pub serve_static_files: Vec<StaticFile>,
    pub exec_set_env: bool,
    pub no_exit_on_zeromsg: bool,
    pub reuser_send_zero_msg_on_disconnect: bool,
    pub process_zero_sighup: bool,
    pub process_exit_sighup: bool,
    pub socks_destination: Option<SocksSocketAddr>,
    pub auto_socks5: Option<SocketAddr>,
    pub socks5_bind_script: Option<OsString>,
    pub tls_domain: Option<String>,
    #[derivative(Debug = "ignore")]
    pub pkcs12_der: Option<Vec<u8>>,
    #[derivative(Debug = "ignore")]
    pub pkcs12_passwd: Option<String>,
    pub tls_insecure: bool,

    pub headers_to_env: Vec<String>,

    pub max_parallel_conns: Option<usize>,
    pub ws_ping_interval: Option<u64>,
    pub ws_ping_timeout: Option<u64>,

    pub request_uri: Option<http::Uri>,
    pub request_method: Option<http::Method>,
    pub request_headers: Vec<(http::header::HeaderName, http::header::HeaderValue)>,

    pub autoreconnect_delay_millis: u64,

    pub ws_text_prefix: Option<String>,
    pub ws_binary_prefix: Option<String>,
    pub ws_binary_base64: bool,
    pub ws_text_base64: bool,

    /// Only affects linter
    pub asyncstdio: bool,

    pub foreachmsg_wait_reads: bool,
}
