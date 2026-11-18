use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::command::SavePurchaseOrderLine;
use crate::Result;
use crate::entity::purchase_order::{
    PurchaseCommandReceipt, PurchaseCommandReceiptIdentity, payload_fingerprint,
};

/// 采购变更提交命令动作。
pub const PURCHASE_CHANGE_SUBMIT_ACTION: &str = "purchase_change_order.submit";

/// 撤回采购变更审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchaseChangeApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "单据或资料版本无效，请刷新当前页面后再试"))]
    pub expected_lock_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "本次操作缺少提交标识，请刷新页面后重新操作"))]
    pub idempotency_key: String,
}

/// 发起采购变更请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StartPurchaseChangeRequest {
    /// 期望的采购单乐观锁版本。
    #[validate(range(min = 1, message = "单据或资料版本无效，请刷新当前页面后再试"))]
    pub expected_lock_version: u64,
    /// 采购变化原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "本次操作缺少提交标识，请刷新页面后重新操作"))]
    pub idempotency_key: String,
}

/// 发起采购变更结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StartPurchaseChangeResult {
    /// 变更单主键。
    pub change_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 基准版本号。
    pub base_revision_no: u32,
    /// 采购单新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 提交采购变更目标内容请求（完整头、行及销售分配）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitPurchaseChangeRequest {
    /// 驳回后重新提交的必填说明。
    #[serde(default)]
    #[validate(length(max = 1000, message = "重新提交说明最多1000个字符"))]
    pub resubmission_reason: Option<String>,
    /// 期望的变更单乐观锁版本。
    #[validate(range(min = 1, message = "单据或资料版本无效，请刷新当前页面后再试"))]
    pub expected_lock_version: u64,
    /// 目标付款条件；缺省沿用基准版本快照。
    pub payment_term_code: Option<String>,
    /// 变更原因；缺省沿用变更单已有原因。
    #[serde(default)]
    #[validate(length(max = 500, message = "变更原因最多500个字符"))]
    pub reason: Option<String>,
    /// 目标完整行集合。
    pub lines: Vec<SavePurchaseOrderLine>,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "本次操作缺少提交标识，请刷新页面后重新操作"))]
    pub idempotency_key: String,
}

impl SubmitPurchaseChangeRequest {
    /// 构造与当前状态无关的精确提交命令身份。
    /// # 参数
    /// * `change_id` / `actor_id` - 已授权变更单及当前提交人。
    /// # 返回
    /// 返回稳定幂等身份。
    /// # 错误
    /// 身份为空时拒绝。
    pub fn command_identity(
        &self,
        change_id: &str,
        actor_id: &str,
    ) -> Result<PurchaseCommandReceiptIdentity> {
        Ok(PurchaseCommandReceipt::<PurchaseChangeSubmitResult>::identity(
            "purchase-change-submit-",
            actor_id,
            PURCHASE_CHANGE_SUBMIT_ACTION,
            Some(change_id),
            self.idempotency_key.trim(),
        )?)
    }

    /// 冻结原请求版本、完整目标及说明，避免相同键回放不同内容。
    /// # 参数
    /// * `change_id` - 已授权变更单。
    /// # 返回
    /// 返回不包含幂等键的完整请求摘要。
    /// # 错误
    /// 请求序列化失败时拒绝。
    pub fn request_fingerprint(&self, change_id: &str) -> Result<String> {
        Ok(payload_fingerprint(
            PURCHASE_CHANGE_SUBMIT_ACTION,
            change_id,
            &(
                self.expected_lock_version,
                &self.payment_term_code,
                &self.reason,
                &self.lines,
                self.resubmission_reason.as_deref().map(str::trim),
            ),
        )?)
    }
}

/// 作废采购变更草稿请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct VoidPurchaseChangeRequest {
    /// 期望的变更单乐观锁版本。
    #[validate(range(min = 1, message = "单据或资料版本无效，请刷新当前页面后再试"))]
    pub expected_lock_version: u64,
}

/// 采购变更提交结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseChangeSubmitResult {
    /// 变更单主键。
    pub change_id: String,
    /// 形成的不可变目标提交。
    pub submission_id: String,
    /// 提交序号。
    pub submission_no: String,
    /// 变更单状态。
    pub status: String,
    /// 变更单新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 采购变更生效请求（§8.1.3：基准版本校验 + 新版本 + 差额 + 指针推进）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EffectPurchaseChangeRequest {
    /// 期望的变更单乐观锁版本。
    #[validate(range(min = 1, message = "单据或资料版本无效，请刷新当前页面后再试"))]
    pub expected_lock_version: u64,
    /// 目标提交。
    #[validate(custom(function = "non_blank", message = "提交ID不能为空"))]
    pub submission_id: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "本次操作缺少提交标识，请刷新页面后重新操作"))]
    pub idempotency_key: String,
}

/// 采购变更生效结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeEffectResult {
    /// 变更单主键。
    pub change_id: String,
    /// 形成的新采购版本。
    pub revision_id: String,
    /// 新版本号。
    pub revision_no: u32,
    /// 追加的应付差额分录。
    pub payable_delta_entry_id: Option<String>,
    /// 采购单新乐观锁版本。
    pub purchase_order_lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 采购变更单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseChangeOrderListParams {
    /// 跨页必须使用前一页的当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 原采购单筛选。
    pub purchase_order_id: Option<String>,
    /// 状态筛选。
    pub status: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

#[cfg(test)]
mod submit_tests {
    use serde_json::json;

    use super::*;
    use crate::entity::purchase_order::PurchaseCommandReceiptError;

    fn request() -> SubmitPurchaseChangeRequest {
        SubmitPurchaseChangeRequest {
            expected_lock_version: 4,
            payment_term_code: Some("NET-30".into()),
            reason: Some("调整采购".into()),
            resubmission_reason: Some("补充依据".into()),
            lines: vec![
                serde_json::from_value(json!({
                    "line_type": "ITEM_SERVICE", "quantity": "2", "unit_cost_gross": "3"
                }))
                .unwrap(),
            ],
            idempotency_key: "retry-key".into(),
        }
    }

    /// 首次结果经真实回执解码可重放，改原锁版本、完整行或任一说明均冲突。
    #[test]
    fn exact_change_submission_replays_and_changed_payload_conflicts() {
        let request = request();
        let identity = request.command_identity("change-1", "actor").unwrap();
        let fingerprint = request.request_fingerprint("change-1").unwrap();

        let result = PurchaseChangeSubmitResult {
            change_id: "change-1".into(),
            submission_id: "submission-2".into(),
            submission_no: "CS-000002".into(),
            status: "IN_APPROVAL".into(),
            lock_version: 5,
            reference: "CS-CS-000002".into(),
        };
        let receipt =
            PurchaseCommandReceipt::new(&identity, &fingerprint, result.clone(), "audit".into()).unwrap();
        let stored = serde_json::to_value(&receipt).unwrap();
        let decoded = PurchaseCommandReceipt::<PurchaseChangeSubmitResult>::decode(
            serde_json::from_value(stored).unwrap(),
            &identity,
            &fingerprint,
        )
        .unwrap();
        assert_eq!(decoded.into_payload(), result);

        for changed in [
            {
                let mut value = request.clone();
                value.expected_lock_version += 1;
                value
            },
            {
                let mut value = request.clone();
                value.payment_term_code = None;
                value
            },
            {
                let mut value = request.clone();
                value.reason = Some("其他原因".into());
                value
            },
            {
                let mut value = request.clone();
                value.resubmission_reason = Some("其他说明".into());
                value
            },
            {
                let mut value = request.clone();
                value.lines[0].quantity = Some("3".into());
                value
            },
            {
                let mut value = request.clone();
                value.lines[0].unit_cost_gross = Some("4".into());
                value
            },
            {
                let mut value = request.clone();
                value.lines[0].expected_delivery_date = Some("2026-12-01".into());
                value
            },
        ] {
            assert_eq!(changed.command_identity("change-1", "actor").unwrap(), identity);
            assert_eq!(
                PurchaseCommandReceipt::decode(
                    receipt.clone(),
                    &identity,
                    &changed.request_fingerprint("change-1").unwrap()
                )
                .unwrap_err(),
                PurchaseCommandReceiptError::PayloadConflict
            );
        }
    }

    /// 重提说明在统一的一千字符边界内接受，超限拒绝。
    #[test]
    fn resubmission_reason_uses_shared_thousand_character_limit() {
        let mut request = request();
        request.resubmission_reason = Some("说".repeat(1000));
        request.validate().unwrap();
        request.resubmission_reason = Some("说".repeat(1001));
        assert!(request.validate().is_err());
    }
}
