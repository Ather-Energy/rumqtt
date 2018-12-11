extern crate futures;
extern crate pretty_env_logger;
extern crate rumqtt;
extern crate tokio;
#[macro_use]
extern crate serde_derive;


use rumqtt::{MqttClient, MqttOptions, Proxy, ReconnectOptions};

use rumqtt::QoS;
use std::thread;
use std::time::Duration;

#[derive(Deserialize, Debug)]
struct Config {
    proxy_host: String,
    proxy_port: u16,
    main_host: String,
    main_port: u16,
}

fn main() {
    pretty_env_logger::init();
    let config: Config = envy::from_env().unwrap();
    let key = include_bytes!("gcloudfiles/rsa_private.der");

    let reconnect_options = ReconnectOptions::Never;
    let proxy = Proxy::HttpConnect(config.proxy_host, config.proxy_port, key.to_vec(), 40);

    let id = "RAVI-LINUX";
    let host = "prod-mqtt-broker.atherengineering.in";
    let port = 1883;

    let mqtt_options = MqttOptions::new(id, host, port);

    let mqtt_options = mqtt_options.set_keep_alive(10)
                                   .set_reconnect_opts(reconnect_options)
                                   .set_proxy(proxy);

    let (mut mqtt_client, notifications) = MqttClient::start(mqtt_options).unwrap();

    mqtt_client.subscribe("hello/world", QoS::AtLeastOnce).unwrap();

    thread::spawn(move || for i in 0..100 {
        let payload = format!("publish {}", i);
        thread::sleep(Duration::from_millis(100));
        mqtt_client.publish("hello/world", QoS::AtLeastOnce, payload).unwrap();
    });

    for notification in notifications {
        println!("{:?}", notification)
    }
}