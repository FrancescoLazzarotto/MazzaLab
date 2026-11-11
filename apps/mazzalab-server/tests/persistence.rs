//! Autosave, backups and recovery, through a running server.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use mazzalab_core::Workspace;
use mazzalab_project::{autosave_path, backup_path};
use mazzalab_server::{RunningServer, ServerConfig};
use mazzalab_show::Command;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn show_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mazzalab-persist-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn server(show: &Path, workspace: Workspace, recovered: bool) -> RunningServer {
    let config = ServerConfig {
        show_path: Some(show.to_path_buf()),
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        autosave: Some(Duration::from_millis(40)),
        backups: 3,
        recovered,
        ..ServerConfig::default()
    };
    mazzalab_server::start(config, workspace, |_| Vec::new())
        .await
        .unwrap()
}

async fn rename(server: &RunningServer, name: &str) {
    server
        .handle
        .execute(Command::RenameWorkspace { name: name.into() })
        .await
        .unwrap();
}

/// Waits until the autosave file holds a show with this name.
async fn autosaved(show: &Path, name: &str) -> bool {
    for _ in 0..100 {
        if mazzalab_project::load(autosave_path(show)).is_ok_and(|workspace| workspace.name == name)
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn unsaved_edits_are_autosaved_and_saving_clears_them() {
    let dir = show_dir("autosave");
    let show = dir.join("Show.mazzalab");
    let server = server(&show, Workspace::new("Show"), false).await;

    // A clean show writes nothing.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!autosave_path(&show).exists());

    rename(&server, "first edit").await;
    assert!(autosaved(&show, "first edit").await, "no autosave");
    assert!(!show.exists(), "autosave must not touch the show file");

    // Later edits replace the autosave.
    rename(&server, "second edit").await;
    assert!(autosaved(&show, "second edit").await);

    server.handle.save().await.unwrap();
    assert!(!autosave_path(&show).exists(), "saving clears the autosave");
    assert_eq!(mazzalab_project::load(&show).unwrap().name, "second edit");
    // Nothing was replaced by the first save, so there is no backup yet.
    assert!(!backup_path(&show, 1).exists());

    rename(&server, "third edit").await;
    server.handle.save().await.unwrap();
    assert_eq!(
        mazzalab_project::load(backup_path(&show, 1)).unwrap().name,
        "second edit"
    );
    server.stop().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_crash_leaves_changes_that_the_next_session_recovers() {
    let dir = show_dir("recover");
    let show = dir.join("Show.mazzalab");
    mazzalab_project::save(&show, &Workspace::new("as saved")).unwrap();

    let first = server(&show, mazzalab_project::load(&show).unwrap(), false).await;
    rename(&first, "lost work").await;
    assert!(autosaved(&show, "lost work").await);
    // The process dies here: nothing is saved.
    first.stop().await;
    assert_eq!(mazzalab_project::load(&show).unwrap().name, "as saved");

    let recovered = mazzalab_server::recoverable(&show).expect("nothing to recover");
    assert_eq!(recovered.name, "lost work");
    let second = server(&show, recovered, true).await;
    let snapshot = second.handle.snapshot().await.unwrap();
    assert!(snapshot.dirty && snapshot.recovered);
    assert_eq!(snapshot.workspace.name, "lost work");
    // Nothing was edited, so undo has nowhere to go.
    assert!(!snapshot.can_undo);

    second.handle.save().await.unwrap();
    let snapshot = second.handle.snapshot().await.unwrap();
    assert!(!snapshot.dirty && !snapshot.recovered);
    assert_eq!(mazzalab_project::load(&show).unwrap().name, "lost work");
    assert!(mazzalab_server::recoverable(&show).is_none());
    // The version the recovery replaced is one save away.
    assert_eq!(
        mazzalab_project::load(backup_path(&show, 1)).unwrap().name,
        "as saved"
    );
    second.stop().await;
    std::fs::remove_dir_all(dir).unwrap();
}
