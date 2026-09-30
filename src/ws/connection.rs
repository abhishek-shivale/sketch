//! Lifecycle of one WebSocket connection: a reader loop (this task), a writer
//! task that owns the socket sink, and a heartbeat.

use std::{sync::Arc, time::Duration};

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    sync::mpsc,
    time::{self, Instant, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, info, info_span};
use uuid::Uuid;

use super::session::{Flow, Session};
use crate::{
    error::ProtocolError,
    hub::{Hub, Peer},
};

/// How long the writer gets to flush and send a close frame on exit.
const WRITER_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn run(socket: WebSocket, hub: Arc<Hub>, shutdown: CancellationToken) {
    let id = Uuid::new_v4();
    async move {
        let _guard = hub.track_connection();
        let config = hub.config().clone();
        let (sink, stream) = socket.split();
        let (tx, rx) = mpsc::channel(config.send_queue);
        // Child of the server-wide token: fires on shutdown, slow consumer,
        // or writer failure.
        let kick = shutdown.child_token();
        let writer = tokio::spawn(write_loop(sink, rx, kick.clone()).in_current_span());

        let mut session = Session::new(
            Arc::clone(&hub),
            Peer {
                id,
                tx,
                kick: kick.clone(),
            },
        );
        info!(connections = hub.connection_count(), "connected");

        let reason = match session.greet() {
            Ok(()) => {
                read_loop(
                    &mut session,
                    stream,
                    &kick,
                    config.heartbeat_interval,
                    config.heartbeat_timeout,
                )
                .await
            }
            Err(e) => {
                debug!("failed to greet: {e}");
                "greeting failed"
            }
        };

        session.leave();
        // Dropping the session drops the last sender, so the writer drains
        // what is queued and then closes the socket.
        drop(session);
        let mut writer = writer;
        if time::timeout(WRITER_DRAIN_TIMEOUT, &mut writer)
            .await
            .is_err()
        {
            kick.cancel();
            let _ = writer.await;
        }
        info!(
            reason,
            connections = hub.connection_count() - 1,
            "disconnected"
        );
    }
    .instrument(info_span!("conn", %id))
    .await;
}

async fn read_loop(
    session: &mut Session,
    mut stream: SplitStream<WebSocket>,
    kick: &CancellationToken,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
) -> &'static str {
    let mut heartbeat = time::interval_at(Instant::now() + heartbeat_interval, heartbeat_interval);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            () = kick.cancelled() => return "cancelled",
            _ = heartbeat.tick() => {
                if last_seen.elapsed() > heartbeat_timeout {
                    return "heartbeat timeout";
                }
                session.ping();
            }
            frame = stream.next() => {
                let message = match frame {
                    None => return "stream ended",
                    Some(Err(e)) => {
                        debug!("transport error: {e}");
                        return "transport error";
                    }
                    Some(Ok(message)) => message,
                };
                last_seen = Instant::now();
                match message {
                    Message::Text(text) => match session.handle_text(&text) {
                        Ok(Flow::Continue) => {}
                        Ok(Flow::Close) => return "client disconnected",
                        Err(e) => session.report(&e),
                    },
                    Message::Binary(_) => session.report(&ProtocolError::BinaryFrame),
                    Message::Close(_) => return "close frame",
                    // Pings are answered by the WebSocket layer; both only
                    // matter as proof of life.
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }
}

async fn write_loop(
    mut sink: SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<Message>,
    kick: CancellationToken,
) {
    loop {
        let message = tokio::select! {
            biased;
            () = kick.cancelled() => break,
            message = rx.recv() => match message {
                Some(message) => message,
                None => break,
            },
        };
        let sent = tokio::select! {
            result = sink.send(message) => result,
            () = kick.cancelled() => break,
        };
        if let Err(e) = sent {
            debug!("write failed: {e}");
            // Wake the reader so the whole connection shuts down.
            kick.cancel();
            return;
        }
    }

    let close = Message::Close(Some(CloseFrame {
        code: if kick.is_cancelled() {
            close_code::AWAY
        } else {
            close_code::NORMAL
        },
        reason: "".into(),
    }));
    let _ = time::timeout(Duration::from_secs(1), async {
        let _ = sink.send(close).await;
        let _ = sink.close().await;
    })
    .await;
}
