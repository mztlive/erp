//! 调用方解析动态指派时提供的不透明来源快照。
use serde::{Deserialize, Serialize};

use super::Timestamp;

/// 实例固定的来源依据；BPM 不解释来源资源或查询外部系统。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssigneeResolution {
    pub rule: String,
    pub subject_id: String,
    pub source_id: String,
    pub source_name: String,
    pub assignment_id: String,
    pub participant_name: String,
    pub resolved_at: Timestamp,
}
