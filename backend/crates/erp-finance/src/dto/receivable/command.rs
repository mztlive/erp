//! 应收往来子账与客户回款的写入命令 DTO。

use application_core::non_blank;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{CustomerAccountId, PartyId, ReceivableEntryId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::receivable::AccountReviewStatus;

/// 应收往来子账创建请求（W11「从销售单登记应收」：子账 + 原始应收分录）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateReceivableAccountRequest {
    /// 来源销售单。
    pub sales_order_id: String,
    /// 往来子账序号（同一销售单内从 1 递增）。
    #[validate(range(min = 1, message = "往来子账序号必须从 1 开始"))]
    pub account_seq: u32,
    /// 企业客户经营归属。
    pub customer_id: CustomerAccountId,
    /// 收款和开票往来主体。
    pub counterparty_party_id: PartyId,
    /// 卡券票款复核状态缓存；缺省时由来源销售单业务性质派生，传入值必须与派生值一致。
    #[serde(default)]
    pub review_status: Option<AccountReviewStatus>,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 可开票含税总额（缺省等于含税应收总额）。
    #[serde(default)]
    pub invoiceable_total: Option<Amount>,
    /// 到期日（`YYYY-MM-DD`）。
    pub due_date: BusinessDate,
    /// 来源销售修订 ID（作为分录来源修订）。
    pub source_sales_order_revision_id: String,
    /// 来源单据序号（分录来源内序号，从 1 开始）。
    #[validate(range(min = 1, message = "来源内序号必须从 1 开始"))]
    pub source_sequence: u32,
}

// ---------------------------------------------------------------------------
// 客户回款单（customer_receipt）
// ---------------------------------------------------------------------------

/// 客户回款单创建请求（W11 登记草稿回款；过账与分配走 `post`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCustomerReceiptRequest {
    /// 回款单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "回款单号不能为空"))]
    pub receipt_no: String,
    /// 实际付款往来主体。
    pub counterparty_party_id: PartyId,
    /// 可选经营归属提示。
    pub customer_id: Option<CustomerAccountId>,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
}

/// 回款核销分配请求行（§8.3-1：同一往来主体、净分配不超过回款金额）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReceiptAllocationLineRequest {
    /// 被核销应收分录。
    pub receivable_entry_id: ReceivableEntryId,
    /// 本次核销金额（正数）。
    pub allocated_amount: Amount,
}

/// 客户回款过账请求（仅由最终通过动作内部消费冻结分配；HTTP 旁路已关闭）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostCustomerReceiptRequest {
    /// 核销分配行（允许保留未分配余额）。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<ReceiptAllocationLineRequest>,
}

/// 客户回款提交审批请求。客户端不得选择定义或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitCustomerReceiptRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
    /// 提交时冻结的待过账核销分配。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<ReceiptAllocationLineRequest>,
}

/// 客户回款原子创建并提交审批请求。
///
/// 已有草稿提交 `receipt_id + expected_version`；新登记提交完整 `receipt`。
/// 服务端在一个事务内完成绑定、创建、冻结分配、审批启动与审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitCustomerReceiptRequest {
    /// 已有回款草稿主键。
    pub receipt_id: Option<String>,
    /// 已有草稿期望乐观锁版本。
    pub expected_version: Option<u64>,
    /// 新回款完整字段；提交已有草稿时为空。
    pub receipt: Option<CreateCustomerReceiptRequest>,
    /// 提交时冻结的待过账核销分配。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<ReceiptAllocationLineRequest>,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl CommitCustomerReceiptRequest {
    /// 以幂等键构造原子提交请求，其余字段保持空条件由链式方法补齐。
    ///
    /// # 参数
    /// * `idempotency_key` - 业务请求幂等键
    ///
    /// # 返回
    /// 返回仅携带幂等键的提交请求。
    ///
    /// # 错误
    /// 无。
    pub fn new(idempotency_key: impl Into<String>) -> Self {
        Self {
            receipt_id: None,
            expected_version: None,
            receipt: None,
            allocations: Vec::new(),
            idempotency_key: idempotency_key.into(),
        }
    }

    /// 设置已有回款草稿主键。
    ///
    /// # 参数
    /// * `receipt_id` - 已有回款草稿主键
    ///
    /// # 返回
    /// 返回更新后的提交请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_receipt_id(mut self, receipt_id: impl Into<String>) -> Self {
        self.receipt_id = Some(receipt_id.into());
        self
    }

    /// 设置已有草稿期望乐观锁版本。
    ///
    /// # 参数
    /// * `expected_version` - 期望乐观锁版本
    ///
    /// # 返回
    /// 返回更新后的提交请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_expected_version(mut self, expected_version: u64) -> Self {
        self.expected_version = Some(expected_version);
        self
    }

    /// 设置新回款完整字段。
    ///
    /// # 参数
    /// * `receipt` - 新回款完整字段
    ///
    /// # 返回
    /// 返回更新后的提交请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_receipt(mut self, receipt: CreateCustomerReceiptRequest) -> Self {
        self.receipt = Some(receipt);
        self
    }

    /// 设置提交时冻结的待过账核销分配。
    ///
    /// # 参数
    /// * `allocations` - 待过账核销分配行
    ///
    /// # 返回
    /// 返回更新后的提交请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_allocations(mut self, allocations: Vec<ReceiptAllocationLineRequest>) -> Self {
        self.allocations = allocations;
        self
    }
}

/// 撤回客户回款审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelCustomerReceiptApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::CommitCustomerReceiptRequest;

    #[test]
    fn commit_request_new_carries_only_idempotency_key() {
        let request = CommitCustomerReceiptRequest::new("idem-1");
        assert_eq!(request.idempotency_key, "idem-1");
        assert_eq!(request.receipt_id, None);
        assert_eq!(request.expected_version, None);
        assert!(request.receipt.is_none());
        assert!(request.allocations.is_empty());
        assert!(request.validate().is_err());
    }

    #[test]
    fn commit_request_chainable_setters_fill_optional_fields() {
        let request =
            CommitCustomerReceiptRequest::new("idem-2").with_receipt_id("cr-1").with_expected_version(2);
        assert_eq!(request.receipt_id.as_deref(), Some("cr-1"));
        assert_eq!(request.expected_version, Some(2));
        assert_eq!(request.idempotency_key, "idem-2");
    }
}
