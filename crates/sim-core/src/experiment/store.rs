use super::contract::Run;
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Mutex;

pub struct Store(Mutex<Connection>);
impl Store {
    pub fn open(path: &str) -> Result<Self> {
        if path != ":memory:" {
            if let Some(p) = std::path::Path::new(path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
            {
                std::fs::create_dir_all(p)?;
            }
        }
        let db = Connection::open(path)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS experiment_runs (workspace TEXT NOT NULL, id TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY(workspace,id));")?;
        let store = Self(Mutex::new(db));
        // A process restart never silently replays paid model calls.
        let runs: Vec<String> = store
            .0
            .lock()
            .unwrap()
            .prepare("SELECT body FROM experiment_runs")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for body in runs {
            let mut run: Run = serde_json::from_str(&body)?;
            if [
                "planning",
                "simulating",
                "checking",
                "repairing",
                "verifying",
                "researching",
                "synthesizing",
            ]
            .contains(&run.status.as_str())
            {
                run.status = "interrupted".into();
                run.error=Some("Server restarted. Start a new run; saved evidence is available in this receipt.".into());
                run.event(
                    "interrupted",
                    "Server restarted; no paid calls were replayed",
                );
                store.save(&run)?;
            }
        }
        Ok(store)
    }
    pub fn get(&self, ws: &str, id: &str) -> Result<Option<Run>> {
        let body: Option<String> = self
            .0
            .lock()
            .unwrap()
            .query_row(
                "SELECT body FROM experiment_runs WHERE workspace=? AND id=?",
                params![ws, id],
                |r| r.get(0),
            )
            .optional()?;
        body.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn insert(&self, run: &Run) -> Result<bool> {
        Ok(self.0.lock().unwrap().execute(
            "INSERT OR IGNORE INTO experiment_runs VALUES (?,?,?)",
            params![run.workspace, run.run_id, serde_json::to_string(run)?],
        )? == 1)
    }
    pub fn save(&self, run: &Run) -> Result<()> {
        let db = self.0.lock().unwrap();
        let existing: Option<String> = db
            .query_row(
                "SELECT body FROM experiment_runs WHERE workspace=? AND id=?",
                params![run.workspace, run.run_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(s) = existing {
            let old: Run = serde_json::from_str(&s)?;
            ensure!(old.status != "cancelled", "Run cancelled");
        }
        db.execute(
            "UPDATE experiment_runs SET body=? WHERE workspace=? AND id=?",
            params![serde_json::to_string(run)?, run.workspace, run.run_id],
        )?;
        Ok(())
    }
    /// Claim a verification retry under the same lock as cancellation/worker saves.
    pub fn claim_verification(&self, ws: &str, id: &str) -> Result<Option<Run>> {
        let db = self.0.lock().unwrap();
        let body: Option<String> = db
            .query_row(
                "SELECT body FROM experiment_runs WHERE workspace=? AND id=?",
                params![ws, id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(body) = body else { return Ok(None) };
        let mut run: Run = serde_json::from_str(&body)?;
        ensure!(
            super::p2::can_retry(&run) || super::p2::can_retry_assessment(&run),
            "This run is not eligible for another verification attempt"
        );
        ensure!(
            !super::checks::checks(&run).is_empty()
                && super::checks::checks(&run)
                    .iter()
                    .all(|c| c.status == "pass"),
            "Saved results failed integrity checks"
        );
        let assessment_retry = super::p2::can_retry_assessment(&run);
        if assessment_retry {
            super::p2::request_for_assessment(&run)?;
        } else {
            super::p2::request(&run)?;
        }
        run.status = "verifying".into();
        run.error = None;
        run.verifier = None;
        run.event(
            "verifying",
            if assessment_retry {
                "Assessment retry requested; reusing original simulation results and saved sources"
            } else {
                "Verification retry requested; reusing the original simulation results"
            },
        );
        db.execute(
            "UPDATE experiment_runs SET body=? WHERE workspace=? AND id=?",
            params![serde_json::to_string(&run)?, ws, id],
        )?;
        Ok(Some(run))
    }
    pub fn cancel(&self, ws: &str, id: &str) -> Result<Option<Run>> {
        let db = self.0.lock().unwrap();
        let body: Option<String> = db
            .query_row(
                "SELECT body FROM experiment_runs WHERE workspace=? AND id=?",
                params![ws, id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(body) = body else { return Ok(None) };
        let mut r: Run = serde_json::from_str(&body)?;
        if [
            "planning",
            "simulating",
            "checking",
            "repairing",
            "verifying",
            "researching",
            "synthesizing",
        ]
        .contains(&r.status.as_str())
        {
            r.status = "cancelled".into();
            r.event("cancelled","Cancelled. An in-flight request may finish, but its result cannot complete this run.");
            db.execute(
                "UPDATE experiment_runs SET body=? WHERE workspace=? AND id=?",
                params![serde_json::to_string(&r)?, ws, id],
            )?;
        }
        Ok(Some(r))
    }
    pub fn list(&self, ws: &str) -> Result<Vec<serde_json::Value>> {
        let db = self.0.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT body FROM experiment_runs WHERE workspace=? ORDER BY rowid DESC LIMIT 30",
        )?;
        let rows = stmt.query_map([ws], |r| r.get::<_, String>(0))?;
        rows.map(|s| {
            let r: Run = serde_json::from_str(&s?)?;
            Ok(serde_json::json!({"run_id":r.run_id,"goal":r.goal,"status":r.status,"mode":r.mode}))
        })
        .collect()
    }
}
