//! 采购付款申请写命令及查询合同。审核人与主体仅由服务端来源装配。

use std::collections::HashSet;

use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::payable::{PurchasePaymentRequestData, PurchasePaymentRequestStatus};

/// 创建或修改尚未批准的申请草稿。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePurchasePaymentRequest {
    pub expected_version: Option<u64>,
    pub data: PurchasePaymentRequestData,
}

/// 原子提交并占用逐采购余额。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitPurchasePaymentRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
}

/// 撤回或关闭未付部分，保留原命令身份恢复未知结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchasePaymentRequest {
    pub expected_version: u64,
    pub reason: String,
    pub idempotency_key: String,
}

/// 普通列表和任务读取的有界筛选条件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurchasePaymentRequestQuery {
    pub scope_version: Option<String>,
    pub supplier_id: Option<String>,
    pub purchase_order_id: Option<String>,
    pub payable_account_id: Option<String>,
    pub work_item_id: Option<String>,
    pub created_by: Option<String>,
    pub status: Option<PurchasePaymentRequestStatus>,
    pub q: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}

/// 一次操作原子创建并提交多份独立申请，命令身份用于恢复原结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaymentRequestBatch {
    pub idempotency_key: String,
    pub requests: Vec<PurchasePaymentRequestData>,
}

impl CreatePaymentRequestBatch {
    /// 规范化批量表单并限制重复来源及批次体积。
    /// # 参数
    /// 消费当前批量命令。
    /// # 返回
    /// 返回规范化后的完整批次。
    /// # 错误
    /// 空批次、超过 50 份、重复应付来源或任一表单非法时拒绝。
    pub fn normalize(mut self) -> Result<Self> {
        if self.requests.is_empty() || self.requests.len() > 50 {
            return Err(Error::from("每次须创建 1 至 50 份付款申请"));
        }
        self.idempotency_key = self.idempotency_key.trim().to_owned();
        if self.idempotency_key.is_empty() || self.idempotency_key.len() > 160 {
            return Err(Error::from("请提供有效批量操作号"));
        }
        let mut sources = HashSet::new();
        self.requests = self
            .requests
            .into_iter()
            .map(PurchasePaymentRequestData::normalize)
            .collect::<Result<Vec<_>>>()?;
        for request in &self.requests {
            for line in &request.lines {
                if !sources.insert(&line.payable_account_id) {
                    return Err(Error::from("同一采购应付不能在本批次重复申请"));
                }
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
mod batch_tests {
    use serde_json::json;

    use super::*;

    fn request(source: &str) -> PurchasePaymentRequestData {
        serde_json::from_value(json!({
            "supplier_id": "supplier", "payee_bank_account_id": "bank",
            "lines": [{"payable_account_id": source, "amount": "30.00"}],
            "desired_payment_date": "2026-10-15", "reason": " 预付款 ", "attachment_ids": []
        }))
        .unwrap()
    }

    #[test]
    fn normalizes_distinct_sources_and_rejects_invalid_batches() {
        let batch = |requests| CreatePaymentRequestBatch { idempotency_key: "batch".into(), requests };
        let result = batch(vec![request("a"), request("b")]).normalize().unwrap();
        assert_eq!(result.requests.len(), 2);
        assert_eq!(result.requests[0].reason, "预付款");
        assert!(batch(vec![]).normalize().is_err());
        assert!(batch(vec![request("a"), request("a")]).normalize().is_err());
        assert!(batch((0..51).map(|i| request(&format!("p-{i}"))).collect()).normalize().is_err());
        let mut invalid = request("a");
        invalid.reason.clear();
        assert!(batch(vec![request("b"), invalid]).normalize().is_err());
    }
    #[test]
    fn batch_transport_rejects_client_subject_and_invalid_money() {
        let mut data = serde_json::to_value(request("a")).unwrap();
        data["payer_party_id"] = json!("invented-company");
        assert!(
            serde_json::from_value::<CreatePaymentRequestBatch>(json!({
                "idempotency_key": "key", "requests": [data]
            }))
            .is_err()
        );
        let mut data = serde_json::to_value(request("a")).unwrap();
        data["lines"][0]["amount"] = json!("not-money");
        assert!(
            serde_json::from_value::<CreatePaymentRequestBatch>(json!({
                "idempotency_key": "key", "requests": [data]
            }))
            .is_err()
        );
        assert!(
            CreatePaymentRequestBatch { idempotency_key: " ".into(), requests: vec![request("a")] }
                .normalize()
                .is_err()
        );
    }
}
