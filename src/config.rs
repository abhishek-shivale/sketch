use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

use crate::error::ConfigError;

/// Runtime configuration. Every field can be overridden with a `SKETCH_*`
/// environment variable; see [`Config::from_env`].
#[derive(Debug, Clone)]
pub struct Config {
    /// `SKETCH_ADDR` (or legacy `BIND`): socket address to listen on.
    pub addr: SocketAddr,
    /// `SKETCH_PUBLIC_DIR`: directory with the built frontend.
    pub public_dir: PathBuf,
    /// `SKETCH_WORKER_THREADS`: Tokio worker threads (`None` = one per core).
    pub worker_threads: Option<usize>,
    /// `SKETCH_CORS_ORIGINS`: comma separated allow-list (empty = any origin).
    pub cors_origins: Vec<String>,

    /// `SKETCH_HEARTBEAT_INTERVAL_SECS`: how often the server pings clients.
    pub heartbeat_interval: Duration,
    /// `SKETCH_HEARTBEAT_TIMEOUT_SECS`: silence after which a client is dropped.
    pub heartbeat_timeout: Duration,
    /// `SKETCH_SEND_QUEUE`: outbound messages buffered per connection before
    /// the client is considered too slow and disconnected.
    pub send_queue: usize,
    /// `SKETCH_MAX_MESSAGE_BYTES`: largest WebSocket message accepted.
    pub max_message_bytes: usize,
    /// `SKETCH_RATE_PER_SEC` / `SKETCH_RATE_BURST`: per-connection message rate.
    pub rate_per_sec: u32,
    pub rate_burst: u32,

    /// `SKETCH_MAX_ROOMS`: rooms held in memory at once.
    pub max_rooms: usize,
    /// `SKETCH_ROOM_MAX_SHAPES`: shapes retained in one room's history.
    pub room_max_shapes: usize,
    /// `SKETCH_ROOM_MAX_BYTES`: approximate history bytes retained per room.
    pub room_max_bytes: usize,
    /// `SKETCH_ROOM_IDLE_TTL_SECS`: how long an empty room is kept.
    pub room_idle_ttl: Duration,
    /// `SKETCH_JANITOR_INTERVAL_SECS`: how often the cleanup cycle runs.
    pub janitor_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            public_dir: PathBuf::from("public"),
            worker_threads: None,
            cors_origins: Vec::new(),
            heartbeat_interval: Duration::from_secs(30),
            heartbeat_timeout: Duration::from_secs(75),
            send_queue: 512,
            max_message_bytes: 16 * 1024 * 1024,
            rate_per_sec: 300,
            rate_burst: 600,
            max_rooms: 10_000,
            room_max_shapes: 5_000,
            room_max_bytes: 32 * 1024 * 1024,
            room_idle_ttl: Duration::from_secs(300),
            janitor_interval: Duration::from_secs(30),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Builds a config from an arbitrary key lookup (used by tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let d = Self::default();
        let secs =
            |var, default: Duration| parse(&get, var, default.as_secs()).map(Duration::from_secs);

        let config = Self {
            // `BIND` is the older name, still used by the Dockerfile.
            addr: match get("SKETCH_ADDR") {
                Some(v) => parse_value("SKETCH_ADDR", &v)?,
                None => parse(&get, "BIND", d.addr)?,
            },
            public_dir: get("SKETCH_PUBLIC_DIR").map_or(d.public_dir, PathBuf::from),
            worker_threads: match get("SKETCH_WORKER_THREADS") {
                Some(v) => Some(parse_value("SKETCH_WORKER_THREADS", &v)?),
                None => None,
            },
            cors_origins: get("SKETCH_CORS_ORIGINS")
                .map(|v| {
                    v.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
            heartbeat_interval: secs("SKETCH_HEARTBEAT_INTERVAL_SECS", d.heartbeat_interval)?,
            heartbeat_timeout: secs("SKETCH_HEARTBEAT_TIMEOUT_SECS", d.heartbeat_timeout)?,
            send_queue: parse(&get, "SKETCH_SEND_QUEUE", d.send_queue)?,
            max_message_bytes: parse(&get, "SKETCH_MAX_MESSAGE_BYTES", d.max_message_bytes)?,
            rate_per_sec: parse(&get, "SKETCH_RATE_PER_SEC", d.rate_per_sec)?,
            rate_burst: parse(&get, "SKETCH_RATE_BURST", d.rate_burst)?,
            max_rooms: parse(&get, "SKETCH_MAX_ROOMS", d.max_rooms)?,
            room_max_shapes: parse(&get, "SKETCH_ROOM_MAX_SHAPES", d.room_max_shapes)?,
            room_max_bytes: parse(&get, "SKETCH_ROOM_MAX_BYTES", d.room_max_bytes)?,
            room_idle_ttl: secs("SKETCH_ROOM_IDLE_TTL_SECS", d.room_idle_ttl)?,
            janitor_interval: secs("SKETCH_JANITOR_INTERVAL_SECS", d.janitor_interval)?,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let fail = |var, reason: &str| {
            Err(ConfigError {
                var,
                reason: reason.to_string(),
            })
        };
        if self.heartbeat_interval.is_zero() {
            return fail("SKETCH_HEARTBEAT_INTERVAL_SECS", "must be greater than 0");
        }
        if self.heartbeat_timeout <= self.heartbeat_interval {
            return fail(
                "SKETCH_HEARTBEAT_TIMEOUT_SECS",
                "must be greater than the heartbeat interval",
            );
        }
        if self.janitor_interval.is_zero() {
            return fail("SKETCH_JANITOR_INTERVAL_SECS", "must be greater than 0");
        }
        if self.send_queue == 0 {
            return fail("SKETCH_SEND_QUEUE", "must be greater than 0");
        }
        if self.rate_per_sec == 0 || self.rate_burst == 0 {
            return fail(
                "SKETCH_RATE_PER_SEC",
                "rate and burst must be greater than 0",
            );
        }
        if self.worker_threads == Some(0) {
            return fail("SKETCH_WORKER_THREADS", "must be greater than 0");
        }
        if let Some(bad) = self
            .cors_origins
            .iter()
            .find(|o| axum::http::HeaderValue::from_str(o).is_err())
        {
            return fail(
                "SKETCH_CORS_ORIGINS",
                &format!("`{bad}` is not a valid origin"),
            );
        }
        Ok(())
    }
}

fn parse<T>(
    get: impl Fn(&str) -> Option<String>,
    var: &'static str,
    default: T,
) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match get(var) {
        Some(value) => parse_value(var, &value),
        None => Ok(default),
    }
}

fn parse_value<T>(var: &'static str, value: &str) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.trim().parse().map_err(|e: T::Err| ConfigError {
        var,
        reason: format!("`{value}`: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn from(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn defaults_are_valid() {
        let config = from(&[]).unwrap();
        assert_eq!(config.addr.port(), 3000);
        assert!(config.cors_origins.is_empty());
    }

    #[test]
    fn overrides_are_parsed() {
        let config = from(&[
            ("SKETCH_ADDR", "0.0.0.0:8080"),
            ("SKETCH_CORS_ORIGINS", "https://a.dev, https://b.dev,"),
            ("SKETCH_ROOM_IDLE_TTL_SECS", "5"),
            ("SKETCH_WORKER_THREADS", "4"),
        ])
        .unwrap();
        assert_eq!(config.addr.port(), 8080);
        assert_eq!(config.cors_origins, ["https://a.dev", "https://b.dev"]);
        assert_eq!(config.room_idle_ttl, Duration::from_secs(5));
        assert_eq!(config.worker_threads, Some(4));
    }

    #[test]
    fn legacy_bind_is_a_fallback_for_addr() {
        assert_eq!(from(&[("BIND", "0.0.0.0:4000")]).unwrap().addr.port(), 4000);
        let both = from(&[("BIND", "0.0.0.0:4000"), ("SKETCH_ADDR", "0.0.0.0:5000")]).unwrap();
        assert_eq!(both.addr.port(), 5000);
    }

    #[test]
    fn bad_values_name_the_variable() {
        let err = from(&[("SKETCH_SEND_QUEUE", "lots")]).unwrap_err();
        assert_eq!(err.var, "SKETCH_SEND_QUEUE");

        let err = from(&[("SKETCH_HEARTBEAT_TIMEOUT_SECS", "10")]).unwrap_err();
        assert_eq!(err.var, "SKETCH_HEARTBEAT_TIMEOUT_SECS");
    }
}
