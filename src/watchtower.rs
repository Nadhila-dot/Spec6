//! Autonomous monitoring — the "Watchtower".
//!
//! Sentinel is not purely reactive. A background patrol periodically re-scans
//! every tracked company so its threat dossier stays fresh without a human
//! asking. It reuses the exact same BrightData overview pipeline the chat agent
//! drives, so an autonomous re-scan is indistinguishable from a manual one — it
//! just happens on a clock.
//!
//! Governance: patrols are gated on staleness (we never re-run a company whose
//! overview is still in progress or was refreshed within the interval), so the
//! loop self-limits its paid web-data spend.

use crate::{overview, render::AppState};
use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use futures::stream::TryStreamExt;
use mongodb::bson::doc;
use serde::Serialize;
use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Live, lock-free status surfaced at `/api/watchtower/status`.
#[derive(Debug, Default)]
pub struct WatchtowerStatus {
    pub enabled: AtomicBool,
    pub interval_secs: AtomicU64,
    pub last_patrol_unix_ms: AtomicU64,
    pub total_patrols: AtomicU64,
    pub total_scans_triggered: AtomicU64,
}

#[derive(Debug, Serialize)]
pub struct WatchtowerSnapshot {
    pub enabled: bool,
    pub interval_secs: u64,
    pub last_patrol_unix_ms: u64,
    pub total_patrols: u64,
    pub total_scans_triggered: u64,
}

impl WatchtowerStatus {
    pub fn snapshot(&self) -> WatchtowerSnapshot {
        WatchtowerSnapshot {
            enabled: self.enabled.load(Ordering::Relaxed),
            interval_secs: self.interval_secs.load(Ordering::Relaxed),
            last_patrol_unix_ms: self.last_patrol_unix_ms.load(Ordering::Relaxed),
            total_patrols: self.total_patrols.load(Ordering::Relaxed),
            total_scans_triggered: self.total_scans_triggered.load(Ordering::Relaxed),
        }
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Arm the autonomous patrol loop. No-op (beyond recording status) when disabled.
pub fn spawn(state: AppState) {
    let interval_secs = state.config.watchtower_interval_secs.max(60);
    state
        .watchtower
        .interval_secs
        .store(interval_secs, Ordering::Relaxed);
    state
        .watchtower
        .enabled
        .store(state.config.watchtower_enabled, Ordering::Relaxed);

    if !state.config.watchtower_enabled {
        tracing::info!("watchtower disabled (set WATCHTOWER_ENABLED=true to arm)");
        return;
    }

    let first_delay = Duration::from_secs(state.config.watchtower_first_delay_secs);
    let interval = Duration::from_secs(interval_secs);
    let stale = ChronoDuration::seconds(interval_secs as i64);

    tokio::spawn(async move {
        tokio::time::sleep(first_delay).await;
        loop {
            match patrol(&state, stale, None).await {
                Ok(triggered) => {
                    tracing::info!(triggered, "watchtower patrol complete");
                }
                Err(err) => tracing::warn!(error = ?err, "watchtower patrol failed"),
            }
            tokio::time::sleep(interval).await;
        }
    });
    tracing::info!(interval_secs, "watchtower armed");
}

/// Run one patrol. When `only_user` is `Some`, restrict to that user's
/// companies and ignore staleness (used by the manual "Run patrol now" button).
/// Returns the number of autonomous scans dispatched.
pub async fn patrol(
    state: &AppState,
    stale: ChronoDuration,
    only_user: Option<mongodb::bson::oid::ObjectId>,
) -> Result<usize> {
    let cutoff = Utc::now() - stale;
    let force = only_user.is_some();
    let filter = match only_user {
        Some(user_id) => doc! { "user_id": user_id },
        None => doc! {},
    };

    let mut cursor = state.db.chat_groups().find(filter, None).await?;
    let mut triggered = 0usize;

    while let Some(group) = cursor.try_next().await? {
        let Some(group_id) = group.id else { continue };
        if !overview::should_queue_company_overview(&group.name, &group.data_text) {
            continue;
        }

        let existing = state
            .db
            .company_overviews()
            .find_one(
                doc! { "user_id": group.user_id, "company_id": group_id },
                None,
            )
            .await?;

        if let Some(ov) = &existing {
            let in_progress = ov.status == "queued" || ov.status == "running";
            if in_progress {
                continue;
            }
            if !force && ov.updated_at > cutoff {
                continue;
            }
        }

        overview::queue_company_overview(state.clone(), group.user_id, group_id).await;
        triggered += 1;

        state.overview_events.emit(
            &group_id.to_hex(),
            "autonomous_patrol",
            &serde_json::json!({
                "company_id": group_id.to_hex(),
                "company_name": group.name,
                "at": Utc::now(),
                "manual": force,
            }),
        );
    }

    state
        .watchtower
        .total_patrols
        .fetch_add(1, Ordering::Relaxed);
    state
        .watchtower
        .total_scans_triggered
        .fetch_add(triggered as u64, Ordering::Relaxed);
    state
        .watchtower
        .last_patrol_unix_ms
        .store(now_unix_ms(), Ordering::Relaxed);

    Ok(triggered)
}
