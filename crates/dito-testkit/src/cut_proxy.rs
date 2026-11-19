use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// What happens to a request once the server has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// The connection closes before a byte of the answer comes back.
    BeforeAnswer,
    /// The connection closes after the head of the answer, without its body.
    InAnswer,
    /// The answer never comes and the connection stays open.
    NoAnswer,
}

/// A TCP hop in front of a server that fails chosen requests the way a dead pooled connection does. Every request
/// still reaches the server, so the server's own count is the number of attempts.
pub struct CutProxy {
    pub url: String,
    plan: Arc<Mutex<VecDeque<Cut>>>,
    task: JoinHandle<()>,
}

impl Drop for CutProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl CutProxy {
    /// `server_url` is `http://host:port`, as [`MockServer::url`](crate::MockServer) is.
    pub async fn start(server_url: &str) -> anyhow::Result<Self> {
        let server = server_url.trim_start_matches("http://").to_owned();
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let plan = Arc::new(Mutex::new(VecDeque::new()));
        let planned = plan.clone();
        let task = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                tokio::spawn(relay(client, server.clone(), planned.clone()));
            }
        });
        Ok(Self { url, plan, task })
    }

    /// The next requests are cut in this order, one each; the ones after them pass.
    pub fn cut(&self, cuts: impl IntoIterator<Item = Cut>) {
        self.plan.lock().expect("plan mutex poisoned").extend(cuts);
    }
}

async fn relay(mut client: TcpStream, server: String, plan: Arc<Mutex<VecDeque<Cut>>>) {
    let Ok(mut server) = TcpStream::connect(server).await else { return };
    let (mut sent, mut answered) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    let mut cut = None;
    let mut between_requests = true;
    loop {
        tokio::select! {
            read = client.read(&mut sent) => {
                let Ok(n @ 1..) = read else { return };
                if std::mem::take(&mut between_requests) {
                    cut = plan.lock().expect("plan mutex poisoned").pop_front();
                }
                if server.write_all(&sent[..n]).await.is_err() {
                    return;
                }
            }
            read = server.read(&mut answered) => {
                let Ok(n @ 1..) = read else { return };
                let answer = &answered[..n];
                match cut {
                    Some(Cut::BeforeAnswer) => return,
                    Some(Cut::InAnswer) => {
                        let head = answer.windows(4).position(|w| w == b"\r\n\r\n").map_or(n, |at| at + 4);
                        let _ = client.write_all(&answer[..head]).await;
                        return;
                    }
                    Some(Cut::NoAnswer) => continue,
                    None => {}
                }
                between_requests = true;
                if client.write_all(answer).await.is_err() {
                    return;
                }
            }
        }
    }
}
