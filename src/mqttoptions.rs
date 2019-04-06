//! Options to set mqtt client behaviour
use mqtt311::LastWill;
use std::time::Duration;

/// Control how the connection is re-established if it is lost.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ReconnectOptions {
    /// Don't automatically reconnect
    Never,
    /// Reconnect automatically if the initial connection was successful.
    ///
    /// Before a reconnection attempt, sleep for the specified amount of time (in seconds).
    AfterFirstSuccess(u64),
    /// Always reconnect automatically.
    ///
    /// Before a reconnection attempt, sleep for the specified amount of time (in seconds).
    Always(u64),
}

/// Client authentication option for mqtt connect packet
#[derive(Clone, Debug)]
pub enum SecurityOptions {
    /// No authentication.
    None,
    /// Use the specified `(username, password)` tuple to authenticate.
    UsernamePassword(String, String),
    #[cfg(feature = "jwt")]
    /// Authenticate against a Google Cloud IoT Core project with the triple
    /// `(project name, private_key.der to sign jwt, expiry in seconds)`.
    GcloudIot(String, Vec<u8>, i64),
}

#[derive(Clone, Debug)]
pub enum ConnectionMethod {
    /// Plain text connection
    Tcp,
    /// Encrypted connection. (ca data, optional client cert and key data)
    Tls(Vec<u8>, Option<(Vec<u8>, Vec<u8>)>),
}

/// Mqtt through http proxy
#[derive(Clone, Debug)]
pub enum Proxy {
    /// No tunnel
    None,
    /// Tunnel through a proxy using http connect.
    /// (Proxy name, Port, priave_key.der to sign jwt, Expiry in seconds)
    HttpConnect(String, u16, Vec<u8>, i64),
}

/// Mqtt options
#[derive(Clone, Debug)]
pub struct MqttOptions {
    /// broker address that you want to connect to
    host: String,
    port: u16,
    /// keep alive time to send pingreq to broker when the connection is idle
    keep_alive: Duration,
    /// clean (or) persistent session
    clean_session: bool,
    /// client identifier
    client_id: String,
    /// connection method
    connection_method: ConnectionMethod,
    /// proxy
    proxy: Proxy,
    /// reconnection options
    reconnect: ReconnectOptions,
    /// security options
    security: SecurityOptions,
    /// maximum packet size
    max_packet_size: usize,
    /// last will and testament
    last_will: Option<LastWill>,
    /// request (publish, subscribe) channel capacity
    request_channel_capacity: usize,
    /// notification channel capacity
    notification_channel_capacity: usize,
    /// rate limit for outgoing messages (no. of messages per second)
    outgoing_ratelimit: Option<u64>,
    /// rate limit applied after queue size limit (size, sleep time after every message)
    outgoing_queuelimit: (usize, Duration)
}

impl Default for MqttOptions {
    fn default() -> Self {
        MqttOptions {
            host: "127.0.0.1".into(),
            port: 1883,
            keep_alive: Duration::from_secs(30),
            clean_session: true,
            client_id: "test-client".into(),
            connection_method: ConnectionMethod::Tcp,
            proxy: Proxy::None,
            reconnect: ReconnectOptions::AfterFirstSuccess(10),
            security: SecurityOptions::None,
            max_packet_size: 256 * 1024,
            last_will: None,
            request_channel_capacity: 10,
            notification_channel_capacity: 10,
            outgoing_ratelimit: None,
            outgoing_queuelimit: (100, Duration::from_secs(3)),
        }
    }
}

impl MqttOptions {
    /// New mqtt options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set client id
    pub fn set_client_id<T>(&mut self, client_id: T) -> &mut Self
    where
        T: Into<String>,
    {
        let client_id = client_id.into();
        if client_id.starts_with(' ') || client_id.is_empty() {
            panic!("Invalid client id")
        }

        self.client_id = client_id;
        self
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Set port number
    pub fn set_port(&mut self, port: u16) -> &mut Self {
        self.port = port;
        self
    }

    /// Port number
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Set port number
    pub fn set_host<T>(&mut self, host: T) -> &mut Self
    where
        T: Into<String>,
    {
        self.host = host.into();
        self
    }

    /// Broker address
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Set number of seconds after which client should ping the broker
    /// if there is no other data exchange
    pub fn set_keep_alive(&mut self, secs: u64) -> &mut Self {
        if secs < 10 {
            panic!("Keep alives should be >= 10 secs");
        }

        self.keep_alive = Duration::from_secs(secs);
        self
    }

    /// Keep alive time
    pub fn keep_alive(&self) -> Duration {
        self.keep_alive
    }

    /// Set packet size limit (in Kilo Bytes)
    pub fn set_max_packet_size(&mut self, sz: usize) -> &mut Self {
        self.max_packet_size = sz * 1024;
        self
    }

    /// Maximum packet size
    pub fn max_packet_size(&self) -> usize {
        self.max_packet_size
    }

    /// `clean_session = true` removes all the state from queues & instructs the broker
    /// to clean all the client state when client disconnects.
    ///
    /// When set `false`, broker will hold the client state and performs pending
    /// operations on the client when reconnection with same `client_id`
    /// happens. Local queue state is also held to retransmit packets after reconnection.
    pub fn set_clean_session(&mut self, clean_session: bool) -> &mut Self {
        self.clean_session = clean_session;
        self
    }

    /// Clean session
    pub fn clean_session(&self) -> bool {
        self.clean_session
    }

    /// Set how to connect to a MQTT Broker (either TCP or Tls)
    pub fn set_connection_method(&mut self, opts: ConnectionMethod) -> &mut Self {
        self.connection_method = opts;
        self
    }

    pub fn connection_method(&self) -> ConnectionMethod {
        self.connection_method.clone()
    }

    pub fn set_proxy(&mut self, proxy: Proxy) -> &mut Self {
        self.proxy = proxy;
        self
    }

    pub fn proxy(&self) -> Proxy {
        self.proxy.clone()
    }

    /// Time interval after which client should retry for new
    /// connection if there are any disconnections. By default, no retry will happen
    pub fn set_reconnect_opts(&mut self, opts: ReconnectOptions) -> &mut Self {
        self.reconnect = opts;
        self
    }

    /// Reconnection options
    pub fn reconnect_opts(&self) -> ReconnectOptions {
        self.reconnect
    }

    /// Set security option
    /// Supports username-password auth, tls client cert auth, gcloud iotcore jwt auth
    pub fn set_security_opts(&mut self, opts: SecurityOptions) -> &mut Self {
        self.security = opts;
        self
    }

    /// Security options
    pub fn security_opts(&self) -> SecurityOptions {
        self.security.clone()
    }

    /// Set last will and testament
    pub fn set_last_will(&mut self, last_will: LastWill) -> &mut Self {
        self.last_will = Some(last_will);
        self
    }

    /// Last will and testament
    pub fn last_will(&self) -> Option<mqtt311::LastWill> {
        self.last_will.clone()
    }

    /// Set notification channel capacity
    pub fn set_notification_channel_capacity(&mut self, capacity: usize) -> &mut Self {
        self.notification_channel_capacity = capacity;
        self
    }

    /// Notification channel capacity
    pub fn notification_channel_capacity(&self) -> usize {
        self.notification_channel_capacity
    }

    /// Set request channel capacity
    pub fn set_request_channel_capacity(&mut self, capacity: usize) -> &mut Self {
        self.request_channel_capacity = capacity;
        self
    }

    /// Request channel capacity
    pub fn request_channel_capacity(&self) -> usize {
        self.request_channel_capacity
    }
    
    /// Enables throttling and sets outoing message rate to the specified 'rate'
    pub fn set_outgoing_ratelimit(&mut self, rate: u64) -> &mut Self {
        if rate == 0 {
            panic!("zero rate is not allowed");
        }

        self.outgoing_ratelimit = Some(rate);
        self
    }

    /// Outgoing message rate
    pub fn outgoing_ratelimit(&self) -> Option<u64> {
        self.outgoing_ratelimit
    }

    /// Sleeps for the 'delay' about of time before sending the next message if the 
    /// specified 'queue_size's are hit
    pub fn set_outgoing_queuelimit(&mut self, queue_size: usize, delay: Duration) -> &mut Self {
        if queue_size == 0 {
            panic!("zero queue size is not allowed")
        }

        self.outgoing_queuelimit = (queue_size, delay);
        self
    }

    /// Outgoing queue limit
    pub fn outgoing_queuelimit(&self) -> (usize, Duration) {
        self.outgoing_queuelimit
    }

    /// Create MqttOptionsBuilder
    pub fn builder() -> MqttOptionsBuilder {
        MqttOptionsBuilder::new()
    }
}

pub struct MqttOptionsBuilder {
    inner: MqttOptions,
}

impl MqttOptionsBuilder {

    fn new() -> Self {
        Self {
            inner: MqttOptions::new(),
        }
    }

    /// Set client id
    pub fn client_id<T>(mut self, client_id: T) -> Self
    where
        T: Into<String>,
    {
        self.inner.set_client_id(client_id);
        self
    }

    /// Set port number
    pub fn port(mut self, port: u16) -> Self {
        self.inner.set_port(port);
        self
    }

    /// Set port number
    pub fn host<T>(mut self, host: T) -> Self
    where
        T: Into<String>,
    {
        self.inner.set_host(host);
        self
    }

    /// Set number of seconds after which client should ping the broker
    /// if there is no other data exchange
    pub fn keep_alive(mut self, secs: u64) -> Self {
        self.inner.set_keep_alive(secs);
        self
    }

    /// Set packet size limit (in Kilo Bytes)
    pub fn max_packet_size(mut self, sz: usize) -> Self {
        self.inner.set_max_packet_size(sz);
        self
    }

    /// `clean_session = true` removes all the state from queues & instructs the broker
    /// to clean all the client state when client disconnects.
    ///
    /// When set `false`, broker will hold the client state and performs pending
    /// operations on the client when reconnection with same `client_id`
    /// happens. Local queue state is also held to retransmit packets after reconnection.
    pub fn clean_session(mut self, clean_session: bool) -> Self {
        self.inner.set_clean_session(clean_session);
        self
    }

    /// Set how to connect to a MQTT Broker (either TCP or Tls)
    pub fn connection_method(mut self, opts: ConnectionMethod) -> Self {
        self.inner.set_connection_method(opts);
        self
    }

    pub fn proxy(mut self, proxy: Proxy) -> Self {
        self.inner.set_proxy(proxy);
        self
    }

    /// Time interval after which client should retry for new
    /// connection if there are any disconnections. By default, no retry will happen
    pub fn reconnect_opts(mut self, opts: ReconnectOptions) -> Self {
        self.inner.set_reconnect_opts(opts);
        self
    }

    /// Set security option
    /// Supports username-password auth, tls client cert auth, gcloud iotcore jwt auth
    pub fn security_opts(mut self, opts: SecurityOptions) -> Self {
        self.inner.set_security_opts(opts);
        self
    }

    /// Set last will and testament
    pub fn last_will(mut self, last_will: LastWill) -> Self {
        self.inner.set_last_will(last_will);
        self
    }

    /// Set notification channel capacity
    pub fn notification_channel_capacity(mut self, capacity: usize) -> Self {
        self.inner.set_notification_channel_capacity(capacity);
        self
    }

    /// Set request channel capacity
    pub fn request_channel_capacity(mut self, capacity: usize) -> Self {
        self.inner.set_request_channel_capacity(capacity);
        self
    }

    /// Enables throttling and sets outoing message rate to the specified 'rate'
    pub fn outgoing_ratelimit(mut self, rate: u64) -> Self {
        self.inner.set_outgoing_ratelimit(rate);
        self
    }

    /// Sleeps for the 'delay' about of time before sending the next message if the
    /// specified 'queue_size's are hit
    pub fn outgoing_queuelimit(mut self, queue_size: usize, delay: Duration) -> Self {
        self.inner.set_outgoing_queuelimit(queue_size, delay);
        self
    }

    /// Build the MqttOptions
    pub fn build(self) -> MqttOptions {
        self.inner
    }

}

#[cfg(test)]
mod test {
    use crate::mqttoptions::{MqttOptions, ReconnectOptions};

    #[test]
    #[should_panic]
    fn client_id_startswith_space() {
        let _mqtt_opts = MqttOptions::builder()
            .client_id(" client_a")
            .host("127.0.0.1")
            .port(1883)
            .reconnect_opts(ReconnectOptions::Always(10))
            .clean_session(true)
            .build();
    }

    #[test]
    #[should_panic]
    fn no_client_id() {
        let _mqtt_opts = MqttOptions::builder()
            .client_id("")
            .host("127.0.0.1")
            .port(1883)
            .reconnect_opts(ReconnectOptions::Always(10))
            .clean_session(true)
            .build();
    }
}
