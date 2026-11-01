//! Publishes `schemas.json` to the MQTT broker, where the decoder picks it up.

use std::time::{Duration, Instant};

use rumqttc::{Client, Event, MqttOptions, Packet, QoS};

/// The topic the decoder reads `schemas.json` from.
const TOPIC: &str = "aio/schemas";

/// How long the broker gets to connect and take the message.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The broker, from `AIO_MQTT_URL`.
#[derive(Debug, PartialEq, Eq)]
struct Broker {
    host: String,
    port: u16,
    /// User and password, if the URL has them.
    credentials: Option<(String, String)>,
}

/// Publishes `text` retained with QoS 1 to `aio/schemas`, and waits until the
/// broker has acked it. The broker comes from `AIO_MQTT_URL`.
pub fn publish_schemas(text: &str) -> Result<(), String> {
    let url = std::env::var("AIO_MQTT_URL").map_err(|_| {
        "AIO_MQTT_URL isn't set; set it to the broker, \
         for example `mqtt://user:pass@192.168.1.10:1883`"
            .to_string()
    })?;
    let broker = parse_url(&url)?;
    let address = format!("{}:{}", broker.host, broker.port);

    let mut options = MqttOptions::new("aio-xtask", &broker.host, broker.port);
    if let Some((user, password)) = &broker.credentials {
        options.set_credentials(user, password);
    }
    // The default limit is 10 KB; schemas.json grows with every source.
    options.set_max_packet_size(1024 * 1024, 1024 * 1024);
    let (client, mut connection) = Client::new(options, 10);
    client
        .publish(TOPIC, QoS::AtLeastOnce, true, text.as_bytes())
        .map_err(|e| format!("can't publish to {address}: {e}"))?;

    // Polling the connection is what connects and sends. It's done once the
    // broker acks the message.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match connection.recv_timeout(left) {
            Ok(Ok(Event::Incoming(Packet::PubAck(_)))) => break,
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(format!("MQTT broker {address}: {e}")),
            Err(_) => {
                return Err(format!(
                    "MQTT broker {address} didn't take the message within {} s",
                    TIMEOUT.as_secs()
                ));
            }
        }
    }
    // Say goodbye, so the broker doesn't log a lost connection. The message is
    // already delivered, so errors don't matter here.
    let _ = client.disconnect();
    let _ = connection.recv_timeout(Duration::from_millis(500));

    println!("published schemas.json to {TOPIC} on {address}");
    Ok(())
}

/// Reads `mqtt://[user:password@]host[:port]`; the port defaults to 1883.
/// No `%` escapes. Error messages never repeat the URL, as it may hold a
/// password.
fn parse_url(url: &str) -> Result<Broker, String> {
    let Some(rest) = url.strip_prefix("mqtt://") else {
        return Err("AIO_MQTT_URL must start with `mqtt://`; TLS isn't supported".to_string());
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);

    // The last `@` ends the user info, so a password may contain `@`.
    let (credentials, address) = match rest.rsplit_once('@') {
        Some((user_info, address)) => {
            let (user, password) = user_info.split_once(':').unwrap_or((user_info, ""));
            (Some((user.to_string(), password.to_string())), address)
        }
        None => (None, rest),
    };
    if address.contains('/') {
        return Err(
            "AIO_MQTT_URL has a path; write only `mqtt://[user:password@]host[:port]`".to_string(),
        );
    }

    let (host, port) = match address.split_once(':') {
        Some((host, port)) => {
            let port = port
                .parse()
                .map_err(|_| format!("AIO_MQTT_URL: port `{port}` isn't a number up to 65535"))?;
            (host, port)
        }
        None => (address, 1883),
    };
    if host.is_empty() {
        return Err("AIO_MQTT_URL has no host".to_string());
    }
    Ok(Broker {
        host: host.to_string(),
        port,
        credentials,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broker(host: &str, port: u16, credentials: Option<(&str, &str)>) -> Broker {
        Broker {
            host: host.to_string(),
            port,
            credentials: credentials.map(|(u, p)| (u.to_string(), p.to_string())),
        }
    }

    #[test]
    fn host_only() {
        assert_eq!(
            parse_url("mqtt://192.168.1.10"),
            Ok(broker("192.168.1.10", 1883, None))
        );
        assert_eq!(
            parse_url("mqtt://broker.lan/"),
            Ok(broker("broker.lan", 1883, None))
        );
    }

    #[test]
    fn port() {
        assert_eq!(
            parse_url("mqtt://192.168.1.10:1884"),
            Ok(broker("192.168.1.10", 1884, None))
        );
        assert!(parse_url("mqtt://host:abc").is_err());
        assert!(parse_url("mqtt://host:70000").is_err());
    }

    #[test]
    fn user_and_password() {
        assert_eq!(
            parse_url("mqtt://user:pass@192.168.1.10:1883"),
            Ok(broker("192.168.1.10", 1883, Some(("user", "pass"))))
        );
        // A password may hold `@` and `:`.
        assert_eq!(
            parse_url("mqtt://user:p@s:s@host"),
            Ok(broker("host", 1883, Some(("user", "p@s:s"))))
        );
        // A user without a password.
        assert_eq!(
            parse_url("mqtt://user@host"),
            Ok(broker("host", 1883, Some(("user", ""))))
        );
    }

    #[test]
    fn other_schemes_are_rejected() {
        for url in ["mqtts://host", "ws://host", "tcp://host:1883", "host:1883"] {
            assert!(parse_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn errors_hide_the_password() {
        for url in ["mqtts://user:secret@host", "mqtt://user:secret@host/x"] {
            let error = parse_url(url).unwrap_err();
            assert!(!error.contains("secret"), "{error}");
        }
    }

    #[test]
    fn host_is_required() {
        assert!(parse_url("mqtt://").is_err());
        assert!(parse_url("mqtt://user:pass@:1883").is_err());
    }
}
