use crate::statement::*;
use async_trait::async_trait;
use tokio::sync::mpsc::{self, Sender};
use tokio::task;

const MAX_RETRIES: usize = 3;
const BACKOFF: std::time::Duration = std::time::Duration::from_millis(1000);

#[async_trait]
pub trait Solution {
    async fn solve(repositories: Vec<ServerName>) -> Option<Binary>;
}

pub struct Solution0;

#[async_trait]
impl Solution for Solution0 {
    async fn solve(repositories: Vec<ServerName>) -> Option<Binary> {
        let (tx, mut rx) = mpsc::channel::<Binary>(repositories.len());
        let handles = repositories
            .into_iter()
            .map(|repo| {
                let retried_download = RetriedDownload::new(repo, tx.clone(), MAX_RETRIES, BACKOFF);
                task::spawn(retried_download.run())
            })
            .collect::<Vec<_>>();

        // Close the sender when all tasks are done, so it is known that no more messages will be sent and the download resulted in error.
        drop(tx);

        rx.recv().await.map(|bin| {
            for handle in handles.iter() {
                handle.abort();
            }
            bin
        })
    }
}

/// A struct to handle the retry logic for downloading binaries.
/// It encapsulates the logic of retrying a download.
/// It uses a channel to send the downloaded binary back to the main task.
struct RetriedDownload {
    repo: ServerName,
    tx: Sender<Binary>,
    retries: usize,
    backoff: std::time::Duration,
}

impl RetriedDownload {
    fn new(
        repo: ServerName,
        tx: mpsc::Sender<Binary>,
        retries: usize,
        backoff: std::time::Duration,
    ) -> Self {
        Self {
            repo,
            tx,
            retries,
            backoff,
        }
    }

    pub async fn run(self) {
        for i in 0..=self.retries {
            match download(self.repo.clone()).await {
                Ok(bin) => {
                    let _ = self.tx.send(bin).await;
                    return;
                }
                Err(err) => {
                    eprintln!(
                        "Error downloading from {}: {:?} (retry {}/{} in {:?})",
                        self.repo.0,
                        err,
                        i + 1,
                        self.retries,
                        self.backoff
                    );
                    tokio::time::sleep(self.backoff).await;
                }
            }
        }
    }
}
