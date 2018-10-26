use client::mqttstate::MqttState;
use client::network::stream::NetworkStream;
use client::Notification;
use client::Request;
use codec::MqttCodec;
use crossbeam_channel;
use error::{ConnectError, NetworkError};
use futures::stream::SplitStream;
use futures::sync::mpsc;
use futures::{future, stream};
use futures::{Future, Sink, Stream};
use mqtt3::Packet;
use mqttoptions::{ConnectionMethod, MqttOptions, ReconnectOptions};
use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use std::time::Duration;
use tokio::runtime::current_thread;
use tokio::timer::{Interval, Timeout};
use tokio_codec::Framed;

//  NOTES: Don't use `wait` in eventloop thread even if you
//         are ok with blocking code. It might cause deadlocks
//  https://github.com/tokio-rs/tokio-core/issues/182

pub struct Connection {
    mqtt_state: Rc<RefCell<MqttState>>,
    userrequest_rx: mpsc::Receiver<Request>,
    notification_tx: crossbeam_channel::Sender<Notification>,
    mqttoptions: MqttOptions,
}

impl Connection {
    /// Takes mqtt options and tries to create initial connection on current thread and handles
    /// connection events in a new thread if the initial connection is successful
    pub fn run(mqttoptions: MqttOptions)
               -> (mpsc::Sender<Request>, crossbeam_channel::Receiver<Notification>) {
        let (notification_tx, notificaiton_rx) = crossbeam_channel::bounded(10);
        let (userrequest_tx, userrequest_rx) = mpsc::channel::<Request>(10);

        thread::spawn(move || {
                          let mqtt_state =
                              Rc::new(RefCell::new(MqttState::new(mqttoptions.clone())));
                          let mut connection = Connection { mqtt_state,
                                                            userrequest_rx,
                                                            notification_tx,
                                                            mqttoptions, };

                          connection.mqtt_eventloop()
                      });

        (userrequest_tx, notificaiton_rx)
    }

    fn mqtt_eventloop(&mut self) {
        let mut connection_count = 1;
        let reconnect_option = self.mqttoptions.reconnect;

        'reconnection: loop {
            let mqtt_connect_future = self.mqtt_connect();
            let mqtt_connect_deadline = Timeout::new(mqtt_connect_future, Duration::from_secs(30));

            // NOTE: We need to use same reactor across threads because io resources (framed) will
            //       bind to reactor lazily.
            //       You'll face `reactor gone` error if `framed` is used again with a new recator
            let mut rt = current_thread::Runtime::new().unwrap();
            let framed = match rt.block_on(mqtt_connect_deadline) {
                Ok(framed) => {
                    connection_count += 1;
                    framed
                }
                Err(e) => {
                    error!("Connection error = {:?}", e);
                    match reconnect_option {
                        ReconnectOptions::AfterFirstSuccess(_) if connection_count == 1 => break,
                        ReconnectOptions::AfterFirstSuccess(time) => {
                            thread::sleep(Duration::from_secs(time))
                        }
                        ReconnectOptions::Always(time) => thread::sleep(Duration::from_secs(time)),
                        ReconnectOptions::Never => break,
                    }
                    continue 'reconnection;
                }
            };

            debug!("Mqtt connection successful!!");

            let mqtt_future = self.mqtt_future(framed);

            if let Err(e) = rt.block_on(mqtt_future) {
                error!("Mqtt eventloop error = {:?}", e);
                match reconnect_option {
                    ReconnectOptions::AfterFirstSuccess(_) if connection_count == 1 => break,
                    ReconnectOptions::AfterFirstSuccess(time) => {
                        thread::sleep(Duration::from_secs(time))
                    }
                    ReconnectOptions::Always(time) => thread::sleep(Duration::from_secs(time)),
                    ReconnectOptions::Never => break,
                }
                continue 'reconnection;
            }

            error!("Reactor Exited !!!!!!!!!!!");
        }
    }

    /// Resolves dns with blocking API and composes a future
    /// which makes a new tcp or tls connection to the broker.
    /// Note that this doesn't actual connect to the broker
    fn tcp_connect_future(
        &self)
        -> impl Future<Item = Framed<NetworkStream, MqttCodec>, Error = ConnectError> {
        // TODO: Remove unwraps
        // TODO: This call will also do dns resolution. Find a way to add timeout to this call
        //        let address = self.mqttoptions.broker_addr.to_socket_addrs().unwrap().next().unwrap();
        //
        //        NetworkStream::connect(address)
        let host = &self.mqttoptions.broker_addr;
        let port = self.mqttoptions.port;
        let connection_method = self.mqttoptions.connection_method.clone();
        let builder = NetworkStream::builder();

        let builder = match connection_method {
            ConnectionMethod::Tls(ca, Some((cert, key))) => builder.add_certificate_authority(&ca)
                                                                   .add_client_auth(&cert, &key),
            ConnectionMethod::Tls(ca, None) => builder.add_certificate_authority(&ca),
            ConnectionMethod::Tcp => builder,
        };

        builder.connect(host, port)
    }

    /// Composes a new future which is a combination of tcp connect + mqtt handshake
    fn mqtt_connect(
        &self)
        -> impl Future<Item = Framed<NetworkStream, MqttCodec>, Error = ConnectError> {
        let mqtt_state = self.mqtt_state.clone();
        let tcp_connect_future = self.tcp_connect_future();
        let connect_packet = future::result(self.mqtt_state.borrow_mut().handle_outgoing_connect());

        tcp_connect_future.and_then(move |framed| {
                              connect_packet.and_then(move |packet| {
                                                          // send mqtt connect packet
                                                          framed.send(Packet::Connect(packet))
                                                                .map_err(|e| ConnectError::from(e))
                                                      })
                                            .and_then(move |framed| {
                                                framed
                        .into_future()
                        .map_err(|(err, _framed)| ConnectError::from(err))
                        .and_then(move |(response, framed)| {
                            // receive mqtt connack packet
                            debug!("Mqtt connect response = {:?}", response);
                            let mut mqtt_state = mqtt_state.borrow_mut();
                            check_and_validate_connack(response, framed, &mut mqtt_state)
                        })
                                            })
                          })
    }

    fn mqtt_future<'a>(&'a mut self,
                       framed: Framed<NetworkStream, MqttCodec>)
                       -> impl Future<Item = (), Error = NetworkError> + 'a {
        let (network_sink, network_stream) = framed.split();

        let keep_alive_stream = self.network_ping_stream();
        let network_reply_stream = self.network_reply_stream(network_stream);
        let network_request_stream = self.network_request_stream();

        network_request_stream.select(network_reply_stream)
                              .select(keep_alive_stream)
                              .forward(network_sink)
                              .map(|(_selct, _splitsink)| ())
    }

    /// Handles all incoming network packets (including sending notifications to user over crossbeam
    /// channel) and creates a stream of packets to send on network
    fn network_reply_stream(&self,
                            network_stream: SplitStream<Framed<NetworkStream, MqttCodec>>)
                            -> impl Stream<Item = Packet, Error = NetworkError> {
        let mqtt_state = self.mqtt_state.clone();

        // TODO: Can we prevent this clone?
        // cloning crossbeam channel sender everytime is a problem accordig to docs
        let notification_tx = self.notification_tx.clone();
        network_stream.map_err(|e| {
                                   error!("Network receiver error = {:?}", e);
                                   NetworkError::from(e)
                               })
                      .and_then(move |packet| {
                          debug!("Incoming packet = {:?}", packet);
                          let network_reply_future =
                              future::result(mqtt_state.borrow_mut()
                                                       .handle_incoming_mqtt_packet(packet));
                          let notification_tx = notification_tx.clone();

                          network_reply_future.and_then(move |(notification, reply)| {
                                                            handle_notification(notification,
                                                                                &notification_tx);
                                                            future::ok(reply)
                                                        })
                                              .or_else(|e| future::err(e))
                      })
                      .filter(|reply| should_forward_packet(reply))
                      .and_then(move |packet| future::ok(packet.into()))
    }

    /// Handles all incoming user and session requests and creates a stream of packets to send
    /// on network
    fn network_request_stream<'a>(&'a mut self)
                                  -> impl Stream<Item = Packet, Error = NetworkError> + 'a {
        let mqtt_state = self.mqtt_state.clone();

        let userrequest_rx = self.userrequest_rx
                                 .by_ref()
                                 .map_err(|e| {
                                              error!("User request error = {:?}", e);
                                              NetworkError::Blah
                                          })
                                 .and_then(move |userrequest| {
                                               let mut mqtt_state = mqtt_state.borrow_mut();
                                               validate_userrequest(userrequest, &mut mqtt_state)
                                           });

        let mqtt_state = self.mqtt_state.clone();

        let last_session_publishes = mqtt_state.borrow_mut().handle_reconnection();
        let last_session_publishes =
            stream::iter_ok::<_, ()>(last_session_publishes).map_err(|e| {
                         error!("Last session publish stream error = {:?}", e);
                         NetworkError::Blah
                     });

        // NOTE: AndThen is a stream and ForEach is a future
        // TODO: Check if 'chain' puts all its elements before userrequests
        userrequest_rx.chain(last_session_publishes)
                      .and_then(move |packet: Packet| {
                                    future::result(mqtt_state.borrow_mut()
                                                             .handle_outgoing_mqtt_packet(packet))
                                })
    }

    fn network_ping_stream(&self) -> impl Stream<Item = Packet, Error = NetworkError> {
        let keep_alive = self.mqttoptions.keep_alive;
        let mqtt_state = self.mqtt_state.clone();
        let ping_interval = Interval::new_interval(keep_alive);

        ping_interval.map_err(|e| e.into())
                     .filter(move |_v| {
                                 let mqtt_state = mqtt_state.borrow();
                                 mqtt_state.is_ping_required()
                             })
                     .and_then(|_v| future::ok(Packet::Pingreq))
    }
}

fn validate_userrequest(userrequest: Request,
                        mqtt_state: &mut MqttState)
                        -> impl Future<Item = Packet, Error = NetworkError> {
    match userrequest {
        Request::Reconnect(mqttoptions) => {
            mqtt_state.opts = mqttoptions;
            future::err(NetworkError::UserReconnect)
        }
        _ => future::ok(userrequest.into()),
    }
}

fn handle_notification(notification: Notification,
                       notification_tx: &crossbeam_channel::Sender<Notification>) {
    match notification {
        Notification::None => (),
        _ if !notification_tx.is_full() => notification_tx.send(notification),
        _ => (),
    }
}

/// Checks if incoming packet is mqtt connack packet. Useful after mqtt
/// connect when we are waiting for connack but not any other packet.
fn check_and_validate_connack(
    packet: Option<Packet>,
    framed: Framed<NetworkStream, MqttCodec>,
    mqtt_state: &mut MqttState)
    -> impl Future<Item = Framed<NetworkStream, MqttCodec>, Error = ConnectError> {
    match packet {
        Some(Packet::Connack(connack)) => {
            if let Err(err) = mqtt_state.handle_incoming_connack(connack) {
                future::err(err)
            } else {
                future::ok(framed)
            }
        }
        Some(packet) => future::err(ConnectError::NotConnackPacket(packet)),
        None => future::err(ConnectError::NoResponse),
    }
}

fn should_forward_packet(reply: &Request) -> bool {
    match reply {
        Request::None => false,
        _ => true,
    }
}

fn packet_info(packet: &Packet) -> String {
    match packet {
        Packet::Publish(p) => format!("topic = {}, \
                                       qos = {:?}, \
                                       pkid = {:?}, \
                                       payload size = {:?} bytes",
                                      p.topic_name,
                                      p.qos,
                                      p.pid,
                                      p.payload.len()),

        _ => format!("{:?}", packet),
    }
}

impl From<Request> for Packet {
    fn from(item: Request) -> Self {
        match item {
            Request::Publish(publish) => Packet::Publish(publish),
            Request::PubAck(pkid) => Packet::Puback(pkid),
            Request::PubRec(pkid) => Packet::Pubrec(pkid),
            Request::PubRel(pkid) => Packet::Pubrel(pkid),
            Request::PubComp(pkid) => Packet::Pubcomp(pkid),
            Request::Ping => Packet::Pingreq,
            Request::Disconnect => Packet::Disconnect,
            Request::Subscribe(subscribe) => Packet::Subscribe(subscribe),
            _ => unimplemented!(),
        }
    }
}