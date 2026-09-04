use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    role: Option<String>,
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default, rename = "costUsd")]
    pub cost_usd: f64,
}

#[derive(Default, Clone, Copy)]
pub struct Totals {
    pub requests: u64,
    pub usage: Usage,
}

impl Totals {
    fn add(&mut self, u: &Usage) {
        self.requests += 1;
        self.usage.input_tokens += u.input_tokens;
        self.usage.output_tokens += u.output_tokens;
        self.usage.cache_read_tokens += u.cache_read_tokens;
        self.usage.cache_write_tokens += u.cache_write_tokens;
        self.usage.cost_usd += u.cost_usd;
    }
}

/// day → totals (UTC date from message timestamp)
pub type ByDay = BTreeMap<String, Totals>;
/// model → totals
pub type ByModel = BTreeMap<String, Totals>;
/// project (dir name) → totals
pub type ByProject = BTreeMap<String, Totals>;

pub struct LocalData {
    pub by_day: ByDay,
    pub by_model: ByModel,
    pub by_project: ByProject,
    pub sessions: u64,
    pub total: Totals,
}

fn data_dir() -> PathBuf {
    let home: PathBuf = std::env::var("HOME").unwrap_or_else(|_| "/".into()).into();
    home.join(".commandcode/projects")
}

/// day key = UTC YYYY-MM-DD from ISO timestamp
fn day_of(ts: &str) -> Option<String> {
    ts.get(0..10).map(|s| s.to_string())
}

pub fn load_local() -> LocalData {
    let mut data = LocalData {
        by_day: ByDay::new(),
        by_model: ByModel::new(),
        by_project: ByProject::new(),
        sessions: 0,
        total: Totals::default(),
    };

    let dir = data_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return data;
    };

    for proj in entries.flatten() {
        let proj_name = proj.file_name().to_string_lossy().to_string();
        let Ok(files) = std::fs::read_dir(proj.path()) else {
            continue;
        };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            // skip checkpoints and non-sessions
            if name.ends_with(".meta.json") || !name.ends_with(".jsonl") {
                continue;
            }
            if name.contains("checkpoints") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(f.path()) else {
                continue;
            };
            let mut is_session_file = false;
            for line in text.lines() {
                let Ok(l) = serde_json::from_str::<Line>(line) else {
                    continue;
                };
                match l.kind.as_str() {
                    "session" => {
                        data.sessions += 1;
                        is_session_file = true;
                    }
                    "message" => {
                        let Some(u) = l.usage else { continue };
                        // only assistant (model) messages carry usage; guard anyway
                        if l.message.as_ref().and_then(|m| m.role.as_deref()) == Some("user") {
                            continue;
                        }
                        let bucket = |t: &mut Totals| t.add(&u);
                        if let Some(day) = day_of(&l.timestamp) {
                            bucket(data.by_day.entry(day).or_default());
                        }
                        if let Some(m) = &l.model {
                            bucket(data.by_model.entry(m.clone()).or_default());
                        }
                        if is_session_file {
                            bucket(data.by_project.entry(proj_name.clone()).or_default());
                        }
                        bucket(&mut data.total);
                    }
                    _ => {}
                }
            }
        }
    }
    data
}
