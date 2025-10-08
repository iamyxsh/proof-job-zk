use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use proof_core::ids::JobId;
use proof_core::job::Job;
use tokio::time::interval;

const DEFAULT_SNAPSHOT_FILE: &str = "coordinator_jobs.json";

pub fn snapshot_path() -> PathBuf {
    let dir = std::env::var("DATA_DIR").unwrap_or_else(|_| ".".to_string());
    Path::new(&dir).join(DEFAULT_SNAPSHOT_FILE)
}

pub fn load_jobs(path: &Path) -> Arc<DashMap<JobId, Job>> {
    let map = Arc::new(DashMap::new());

    let data = match std::fs::read_to_string(path) {
        Ok(d) if !d.is_empty() => d,
        _ => {
            tracing::info!(?path, "no snapshot found, starting with empty job set");
            return map;
        }
    };

    let entries: Vec<Job> = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(?path, error = %e, "failed to parse snapshot, starting fresh");
            return map;
        }
    };

    let count = entries.len();
    for job in entries {
        map.insert(job.id, job);
    }
    tracing::info!(?path, count, "restored jobs from snapshot");
    map
}

pub fn save_jobs(path: &Path, jobs: &DashMap<JobId, Job>) -> std::io::Result<()> {
    let entries: Vec<Job> = jobs.iter().map(|r| r.value().clone()).collect();
    let data = serde_json::to_string_pretty(&entries)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn spawn_snapshot_task(
    jobs: Arc<DashMap<JobId, Job>>,
    path: PathBuf,
    period: Duration,
) {
    tokio::spawn(async move {
        let mut tick = interval(period);
        loop {
            tick.tick().await;
            if let Err(e) = save_jobs(&path, &jobs) {
                tracing::warn!(error = %e, "failed to persist job snapshot");
            } else {
                tracing::debug!("job snapshot written");
            }
        }
    });
}
