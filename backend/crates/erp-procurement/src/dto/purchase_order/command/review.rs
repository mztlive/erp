use serde::{Deserialize, Serialize};
use validator::Validate;

/// 撤回采购单审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchaseOrderApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 财务审核结果（通过/驳回共用形状）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReviewResult {
    /// 已完成的审核待办。
    pub work_item_id: String,
    /// 待办终态，固定为 `COMPLETED`。
    pub work_item_status: String,
    /// 完成后的待办版本。
    pub task_version: String,
    /// 本次审核锁定的不可变采购提交版本。
    pub subject_version: String,
    /// 审核结论（`APPROVED`/`REJECTED`）。
    pub review_result: String,
    /// 通过时形成的生效版本。
    pub revision_id: Option<String>,
    /// 通过时形成的版本号。
    pub revision_no: Option<u32>,
    /// 通过时形成的应付分录。
    pub payable_entry_id: Option<String>,
    /// 新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

impl PurchaseReviewResult {
    /// 由必填待办身份、版本与审核结论构造财务审核结果。
    ///
    /// # 参数
    /// * `work_item_id` - 已完成的审核待办
    /// * `work_item_status` - 待办终态
    /// * `task_version` - 完成后的待办版本
    /// * `subject_version` - 本次审核锁定的采购提交版本
    /// * `review_result` - 审核结论（`APPROVED`/`REJECTED`）
    /// * `reference` - 业务引用
    ///
    /// # 返回
    /// 返回生效版本与应付分录为空、零锁版本的审核结果。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        work_item_id: impl Into<String>,
        work_item_status: impl Into<String>,
        task_version: impl Into<String>,
        subject_version: impl Into<String>,
        review_result: impl Into<String>,
        reference: impl Into<String>,
    ) -> Self {
        Self {
            work_item_id: work_item_id.into(),
            work_item_status: work_item_status.into(),
            task_version: task_version.into(),
            subject_version: subject_version.into(),
            review_result: review_result.into(),
            revision_id: None,
            revision_no: None,
            payable_entry_id: None,
            lock_version: 0,
            reference: reference.into(),
        }
    }

    /// 设置通过时形成的生效版本。
    ///
    /// # 参数
    /// * `revision_id` - 生效版本 ID
    ///
    /// # 返回
    /// 返回更新后的审核结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_revision_id(mut self, revision_id: impl Into<String>) -> Self {
        self.revision_id = Some(revision_id.into());
        self
    }

    /// 设置通过时形成的版本号。
    ///
    /// # 参数
    /// * `revision_no` - 版本号
    ///
    /// # 返回
    /// 返回更新后的审核结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_revision_no(mut self, revision_no: u32) -> Self {
        self.revision_no = Some(revision_no);
        self
    }

    /// 设置通过时形成的应付分录。
    ///
    /// # 参数
    /// * `payable_entry_id` - 应付分录 ID
    ///
    /// # 返回
    /// 返回更新后的审核结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_payable_entry_id(mut self, payable_entry_id: impl Into<String>) -> Self {
        self.payable_entry_id = Some(payable_entry_id.into());
        self
    }

    /// 设置新乐观锁版本。
    ///
    /// # 参数
    /// * `lock_version` - 新乐观锁版本
    ///
    /// # 返回
    /// 返回更新后的审核结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_lock_version(mut self, lock_version: u64) -> Self {
        self.lock_version = lock_version;
        self
    }
}
