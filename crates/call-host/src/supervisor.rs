//! The owner task holds the child; cancellation kills and reaps it.
use crate::{Frame, QUEUE_CAPACITY, read_async, write_async};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{process::Command, sync::mpsc};
struct ReaderTask(tokio::task::JoinHandle<()>);
impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Closed,
    Crashed(Option<i32>),
    Hung,
    Protocol,
}
pub fn classify_exit(status: std::process::ExitStatus) -> Exit {
    if status.success() {
        Exit::Closed
    } else {
        Exit::Crashed(status.code())
    }
}
pub struct Supervisor {
    pub process_id: u32,
    pub send: mpsc::Sender<Frame>,
    pub receive: mpsc::Receiver<Frame>,
    pub exited: tokio::sync::oneshot::Receiver<Exit>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Supervisor {
    pub fn spawn(executable: &Path) -> std::io::Result<Self> {
        let mut child = Command::new(executable)
            .env_clear()
            // No inherited credentials, proxy variables, stores, stdout or stderr.
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut input = child.stdin.take().expect("piped stdin");
        let process_id = child.id().expect("spawned child");
        let mut output = child.stdout.take().expect("piped stdout");
        let (send, mut incoming) = mpsc::channel(QUEUE_CAPACITY);
        let (outgoing, receive) = mpsc::channel(QUEUE_CAPACITY);
        let (done, exited) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            // read_exact is not cancellation-safe inside select. A dedicated
            // reader retains partial headers/bodies across health-check ticks.
            let (frames, mut reads) = mpsc::channel(QUEUE_CAPACITY);
            let _reader = ReaderTask(tokio::spawn(async move {
                loop {
                    let frame = read_async(&mut output).await;
                    let failed = frame.is_err();
                    if frames.send(frame).await.is_err() || failed {
                        break;
                    }
                }
            }));
            let mut tick = tokio::time::interval_at(
                tokio::time::Instant::now() + Duration::from_secs(2),
                Duration::from_secs(2),
            );
            let mut nonce = 0;
            let mut pending = None;
            let mut healthy = tokio::time::Instant::now();
            let exit = loop {
                tokio::select! {
                    status = child.wait() => break status.map(classify_exit).unwrap_or(Exit::Protocol),
                    frame = incoming.recv() => {
                        let Some(frame) = frame else { break Exit::Closed };
                        if matches!(frame, Frame::Shutdown) { break Exit::Closed; }
                        if !matches!(tokio::time::timeout(Duration::from_secs(2), write_async(&mut input, &frame)).await, Ok(Ok(()))) { break Exit::Hung; }
                    }
                    _ = tick.tick() => {
                        if healthy.elapsed() > Duration::from_secs(8) { break Exit::Hung; }
                        if pending.is_none() {
                            nonce += 1;
                            pending = Some(nonce);
                            if !matches!(tokio::time::timeout(Duration::from_secs(2), write_async(&mut input, &Frame::Ping(nonce))).await, Ok(Ok(()))) { break Exit::Hung; }
                        }
                    }
                    frame = reads.recv() => {
                        match frame {
                            Some(Ok(Frame::Pong(n))) if pending == Some(n) => { pending = None; healthy = tokio::time::Instant::now(); }
                            Some(Ok(Frame::Pong(_))) => break Exit::Protocol,
                            Some(Ok(frame)) => if outgoing.try_send(frame).is_err() { break Exit::Protocol; },
                            _ => {
                                // EOF often races wait(); retain the actual crash status.
                                break match tokio::time::timeout(Duration::from_millis(250), child.wait()).await {
                                    Ok(Ok(status)) => classify_exit(status),
                                    _ => Exit::Protocol,
                                };
                            }
                        }
                    }
                }
            };
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = done.send(exit);
        });
        Ok(Self {
            process_id,
            send,
            receive,
            exited,
            task,
        })
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn supervisor_reports_real_clean_and_crash_exits() {
        for (command, expected) in [
            ("/usr/bin/true", Exit::Closed),
            ("/usr/bin/false", Exit::Crashed(Some(1))),
        ] {
            let mut process = Supervisor::spawn(Path::new(command)).unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(3), &mut process.exited)
                    .await
                    .unwrap()
                    .unwrap(),
                expected
            );
        }
    }
    fn sleeping_runtime() -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "twayblade-call-supervisor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn alive(pid: u32) -> bool {
        std::process::Command::new("/bin/kill")
            .arg("-0")
            .arg(pid.to_string())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }
    #[tokio::test]
    async fn hung_runtime_is_killed_and_reaped() {
        let path = sleeping_runtime();
        let mut process = Supervisor::spawn(&path).unwrap();
        let exit = tokio::time::timeout(Duration::from_secs(12), &mut process.exited)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(exit, Exit::Hung);
        assert!(!alive(process.process_id));
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn cancellation_kills_the_child_even_without_a_shutdown_ack() {
        let path = sleeping_runtime();
        let process = Supervisor::spawn(&path).unwrap();
        let pid = process.process_id;
        drop(process);
        tokio::time::timeout(Duration::from_secs(3), async {
            while alive(pid) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
