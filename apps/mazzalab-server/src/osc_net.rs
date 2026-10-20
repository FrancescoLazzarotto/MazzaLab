//! OSC over UDP and TCP (SLIP framed), QLab-compatible.

use std::collections::HashMap;
use std::net::SocketAddr;

use mazzalab_osc::{Reply, ReplyStatus, slip};
use mazzalab_show::SessionRequest;
use rosc::OscMessage;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::broadcast::error::RecvError;
use tracing::{debug, warn};

use crate::actor::{ShowHandle, Update};

/// Per-client OSC preferences.
#[derive(Debug, Default)]
struct Session {
    updates: bool,
    always_reply: bool,
}

/// Runs one OSC message through the show and builds the reply, if one is due.
async fn process(
    handle: &ShowHandle,
    message: &OscMessage,
    session: &mut Session,
) -> Option<OscMessage> {
    let request = match mazzalab_osc::parse_message(message) {
        Ok(request) => request,
        Err(error) => {
            debug!(address = %message.addr, %error, "rejected OSC message");
            let mut reply = Reply::with_status(message.addr.clone(), ReplyStatus::Error);
            reply.data = Some(serde_json::Value::String(error.to_string()));
            return Some(reply.to_message());
        }
    };
    let outcome = handle.osc(request).await?;
    let mut force = false;
    match &outcome.session {
        Some(SessionRequest::Updates(on)) => session.updates = *on,
        Some(SessionRequest::AlwaysReply(on)) => session.always_reply = *on,
        Some(SessionRequest::Thump) => force = true,
        Some(SessionRequest::Connect { .. } | SessionRequest::Disconnect) | None => {}
    }
    let send =
        force || outcome.is_query || outcome.status != ReplyStatus::Ok || session.always_reply;
    send.then(|| {
        Reply {
            workspace_id: Some(handle.workspace_id().to_owned()),
            address: message.addr.clone(),
            status: outcome.status,
            data: outcome.data,
        }
        .to_message()
    })
}

fn encode(message: &OscMessage) -> Option<Vec<u8>> {
    mazzalab_osc::encode(message)
        .inspect_err(|error| warn!(%error, "could not encode OSC reply"))
        .ok()
}

/// Serves OSC on a UDP socket. Replies go to the sender's address on
/// `reply_port`, or back to the sender's own port when `None`.
pub async fn serve_udp(socket: UdpSocket, handle: ShowHandle, reply_port: Option<u16>) {
    let mut sessions: HashMap<SocketAddr, Session> = HashMap::new();
    let mut updates = handle.subscribe();
    let mut buffer = vec![0_u8; 65_536];
    loop {
        tokio::select! {
            received = socket.recv_from(&mut buffer) => {
                let Ok((length, from)) = received else { continue };
                let reply_to = reply_port.map_or(from, |port| SocketAddr::new(from.ip(), port));
                let messages = match mazzalab_osc::decode(&buffer[..length]) {
                    Ok(messages) => messages,
                    Err(error) => {
                        debug!(%from, ?error, "undecodable OSC datagram");
                        continue;
                    }
                };
                let session = sessions.entry(reply_to).or_default();
                for message in messages {
                    if let Some(bytes) = process(&handle, &message, session)
                        .await
                        .as_ref()
                        .and_then(encode)
                    {
                        let _ = socket.send_to(&bytes, reply_to).await;
                    }
                }
            }
            update = updates.recv() => match update {
                Ok(Update::Osc(messages)) => {
                    let packets: Vec<Vec<u8>> = messages.iter().filter_map(encode).collect();
                    for (address, _) in sessions.iter().filter(|(_, session)| session.updates) {
                        for packet in &packets {
                            let _ = socket.send_to(packet, address).await;
                        }
                    }
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            },
        }
    }
}

pub async fn serve_tcp(listener: TcpListener, handle: ShowHandle) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                debug!(%peer, "OSC TCP client connected");
                tokio::spawn(tcp_connection(stream, handle.clone()));
            }
            Err(error) => warn!(%error, "OSC TCP accept failed"),
        }
    }
}

async fn tcp_connection(stream: TcpStream, handle: ShowHandle) {
    let (mut reader, mut writer) = stream.into_split();
    let mut decoder = slip::Decoder::new();
    let mut session = Session::default();
    let mut updates = handle.subscribe();
    let mut buffer = [0_u8; 8192];
    loop {
        tokio::select! {
            read = reader.read(&mut buffer) => {
                let length = match read {
                    Ok(0) | Err(_) => break,
                    Ok(length) => length,
                };
                for frame in decoder.push(&buffer[..length]) {
                    let Ok(messages) = mazzalab_osc::decode(&frame) else { continue };
                    for message in messages {
                        if let Some(bytes) = process(&handle, &message, &mut session)
                            .await
                            .as_ref()
                            .and_then(encode)
                            && writer.write_all(&slip::encode(&bytes)).await.is_err()
                        {
                            return;
                        }
                    }
                }
            }
            update = updates.recv() => match update {
                Ok(Update::Osc(messages)) if session.updates => {
                    for bytes in messages.iter().filter_map(encode) {
                        if writer.write_all(&slip::encode(&bytes)).await.is_err() {
                            return;
                        }
                    }
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            },
        }
    }
}
