//! 采购责任解析的业务场景。
use serde::{Deserialize, Serialize};

/// 责任解析业务场景；询价场景才使用整单询价规则。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionPurpose {
    /// 普通采购任务。
    #[default]
    Procurement,
    /// 询价整单任务。
    Inquiry,
}
