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
    /// 记录正式事实时组合稳定序号、实际发生时间、ERP 记录时间、记录人和来源。调用方须先完成裁剪、非空及长度校验。
    /// 已经记录的事实不可修改；冲正和纠错须追加反向事实。
    pub fn new(
        fact_no: impl Into<String>,
        occurred_at: Instant,
        recorded_at: Instant,
        recorded_by: impl Into<String>,
        source: FactSource,
    ) -> Self {
        Self {
            fact_no: fact_no.into(),
            occurred_at,
            recorded_at,
            recorded_by: recorded_by.into(),
            source_type: source.source_type,
            source_reference: source.source_reference,
            reason_code: source.reason_code,
            reason_text: source.reason_text,
        }
    }
}

/// 正式事实的来源与原因字段组。
pub struct FactSource {
    /// 事实来源类型。
    pub source_type: SourceType,
    /// 可追溯的来源单据或消息引用。
    pub source_reference: Option<String>,
    /// 变更、纠错或人工处理原因代码；适用时必填。
    pub reason_code: Option<String>,
    /// 原因说明文本。
    pub reason_text: Option<String>,
}

impl FactSource {
    /// 登记正式事实前，把必填来源类型与适用的来源引用、原因代码和说明组合起来。这里不裁剪或校验文本；
    /// 调用本构造及 [`FactBase::new`] 前，由领域按场景校验引用和原因。
    pub fn new(
        source_type: SourceType,
        source_reference: Option<String>,
        reason_code: Option<String>,
        reason_text: Option<String>,
    ) -> Self {
        Self { source_type, source_reference, reason_code, reason_text }
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
            FactSource::new(
                SourceType::ManualImport,
                Some("msg-1".to_string()),
                None,
                Some("人工补录".to_string()),
            ),
        );
        assert_eq!(base.fact_no, "F-100");
        assert_eq!(base.source_type, SourceType::ManualImport);
        assert_eq!(base.occurred_at.unix_secs(), 1_700_000_000);
        assert!(base.reason_code.is_none());

        let json = serde_json::to_string(&base).unwrap();
        let back: FactBase = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recorded_by, "system");
        assert_eq!(back.source_type, SourceType::ManualImport);
        assert_eq!(back.recorded_at.unix_secs(), 1_700_000_100);
        assert_eq!(back.reason_text.as_deref(), Some("人工补录"));
    }
}
