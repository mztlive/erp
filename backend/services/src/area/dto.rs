use serde::{Deserialize, Serialize};

/// 行政区节点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AreaNode {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub children: Vec<AreaNode>,
}
