//! mazzalab-osc
//!
//! OSC remote control. The address space follows the publicly documented
//! QLab 5 OSC dictionary so that existing controllers (Stream Deck /
//! Companion profiles, show control systems, custom scripts) can drive
//! MazzaLab unchanged:
//!
//! * UDP and TCP on port [`DEFAULT_PORT`]; UDP replies go to
//!   [`DEFAULT_REPLY_PORT`]; TCP streams use SLIP framing ([`slip`]).
//! * Messages may be prefixed with `/workspace/{id}` to address one workspace.
//! * Replies are sent to `/reply{address}` with a JSON string argument
//!   ([`Reply`]).
//!
//! This crate only translates between bytes, OSC messages and typed
//! [`Request`]s; executing them against an engine happens in the server.

mod command;
mod reply;
pub mod slip;

pub use command::{Arg, Command, CueMethod, CueSelector, ParseError, PropertyOp, Request, parse};
pub use reply::{Reply, ReplyStatus};

use rosc::{OscMessage, OscPacket};

/// Port on which OSC commands are received (UDP and TCP).
pub const DEFAULT_PORT: u16 = 53000;
/// Port to which replies to UDP commands are sent.
pub const DEFAULT_REPLY_PORT: u16 = 53001;

/// Decodes one UDP datagram (or one SLIP frame) into its messages,
/// flattening bundles in order.
pub fn decode(bytes: &[u8]) -> Result<Vec<OscMessage>, rosc::OscError> {
    let (_, packet) = rosc::decoder::decode_udp(bytes)?;
    let mut messages = Vec::new();
    flatten(packet, &mut messages);
    Ok(messages)
}

fn flatten(packet: OscPacket, out: &mut Vec<OscMessage>) {
    match packet {
        OscPacket::Message(message) => out.push(message),
        OscPacket::Bundle(bundle) => {
            for packet in bundle.content {
                flatten(packet, out);
            }
        }
    }
}

pub fn encode(message: &OscMessage) -> Result<Vec<u8>, rosc::OscError> {
    rosc::encoder::encode(&OscPacket::Message(message.clone()))
}

/// Parses an already decoded OSC message.
pub fn parse_message(message: &OscMessage) -> Result<Request, ParseError> {
    let args: Vec<Arg> = message.args.iter().filter_map(Arg::from_osc).collect();
    parse(&message.addr, &args)
}
