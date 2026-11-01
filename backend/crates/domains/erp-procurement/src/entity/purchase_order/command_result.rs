//! 采购命令首次成功结果；由采购领域拥有并随独立回执不可变保存。

use serde::{Deserialize, Serialize};

use super::PurchaseOrder;
use crate::dto::purchase_order::{
    CreatePurchaseOrderResult, ExistingStockReservationResult, SavePurchaseOrderDraftResult, TotalsView,
    VoidPurchaseOrderResult,
};

/// 供给分配提交时冻结的任务状态，独立于工作流实体。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourcingTaskStatus {
    /// 仍有未分配供给。
    Open,
    /// 所有供给已分配。
    Completed,
}
impl SourcingTaskStatus {
    /// 返回客户端合同状态文本。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回首次提交时冻结的状态代码。
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Completed => "COMPLETED",
        }
    }
}
/// 幂等命令收据载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreationReceipt {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 创建完成时乐观锁版本。
    pub lock_version: u64,
}

impl CreationReceipt {
    /// 转换为采购创建响应。
    ///
    /// # 参数
    /// * `replayed` - 是否来自幂等收据回放
    ///
    /// # 返回
    /// 返回 API 创建结果。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 业务引用恒为原采购单 ID。
    pub fn into_result(self, replayed: bool) -> CreatePurchaseOrderResult {
        CreatePurchaseOrderResult::new(
            self.purchase_order_id.clone(),
            self.purchase_no,
            self.purchase_order_id,
        )
        .with_lock_version(self.lock_version)
        .with_replayed(replayed)
    }
}

/// 保存草稿命令收据载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveDraftReceipt {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 保存完成时的乐观锁版本。
    pub lock_version: u64,
    /// 保存完成时的含税金额。
    pub gross: String,
    /// 保存完成时的不含税金额。
    pub net: String,
    /// 保存完成时的税额。
    pub tax: String,
    /// 首次成功响应的业务引用。
    pub reference: String,
}

impl SaveDraftReceipt {
    /// 从已持久化采购单和新草稿金额构造稳定收据。
    ///
    /// # 参数
    /// * `order` - Repository 更新后带新版本的采购单
    /// * `gross` - 首次保存含税金额
    /// * `net` - 首次保存不含税金额
    /// * `tax` - 首次保存税额
    ///
    /// # 返回
    /// 返回可持久化并稳定回放的保存结果载荷。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 版本和业务引用必须与首次成功响应完全一致。
    pub fn from_saved(order: &PurchaseOrder, gross: String, net: String, tax: String) -> Self {
        Self {
            purchase_order_id: order.base.id.clone(),
            lock_version: order.base.version,
            gross,
            net,
            tax,
            reference: format!("SAVED-V{}", order.base.version),
        }
    }

    /// 转换为保存草稿 API 结果。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回首次执行与后续回放共享的原始结果。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 回放不得使用采购单当前版本或重新计算金额覆盖收据结果。
    pub fn into_result(self) -> SavePurchaseOrderDraftResult {
        SavePurchaseOrderDraftResult {
            lock_version: self.lock_version,
            totals: TotalsView { gross: self.gross, net: self.net, tax: self.tax },
            reference: self.reference,
        }
    }
}

/// 作废采购草稿命令收据载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoidDraftReceipt {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 作废后的稳定状态。
    pub status: String,
    /// 作废完成时的乐观锁版本。
    pub lock_version: u64,
    /// 首次执行时规范化的作废原因。
    pub reason: String,
    /// 首次成功响应的业务引用。
    pub reference: String,
}

impl VoidDraftReceipt {
    /// 从已持久化的作废采购单构造稳定收据。
    ///
    /// # 参数
    /// * `order` - Repository 更新后带新版本的已作废采购单
    /// * `reason` - 首次请求中的作废原因
    ///
    /// # 返回
    /// 返回可持久化并稳定回放的作废结果载荷。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 状态、版本和业务引用必须与首次成功响应一致，独立回执保留规范化作废原因。
    pub fn from_voided(order: &PurchaseOrder, reason: &str) -> Self {
        Self {
            purchase_order_id: order.base.id.clone(),
            status: order.stable.status.as_str().to_string(),
            lock_version: order.base.version,
            reason: reason.trim().to_string(),
            reference: format!("VOID-V{}", order.base.version),
        }
    }

    /// 转换为采购草稿作废 API 结果。
    ///
    /// # 参数
    /// * `replayed` - 是否来自匹配命令收据的回放
    ///
    /// # 返回
    /// 返回首次执行或幂等回放结果。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只有读取并校验匹配收据后才能传入 `true`。
    pub fn into_result(self, replayed: bool) -> VoidPurchaseOrderResult {
        VoidPurchaseOrderResult {
            purchase_order_id: self.purchase_order_id,
            status: self.status,
            lock_version: self.lock_version,
            replayed,
            reference: self.reference,
        }
    }
}

/// 选源命令中单张已提交采购单的幂等收据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcingOrderReceipt {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 创建完成时乐观锁版本。
    pub lock_version: u64,
}

/// 选源命令幂等收据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcingReceipt {
    /// 本次创建并已提交审批的全部采购单。
    pub orders: Vec<SourcingOrderReceipt>,
    /// 本次建立的现有库存预占。
    pub stock_reservations: Vec<ExistingStockReservationResult>,
    /// 本次命令同步完成时的原任务状态。
    pub work_item_status: SourcingTaskStatus,
}

/// 采购提交命令的最小、可重放结果收据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseSubmitReceipt {
    /// 采购单号。
    pub purchase_no: String,
    /// 形成的不可变提交。
    pub submission_id: String,
    /// 提交序号。
    pub submission_no: String,
    /// 审核待办；事务内首个入口任务写入后回填，无任务时保持空。
    pub work_item_id: String,
    /// 审核待办乐观锁版本。
    pub task_version: u64,
    /// 待办锁定的不可变采购提交版本。
    pub subject_version: String,
    /// 采购单新乐观锁版本。
    pub lock_version: u64,
}

impl PurchaseSubmitReceipt {
    /// 构造采购提交命令收据。
    ///
    /// # 参数
    /// * `purchase_no` - 采购单号
    /// * `submission_id` - 形成的不可变提交
    /// * `submission_no` - 提交序号
    /// * `work_item_id` - 审核待办
    /// * `subject_version` - 待办锁定的不可变采购提交版本
    ///
    /// # 返回
    /// 返回任务版本为零的收据。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        purchase_no: String,
        submission_id: String,
        submission_no: String,
        work_item_id: String,
        subject_version: String,
    ) -> Self {
        Self {
            purchase_no,
            submission_id,
            submission_no,
            work_item_id,
            task_version: 0,
            subject_version,
            lock_version: 0,
        }
    }

    /// 设置审核待办乐观锁版本与采购单版本。
    ///
    /// # 参数
    /// * `task_version` - 审核待办乐观锁版本
    /// * `lock_version` - 采购单新乐观锁版本
    ///
    /// # 返回
    /// 返回更新后的收据。
    ///
    /// # 错误
    /// 无。
    pub fn with_versions(mut self, task_version: u64, lock_version: u64) -> Self {
        self.task_version = task_version;
        self.lock_version = lock_version;
        self
    }
    /// 回填首个入口任务身份，保证回放与首次响应一致。
    ///
    /// # 参数
    /// * `first_task` - 事务内写入的首个入口任务身份；无任务时为空
    ///
    /// # 返回
    /// 返回携带真实任务身份（或无任务时保持空占位）的收据。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只能在独立回执持久化前调用一次；同一命令首次响应与回放必须返回相同的任务身份。
    pub fn with_first_task(mut self, first_task: Option<&(String, u64)>) -> Self {
        if let Some((work_item_id, task_version)) = first_task {
            self.work_item_id = work_item_id.clone();
            self.task_version = *task_version;
        }
        self
    }
}
