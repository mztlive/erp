//! `FactBase`：正式事实公共字段（P0-1.4 共享基元任务）。
//!
//! 对应数据模型 4.3「不可变修订和正式事实」：`fact_no`、`occurred_at`、`recorded_at`、
//! `recorded_by`、`source_type`、`source_reference`、`reason_code`、`reason_text`。
//! 正式事实不可变：冲正与纠错通过反向事实表达，不修改已记录事实。

use serde::{Deserialize, Serialize};

use super::source::SourceType;
use super::time::Instant;

/// 正式事实的公共字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactBase {
    /// 聚合内稳定序号。
    pub fact_no: String,
    /// 业务实际发生时间。
    pub occurred_at: Instant,
    /// ERP 记录时间。
    pub recorded_at: Instant,
    /// ERP 记录人或系统身份。
    pub recorded_by: String,
    /// 事实来源类型。
    pub source_type: SourceType,
    /// 可追溯的来源单据或消息引用。
    pub source_reference: Option<String>,
    /// 变更、纠错或人工处理原因代码；适用时必填。
    pub reason_code: Option<String>,
    /// 原因说明文本。
    pub reason_text: Option<String>,
}

impl FactBase {
    /// 创建正式事实公共字段。
    ///
    /// # 参数
    /// * `fact_no` - 聚合内稳定序号
    /// * `occurred_at` - 业务实际发生时间
    /// * `recorded_at` - ERP 记录时间
    /// * `recorded_by` - 记录人或系统身份
    /// * `source_type` - 事实来源类型
    /// * `source_reference` - 来源单据或消息引用（可为空）
    /// * `reason_code` - 原因代码（可为空）
    /// * `reason_text` - 原因说明（可为空）
    ///
    /// # 返回
    /// 返回事实公共字段实例。
    // 字段数由数据模型 4.3 固定，8 个参数与字段一一对应，不拆 builder。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fact_no: impl Into<String>,
        occurred_at: Instant,
        recorded_at: Instant,
        recorded_by: impl Into<String>,
        source_type: SourceType,
        source_reference: Option<String>,
        reason_code: Option<String>,
        reason_text: Option<String>,
    ) -> Self {
        Self {
            fact_no: fact_no.into(),
            occurred_at,
            recorded_at,
            recorded_by: recorded_by.into(),
            source_type,
            source_reference,
            reason_code,
            reason_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_serde_roundtrip() {
        let base = FactBase::new(
            "F-100",
            Instant::from_unix_secs(1_700_000_000),
            Instant::from_unix_secs(1_700_000_100),
            "system",
            SourceType::MallSync,
            Some("msg-1".to_string()),
            None,
            Some("人工补录".to_string()),
        );
        assert_eq!(base.fact_no, "F-100");
        assert_eq!(base.source_type, SourceType::MallSync);
        assert_eq!(base.occurred_at.unix_secs(), 1_700_000_000);
        assert!(base.reason_code.is_none());

        let json = serde_json::to_string(&base).unwrap();
        let back: FactBase = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recorded_by, "system");
        assert_eq!(back.source_type, SourceType::MallSync);
        assert_eq!(back.recorded_at.unix_secs(), 1_700_000_100);
        assert_eq!(back.reason_text.as_deref(), Some("人工补录"));
    }
}
