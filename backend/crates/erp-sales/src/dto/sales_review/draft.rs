//! 销售变更原单的草稿编辑协议。

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sales_order::SalesOrderWorkingCopyLineData;

/// 可重新编辑的销售变更工作副本。
#[derive(Debug, Clone, Serialize)]
pub struct SalesChangeDraftView {
    /// 销售变更单版本。
    pub version: u64,
    /// 工作副本版本。
    pub working_copy_version: u64,
    /// 完整目标身份；用于结果未知时核对原单已经冻结的提交。
    pub content_hash: String,
    /// 变更原因。
    pub reason: String,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 完整目标明细；稳定行身份保留。
    pub lines: Vec<SalesOrderWorkingCopyLineData>,
}

/// 保存原销售变更单的目标内容。
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SaveSalesChangeDraftRequest {
    /// 页面读取的变更单版本。
    #[validate(range(min = 1))]
    pub expected_version: u64,
    /// 页面读取的工作副本版本。
    #[validate(range(min = 1))]
    pub expected_working_copy_version: u64,
    /// 本次变更原因。
    #[validate(length(min = 1, max = 512))]
    pub reason: String,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 完整目标明细。
    #[validate(length(min = 1, max = 100))]
    pub lines: Vec<SalesOrderWorkingCopyLineData>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_edit_protocol_requires_both_versions_and_rejects_unknown_fields() {
        let body = serde_json::json!({
            "expected_version": 2, "expected_working_copy_version": 3,
            "reason": "按驳回意见修改", "business_remark": null, "lines": []
        });
        let request: SaveSalesChangeDraftRequest = serde_json::from_value(body.clone()).unwrap();
        assert!(request.validate().is_err(), "不得提交空明细");
        for required in ["expected_version", "expected_working_copy_version"] {
            let mut missing = body.clone();
            missing.as_object_mut().unwrap().remove(required);
            assert!(serde_json::from_value::<SaveSalesChangeDraftRequest>(missing).is_err());
        }
        let mut unknown = body;
        unknown["sales_order_id"] = serde_json::json!("foreign-source");
        assert!(serde_json::from_value::<SaveSalesChangeDraftRequest>(unknown).is_err());
    }
}
