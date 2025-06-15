use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Task {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub stream: Option<usize>,
    pub initial_delay: Option<f64>,
    #[serde(default)]
    pub splits: Vec<crate::cli::SplitPoint>,
    #[serde(default)]
    pub split_ranges: Vec<crate::cli::SplitRange>,
    pub bitrate: Option<String>,
    pub silence_threshold: Option<f64>,
    /// If true, fit the edited audio stream to the original length (trim or pad with silence at the end as needed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit_length: Option<bool>,
}

impl Task {
    pub fn load(path: Option<&str>) -> anyhow::Result<Option<Self>> {
        if let Some(path) = path {
            let contents = std::fs::read_to_string(path)?;
            let task: Task = serde_json::from_str(&contents)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }
}
