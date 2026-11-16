//! A hot backup follows the main server and takes over when it goes silent.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use mazzalab_core::{Cue, CueKind, CueList, WaitCue, Workspace};
use mazzalab_server::{RunningServer, ServerConfig};
use mazzalab_show::Command;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn config(follow: Option<String>) -> ServerConfig {
    ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        autosave: None,
        announce: false,
        record_inputs: false,
        follow,
        ..ServerConfig::default()
    }
}

fn show() -> Workspace {
    let wait = |number: &str, seconds: u64| {
        Cue::new(CueKind::Wait(WaitCue {
            duration: Duration::from_secs(seconds),
        }))
        .with_number(number)
    };
    let mut workspace = Workspace::new("Main show");
    workspace.cue_lists =
        vec![CueList::new("Main").with_cues(vec![wait("1", 60), wait("2", 1), wait("3", 60)])];
    workspace
}

async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !done().await {
        assert!(tokio::time::Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn running(server: &RunningServer) -> Vec<String> {
    let snapshot = server.handle.snapshot().await.unwrap();
    let mut numbers: Vec<String> = server
        .handle
        .status()
        .await
        .unwrap()
        .running
        .iter()
        .filter_map(|run| snapshot.workspace.find_cue(run.cue)?.number.clone())
        .collect();
    numbers.sort();
    numbers
}

#[tokio::test]
async fn a_backup_follows_input_by_input_and_takes_over() {
    let main = mazzalab_server::start(config(None), show(), |_| Vec::new())
        .await
        .unwrap();
    // Something happened before the backup joined: it catches up silently.
    main.handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();

    let backup = mazzalab_server::start(
        config(Some(format!("http://{}", main.http))),
        Workspace::new("Backup's own"),
        |_| Vec::new(),
    )
    .await
    .unwrap();
    until("the backup never loaded the main show", async || {
        backup.handle.snapshot().await.unwrap().workspace.name == "Main show"
    })
    .await;
    until("the backup did not catch up", async || {
        running(&backup).await == ["1"]
    })
    .await;
    assert!(backup.handle.is_following());
    let remote = backup.handle.access(None, true);
    assert!(remote.view && !remote.control && !remote.edit, "{remote:?}");

    // Live inputs follow: a GO, and an edit.
    main.handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    main.handle
        .execute(Command::RenameWorkspace {
            name: "Main show, act 2".into(),
        })
        .await
        .unwrap();
    until("the backup missed the live GO", async || {
        running(&backup).await == ["1", "2"]
    })
    .await;
    until("the backup missed the edit", async || {
        backup.handle.snapshot().await.unwrap().workspace.name == "Main show, act 2"
    })
    .await;
    // Cue 2 ends by itself after a second, on both machines.
    until("cue 2 did not end on the backup", async || {
        running(&backup).await == ["1"]
    })
    .await;
    assert_eq!(running(&main).await, ["1"]);

    // The main server goes away: the backup takes over and runs the show.
    main.stop().await;
    until("the backup never took over", async || {
        !backup.handle.is_following()
    })
    .await;
    assert_eq!(running(&backup).await, ["1"]);
    let local = backup.handle.access(None, true);
    assert!(local.control && local.edit);
    backup
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    assert_eq!(running(&backup).await, ["1", "3"]);
    backup.shutdown();
}
