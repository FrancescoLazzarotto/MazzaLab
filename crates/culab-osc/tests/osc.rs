use culab_osc::{
    Arg, Command, CueMethod, CueSelector, ParseError, PropertyOp, Reply, ReplyStatus, Request,
    decode, encode, parse, parse_message, slip,
};
use rosc::{OscBundle, OscMessage, OscPacket, OscTime, OscType};

fn command(address: &str, args: &[Arg]) -> Command {
    parse(address, args).unwrap().command
}

fn number(n: &str) -> CueSelector {
    CueSelector::Number(n.to_owned())
}

#[test]
fn transport_commands() {
    assert_eq!(command("/go", &[]), Command::Go { cue: None });
    assert_eq!(
        command("/go/12", &[]),
        Command::Go {
            cue: Some("12".into())
        }
    );
    assert_eq!(command("/stop", &[]), Command::Stop);
    assert_eq!(command("/panic", &[]), Command::Panic);
    assert_eq!(command("/playhead/next", &[]), Command::PlayheadNext);
    assert_eq!(
        command("/playbackPosition/previous", &[]),
        Command::PlayheadPrevious
    );
    assert_eq!(command("/playhead/5", &[]), Command::Playhead(number("5")));
    assert_eq!(
        command("/playheadID/ABC", &[]),
        Command::Playhead(CueSelector::Id("ABC".into()))
    );
}

#[test]
fn workspace_prefix_is_extracted() {
    let request = parse("/workspace/hamlet/go", &[]).unwrap();
    assert_eq!(
        request,
        Request {
            workspace: Some("hamlet".into()),
            command: Command::Go { cue: None },
        }
    );
    assert_eq!(parse("/go", &[]).unwrap().workspace, None);
}

#[test]
fn cue_actions_and_selectors() {
    assert_eq!(
        command("/cue/12/start", &[]),
        Command::Cue {
            cue: number("12"),
            method: CueMethod::Start
        }
    );
    assert_eq!(
        command("/cue/selected/stop", &[]),
        Command::Cue {
            cue: CueSelector::Selected,
            method: CueMethod::Stop
        }
    );
    assert_eq!(
        command("/cue_id/1B11984A/pause", &[]),
        Command::Cue {
            cue: CueSelector::Id("1B11984A".into()),
            method: CueMethod::Pause
        }
    );
}

#[test]
fn property_get_set_and_live() {
    let property = |address: &str, args: &[Arg]| match command(address, args) {
        Command::Cue {
            method: CueMethod::Property { name, op, live },
            ..
        } => (name, op, live),
        other => panic!("not a property: {other:?}"),
    };

    assert_eq!(
        property("/cue/10/preWait", &[]),
        ("preWait".into(), PropertyOp::Get, false)
    );
    assert_eq!(
        property("/cue/10/preWait", &[Arg::Float(5.0)]),
        ("preWait".into(), PropertyOp::Set(Arg::Float(5.0)), false)
    );
    assert_eq!(
        property("/cue/1/rate/live", &[Arg::Int(2)]),
        ("rate".into(), PropertyOp::Set(Arg::Int(2)), true)
    );
    assert_eq!(
        property("/cue/1/level/0/1", &[Arg::Float(-6.0)]),
        ("level/0/1".into(), PropertyOp::Set(Arg::Float(-6.0)), false)
    );
}

#[test]
fn increment_syntax_in_argument_or_address() {
    let op = |address: &str, args: &[Arg]| match command(address, args) {
        Command::Cue {
            method: CueMethod::Property { name, op, live },
            ..
        } => (name, op, live),
        other => panic!("not a property: {other:?}"),
    };

    assert_eq!(
        op("/cue/10/preWait/+", &[Arg::Int(1)]),
        ("preWait".into(), PropertyOp::Add(Arg::Int(1)), false)
    );
    assert_eq!(
        op("/cue/10/preWait/+/1", &[]),
        ("preWait".into(), PropertyOp::Add(Arg::Float(1.0)), false)
    );
    assert_eq!(
        op("/cue/x/opacity/-/live", &[Arg::Float(10.0)]),
        (
            "opacity".into(),
            PropertyOp::Subtract(Arg::Float(10.0)),
            true
        )
    );
    // "+" without a delta is not a valid message.
    assert!(matches!(
        parse("/cue/10/preWait/+", &[]),
        Err(ParseError::Unsupported(_))
    ));
}

#[test]
fn lenient_booleans() {
    for truthy in [
        Arg::Bool(true),
        Arg::Int(1),
        Arg::Float(0.5),
        Arg::String("Yes".into()),
        Arg::String("yippee".into()),
        Arg::String("true".into()),
        Arg::String("7".into()),
    ] {
        assert_eq!(truthy.as_bool(), Some(true), "{truthy:?}");
    }
    for falsy in [
        Arg::Bool(false),
        Arg::Int(0),
        Arg::Float(0.0),
        Arg::String("no".into()),
        Arg::String("False".into()),
        Arg::String("0".into()),
    ] {
        assert_eq!(falsy.as_bool(), Some(false), "{falsy:?}");
    }
    assert_eq!(Arg::String("maybe".into()).as_bool(), None);

    assert_eq!(
        command("/updates", &[Arg::String("yes".into())]),
        Command::Updates(true)
    );
    assert!(matches!(
        parse("/updates", &[]),
        Err(ParseError::MissingArgument { .. })
    ));
}

#[test]
fn bad_addresses_are_rejected() {
    assert!(matches!(parse("go", &[]), Err(ParseError::NotAnAddress(_))));
    assert!(matches!(
        parse("/nonsense/thing", &[]),
        Err(ParseError::Unsupported(_))
    ));
    assert!(matches!(
        parse("/cue/1", &[]),
        Err(ParseError::Unsupported(_))
    ));
}

#[test]
fn wire_round_trip_with_bundles() {
    let message = OscMessage {
        addr: "/cue/3/name".into(),
        args: vec![OscType::String("Thunder".into())],
    };
    let bytes = encode(&message).unwrap();
    assert_eq!(decode(&bytes).unwrap(), vec![message.clone()]);

    let bundle = OscPacket::Bundle(OscBundle {
        timetag: OscTime {
            seconds: 0,
            fractional: 1,
        },
        content: vec![
            OscPacket::Message(OscMessage {
                addr: "/go".into(),
                args: vec![],
            }),
            OscPacket::Message(message.clone()),
        ],
    });
    let bytes = rosc::encoder::encode(&bundle).unwrap();
    let messages = decode(&bytes).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(
        parse_message(&messages[1]).unwrap().command,
        Command::Cue {
            cue: number("3"),
            method: CueMethod::Property {
                name: "name".into(),
                op: PropertyOp::Set(Arg::String("Thunder".into())),
                live: false,
            },
        }
    );
}

#[test]
fn reply_format() {
    let reply =
        Reply::ok("/workspace/ABC/cueLists", Some(serde_json::json!([]))).with_workspace("ABC");
    assert_eq!(reply.reply_address(), "/reply/workspace/ABC/cueLists");
    let json: serde_json::Value = serde_json::from_str(&reply.to_json()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "workspace_id": "ABC",
            "address": "/workspace/ABC/cueLists",
            "status": "ok",
            "data": []
        })
    );

    let denied = Reply::with_status("/go", ReplyStatus::Denied);
    let message = denied.to_message();
    assert_eq!(message.addr, "/reply/go");
    assert!(matches!(&message.args[..], [OscType::String(s)] if s.contains("\"denied\"")));
}

#[test]
fn slip_framing_handles_escapes_and_split_reads() {
    let payload = vec![1, 0xC0, 2, 0xDB, 3];
    let framed = slip::encode(&payload);
    assert_eq!(framed.first(), Some(&0xC0));
    assert_eq!(framed.last(), Some(&0xC0));

    let mut decoder = slip::Decoder::new();
    let (first, second) = framed.split_at(3);
    assert!(decoder.push(first).is_empty());
    assert_eq!(decoder.push(second), vec![payload.clone()]);

    let mut stream = slip::encode(b"one");
    stream.extend(slip::encode(b"two"));
    assert_eq!(
        slip::Decoder::new().push(&stream),
        vec![b"one".to_vec(), b"two".to_vec()]
    );
}
