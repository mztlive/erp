//! `purchase_receipt` / `purchase_receipt_line`：采购入库单及行（数据模型 §6.7）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。
//!
//! 状态机按 §7.5（库存入库 DRAFT → POSTED → REVERSED，含终态 REVERSED）；
//! `POSTED` 后不可编辑，纠错只能冲正或采购退货。Service 加载采购版本、付款和
//! 分配数据，本模块校验过账资格、超收上限、预占分摊与履约进度。
//! 公共字段按 §6.7 字典精确建模（`posted_at`/`posted_by`），组合 `BaseModel`。

use std::collections::HashMap;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    PurchaseOrderId, PurchaseOrderRevisionLineId, PurchaseReceiptId, PurchaseReceiptLineId, SalesOrderLineId,
    SalesOrderRevisionLineId, WarehouseId,
};
use crate::money::{round_to_cent, Amount, Quantity};
use crate::purchase_order::{
    PaymentTermSnapshot, ProgressStatus, PurchaseLineSalesAllocation, PurchaseOrderRevisionLine,
    PurchaseOrderStatus,
};
use crate::validation::normalize_required_text;

/// 入库单号最大长度。
const RECEIPT_NO_MAX_LEN: usize = 64;
/// 经办人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 采购来源履约的纯资格规则。
///
/// Service 负责加载采购单、当前修订、付款净额和分配两端记录；本类型只校验
/// 已完整加载数据中的状态、先款门槛与关联一致性，不访问数据库。
pub struct PurchaseFulfillmentEligibility;

impl PurchaseFulfillmentEligibility {
    /// 校验采购单状态允许形成收货、电子交付或服务履约事实。
    ///
    /// # 参数
    /// * `status` - 当前采购单状态
    ///
    /// # 返回
    /// 生效或部分执行时返回 `Ok(())`。
    ///
    /// # 错误
    /// 其它状态返回业务规则错误。
    pub fn ensure_order_fulfillable(status: PurchaseOrderStatus) -> Result<()> {
        if matches!(
            status,
            PurchaseOrderStatus::Effective | PurchaseOrderStatus::PartiallyExecuted
        ) {
            return Ok(());
        }
        Err(Error::from("采购单不在可履约状态，无法过账"))
    }

    /// 校验冻结的先款后货门槛。
    ///
    /// # 参数
    /// * `snapshot` - 当前生效采购版本的付款条件快照
    /// * `gross_amount` - 当前生效采购版本含税总额
    /// * `effective_paid` - 有效已过账付款净核销金额
    ///
    /// # 返回
    /// 未启用门槛或已达到全部门槛时返回 `Ok(())`。
    ///
    /// # 错误
    /// 有效付款低于冻结金额或比例门槛时返回业务规则错误。
    pub fn ensure_prepayment_satisfied(
        snapshot: &PaymentTermSnapshot,
        gross_amount: Amount,
        effective_paid: Amount,
    ) -> Result<()> {
        if !snapshot.prepay_gate {
            return Ok(());
        }
        if snapshot
            .prepay_minimum_amount
            .is_some_and(|minimum| effective_paid.to_decimal() < minimum.to_decimal())
        {
            return Err(Error::from(
                "该采购单为先款后货，有效付款未达金额门槛，请先完成付款",
            ));
        }
        if let Some(minimum_ratio) = snapshot.prepay_minimum_ratio {
            let required = round_to_cent(gross_amount.to_decimal() * minimum_ratio.to_decimal());
            if effective_paid.to_decimal() < required {
                return Err(Error::from(
                    "该采购单为先款后货，有效付款未达比例门槛，请先完成付款",
                ));
            }
        }
        Ok(())
    }

    /// 校验采购销售分配属于当前采购版本和目标销售稳定明细。
    ///
    /// # 参数
    /// * `allocation` - 已加载的采购销售分配
    /// * `current_purchase_line_ids` - 当前生效采购版本行主键集合
    /// * `sales_revision_line` - 已加载销售版本行的「版本行主键、稳定行主键」
    /// * `expected_sales_order_line_id` - 本次履约声明的销售稳定明细
    ///
    /// # 返回
    /// 两端关联一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 分配不属于当前采购版本，或销售版本行与声明明细不一致时返回错误。
    pub fn ensure_allocation_consistent(
        allocation: &PurchaseLineSalesAllocation,
        current_purchase_line_ids: &[PurchaseOrderRevisionLineId],
        sales_revision_line: Option<(&SalesOrderRevisionLineId, &SalesOrderLineId)>,
        expected_sales_order_line_id: &SalesOrderLineId,
    ) -> Result<()> {
        if !current_purchase_line_ids.contains(&allocation.purchase_order_revision_line_id) {
            return Err(Error::from("采购销售分配不属于当前生效版本"));
        }
        let Some((revision_line_id, sales_order_line_id)) = sales_revision_line else {
            return Err(Error::from("采购销售分配与销售明细不一致"));
        };
        if revision_line_id != &allocation.sales_order_revision_line_id
            || sales_order_line_id != expected_sales_order_line_id
        {
            return Err(Error::from("采购销售分配与销售明细不一致"));
        }
        Ok(())
    }
}

/// 采购入库单状态（数据模型 §6.7/§7.5：草稿、已过账、已冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseReceiptState {
    /// 草稿。
    Draft,
    /// 已过账（库存事实已形成，内容不可编辑）。
    Posted,
    /// 已冲正（存在正式反向事实，终态）。
    Reversed,
}

impl PurchaseReceiptState {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Posted => "已过账",
            Self::Reversed => "已冲正",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Posted => "POSTED",
            Self::Reversed => "REVERSED",
        }
    }

    /// 判断是否可编辑（仅草稿）。
    ///
    /// # 返回
    /// 草稿状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        matches!(self, Self::Draft)
    }
}

impl DocumentState for PurchaseReceiptState {
    /// 固定邻接矩阵（§7.5 定向链，`REVERSED` 为不可逆终态）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Posted],
            Self::Posted => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 质量结果（数据模型 §6.7：合格、不合格、部分合格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QualityResult {
    /// 全部合格。
    Passed,
    /// 全部不合格。
    Rejected,
    /// 部分合格。
    Partial,
}

impl QualityResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passed => "合格",
            Self::Rejected => "不合格",
            Self::Partial => "部分合格",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Rejected => "REJECTED",
            Self::Partial => "PARTIAL",
        }
    }
}

/// 采购入库单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReceiptData {
    /// 采购入库单号（全局唯一）。
    pub receipt_no: String,
    /// 来源采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 入库仓。
    pub warehouse_id: WarehouseId,
}

/// 采购入库单更新数据（仅草稿可更新）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReceiptUpdate {
    /// 入库仓；`None` 表示不修改。
    pub warehouse_id: Option<WarehouseId>,
}

/// 采购入库单实体（数据模型 §6.7 表头）。
///
/// `status` 按 §7.5 固定状态机迁移；`posted_at`/`posted_by` 由过账动作写入。
/// 草稿可编辑（可逻辑删除，§4.5.2）；已过账/已冲正不设业务软删除（§4.5.1）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 采购入库单号。
    pub receipt_no: String,
    /// 来源采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 入库仓。
    pub warehouse_id: WarehouseId,
    /// 当前状态。
    pub status: PurchaseReceiptState,
    /// 入库过账时间。
    pub posted_at: Option<Instant>,
    /// 仓储经办人。
    pub posted_by: Option<String>,
}

impl PurchaseReceipt {
    /// 创建采购入库单（初始状态为草稿）。
    ///
    /// 完成 receipt_no 的规范化（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseReceiptId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的入库单实体。
    ///
    /// # 错误
    /// 当 receipt_no 为空或超长时返回错误。
    pub fn new(id: PurchaseReceiptId, data: PurchaseReceiptData) -> Result<Self> {
        let receipt_no = normalize_required_text(
            data.receipt_no,
            "采购入库单号不能为空",
            RECEIPT_NO_MAX_LEN,
            "采购入库单号过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            receipt_no,
            purchase_order_id: data.purchase_order_id,
            warehouse_id: data.warehouse_id,
            status: PurchaseReceiptState::Draft,
            posted_at: None,
            posted_by: None,
        })
    }

    /// 更新采购入库单。
    ///
    /// 复用 `new` 的校验规则；已过账/已冲正的入库单不可编辑（§6.7），
    /// 只能冲正或走采购退货。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不可编辑时返回错误。
    pub fn update(&mut self, update: PurchaseReceiptUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(warehouse_id) = update.warehouse_id {
            self.warehouse_id = warehouse_id;
        }
        Ok(())
    }

    /// 校验入库单仍为调用方看到的草稿版本。
    ///
    /// # 参数
    /// * `expected_version` - 调用方提交的乐观锁版本
    ///
    /// # 返回
    /// 草稿且版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态或版本不满足时返回错误。
    pub fn ensure_draft_version(&self, expected_version: u64) -> Result<()> {
        if self.status != PurchaseReceiptState::Draft {
            return Err(Error::from("只有草稿状态的采购入库单可以过账"));
        }
        if self.base.version != expected_version {
            return Err(Error::from("采购入库单版本已变化，请刷新后重试"));
        }
        Ok(())
    }

    /// 校验已加载入库行完整且全部归属本单。
    ///
    /// # 参数
    /// * `lines` - 已在事务内加载的入库行
    ///
    /// # 返回
    /// 至少一行且全部行归属本单时返回 `Ok(())`。
    ///
    /// # 错误
    /// 空行或行表头关联不一致时返回错误。
    pub fn ensure_posting_lines(&self, lines: &[PurchaseReceiptLine]) -> Result<()> {
        if lines.is_empty() {
            return Err(Error::from("采购入库单没有行，无法过账"));
        }
        let receipt_id = PurchaseReceiptId::new(self.base.id.clone());
        if lines.iter().any(|line| line.purchase_receipt_id != receipt_id) {
            return Err(Error::from("采购入库行与入库单关联不一致"));
        }
        Ok(())
    }

    /// 根据当前生效采购版本与累计有效收货计算采购履约进度。
    ///
    /// # 参数
    /// * `revision_lines` - 当前生效采购版本行
    /// * `received` - 按采购版本行汇总的累计合格收货
    ///
    /// # 返回
    /// 全部有数量行均收满时返回 `Completed`，否则返回 `Partial`。
    pub fn fulfillment_progress(
        revision_lines: &[PurchaseOrderRevisionLine],
        received: &HashMap<String, Quantity>,
    ) -> ProgressStatus {
        let total = revision_lines
            .iter()
            .filter_map(|line| line.quantity)
            .fold(rust_decimal::Decimal::ZERO, |sum, quantity| {
                sum + quantity.to_decimal()
            });
        let received_total = revision_lines
            .iter()
            .filter_map(|line| received.get(&line.base.id))
            .fold(rust_decimal::Decimal::ZERO, |sum, quantity| {
                sum + quantity.to_decimal()
            });
        if total > rust_decimal::Decimal::ZERO && received_total >= total {
            ProgressStatus::Completed
        } else {
            ProgressStatus::Partial
        }
    }

    /// 过账入库（草稿 → 已过账）。
    ///
    /// 过账时记录入库过账时间与仓储经办人（§6.7）。库存入账、余额更新与
    /// 销售预占等跨聚合动作由 P3 在过账事务中完成（§8.2 第 1 条）。
    ///
    /// # 参数
    /// * `posted_at` - 入库过账时间
    /// * `posted_by` - 仓储经办人（账号或系统身份）
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非草稿），或经办人为空/超长时返回错误。
    pub fn mark_posted(&mut self, posted_at: Instant, posted_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.status, PurchaseReceiptState::Posted)?;
        let posted_by = normalize_required_text(
            posted_by.into(),
            "仓储经办人不能为空",
            ACTOR_MAX_LEN,
            "仓储经办人过长",
        )?;
        self.posted_at = Some(posted_at);
        self.posted_by = Some(posted_by);
        self.status = PurchaseReceiptState::Posted;
        Ok(())
    }

    /// 冲正入库单（已过账 → 已冲正，终态）。
    ///
    /// `REVERSED` 表示存在正式反向事实（冲正流水/采购退货），不删除原事实
    /// （§4.5.1、§7.5）；反向事实由 P3 形成。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非已过账）时返回错误。
    pub fn reverse(&mut self) -> Result<()> {
        ensure_transition(self.status, PurchaseReceiptState::Reversed)?;
        self.status = PurchaseReceiptState::Reversed;
        Ok(())
    }

    /// 判断当前状态是否可编辑。
    ///
    /// # 返回
    /// 草稿状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        self.status.is_editable()
    }

    /// 校验当前状态可编辑。
    ///
    /// # 返回
    /// 可编辑返回 `Ok(())`。
    ///
    /// # 错误
    /// 已过账或已冲正的入库单不可编辑时返回错误。
    fn ensure_editable(&self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("已过账或已冲正的采购入库单不可编辑"));
        }
        Ok(())
    }
}

/// 采购入库行创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReceiptLineData {
    /// 入库单。
    pub purchase_receipt_id: PurchaseReceiptId,
    /// 稳定行号（单内从 1 递增）。
    pub line_no: u32,
    /// 采购明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 到货数量。
    pub received_quantity: Quantity,
    /// 合格数量。
    pub qualified_quantity: Quantity,
    /// 不合格数量。
    pub rejected_quantity: Quantity,
    /// 质量结果。
    pub quality_result: QualityResult,
}

/// 采购入库行实体（数据模型 §6.7 行）。
///
/// 行的合格与不合格数量合计不得超过到货数量（§6.7）；仅合格数量形成库存入账
/// 和销售预占。Service 加载当前采购版本与累计收货后，由本实体校验超收上限并
/// 计算采购销售分配对应的预占份额。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseReceiptLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 入库单。
    pub purchase_receipt_id: PurchaseReceiptId,
    /// 稳定行号。
    pub line_no: u32,
    /// 采购明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 到货数量。
    pub received_quantity: Quantity,
    /// 合格数量。
    pub qualified_quantity: Quantity,
    /// 不合格数量。
    pub rejected_quantity: Quantity,
    /// 质量结果。
    pub quality_result: QualityResult,
}

impl PurchaseReceiptLine {
    /// 创建采购入库行。
    ///
    /// 完成数量约束校验：三个数量均非负、合格与不合格数量合计不超过到货数量、
    /// 行号从 1 开始。行级约束 `(purchase_receipt_id, line_no)` 唯一由唯一索引
    /// 保证；入库单已过账后行不可再变更由 P3 按表头状态把关。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseReceiptLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的入库行实体。
    ///
    /// # 错误
    /// 行号小于 1、数量为负或合格/不合格合计超过到货数量时返回错误。
    pub fn new(id: PurchaseReceiptLineId, data: PurchaseReceiptLineData) -> Result<Self> {
        ensure_line_valid(&data)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_receipt_id: data.purchase_receipt_id,
            line_no: data.line_no,
            purchase_order_revision_line_id: data.purchase_order_revision_line_id,
            received_quantity: data.received_quantity,
            qualified_quantity: data.qualified_quantity,
            rejected_quantity: data.rejected_quantity,
            quality_result: data.quality_result,
        })
    }

    /// 返回本行计入累计有效收货上限的数量。
    ///
    /// # 返回
    /// 返回合格数量与不合格数量之和。
    ///
    /// # 错误
    /// 数量相加超出统一精度范围时返回错误。
    pub fn posting_quantity(&self) -> Result<Quantity> {
        Quantity::try_from(self.qualified_quantity.to_decimal() + self.rejected_quantity.to_decimal())
            .map_err(|error| Error::from(error.to_string()))
    }

    /// 校验本行属于当前采购版本且累计收货不超采购数量。
    ///
    /// # 参数
    /// * `revision_line` - 按本行采购版本行主键加载的当前生效版本行
    /// * `already_received` - 过账前该采购版本行的累计合格收货
    ///
    /// # 返回
    /// 关联一致、非物流费用行且累计不超收时返回 `Ok(())`。
    ///
    /// # 错误
    /// 采购版本行关联不一致、没有可收货数量或累计超收时返回错误。
    pub fn ensure_within_revision(
        &self,
        revision_line: &PurchaseOrderRevisionLine,
        already_received: Quantity,
    ) -> Result<()> {
        if revision_line.base.id != self.purchase_order_revision_line_id.to_string() {
            return Err(Error::from("采购入库行与采购版本明细关联不一致"));
        }
        let available = revision_line
            .quantity
            .ok_or_else(|| Error::from("物流费用行不能入库"))?;
        if already_received.to_decimal() + self.posting_quantity()?.to_decimal() > available.to_decimal() {
            return Err(Error::from(
                "累计有效收货超过当前有效采购数量，超收必须走明确审批和采购变更",
            ));
        }
        Ok(())
    }

    /// 按采购销售分配比例分摊本次合格入库数量。
    ///
    /// 最后一个分配吸收六位数量精度的舍入尾差。
    ///
    /// # 参数
    /// * `allocation_quantities` - 各采购销售分配数量
    /// * `purchase_line_total` - 当前采购版本行总数量
    ///
    /// # 返回
    /// 返回与分配集合一一对应、合计等于本行合格数量的预占份额。
    ///
    /// # 错误
    /// 采购行数量非正或份额超出统一数量精度时返回错误。
    pub fn reservation_shares(
        &self,
        allocation_quantities: &[Quantity],
        purchase_line_total: Quantity,
    ) -> Result<Vec<Quantity>> {
        let total = purchase_line_total.to_decimal();
        if total <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("采购明细数量必须为正数"));
        }
        let qualified = self.qualified_quantity.to_decimal();
        let mut assigned = rust_decimal::Decimal::ZERO;
        let mut shares = Vec::with_capacity(allocation_quantities.len());
        for (index, allocation_quantity) in allocation_quantities.iter().enumerate() {
            let share = if index + 1 == allocation_quantities.len() {
                qualified - assigned
            } else {
                (qualified * allocation_quantity.to_decimal() / total).round_dp(6)
            };
            assigned += share;
            shares.push(Quantity::try_from(share).map_err(|error| Error::from(error.to_string()))?);
        }
        Ok(shares)
    }
}

/// 校验入库行数量约束。
///
/// # 参数
/// * `data` - 行创建/更新数据
///
/// # 返回
/// 通过返回 `Ok(())`。
///
/// # 错误
/// 行号小于 1、数量为负或合格/不合格合计超过到货数量时返回错误。
fn ensure_line_valid(data: &PurchaseReceiptLineData) -> Result<()> {
    if data.line_no < 1 {
        return Err(Error::from("行号必须从 1 开始"));
    }
    if data.received_quantity.to_decimal() < rust_decimal::Decimal::ZERO
        || data.qualified_quantity.to_decimal() < rust_decimal::Decimal::ZERO
        || data.rejected_quantity.to_decimal() < rust_decimal::Decimal::ZERO
    {
        return Err(Error::from("入库行数量不得为负"));
    }
    if data.qualified_quantity.to_decimal() + data.rejected_quantity.to_decimal()
        > data.received_quantity.to_decimal()
    {
        return Err(Error::from("合格与不合格数量合计不得超过到货数量"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PurchaseReceiptId;
    use std::str::FromStr;

    fn line_data() -> PurchaseReceiptLineData {
        PurchaseReceiptLineData {
            purchase_receipt_id: PurchaseReceiptId::new("receipt-1"),
            line_no: 1,
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("po-line-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            quality_result: QualityResult::Partial,
        }
    }

    fn receipt_data() -> PurchaseReceiptData {
        PurchaseReceiptData {
            receipt_no: " PR-2026-001 ".to_string(),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            warehouse_id: WarehouseId::new("wh-1"),
        }
    }

    /// 构造具备数量和销售关联的最小采购版本行测试夹具。
    ///
    /// # 参数
    /// * `quantity` - 采购版本行数量
    ///
    /// # 返回
    /// 返回用于收货上限、进度与关联规则测试的采购版本行。
    fn revision_line(quantity: Quantity) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new("po-line-1"),
            crate::purchase_order::PurchaseOrderRevisionLineData {
                purchase_order_revision_id: crate::ids::PurchaseOrderRevisionId::new("po-rev-1"),
                line_no: 1,
                line_type: crate::purchase_order::PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(crate::ids::ProcurementConfirmationLineId::new(
                    "confirmation-line-1",
                )),
                sku_id: Some(crate::ids::SkuId::new("sku-1")),
                sku_revision_id: None,
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("默认规格".to_string()),
                quantity: Some(quantity),
                base_unit_code: Some("PCS".to_string()),
                unit_cost_gross: Some(crate::money::UnitPrice::from_str("10.0000").unwrap()),
                gross_amount: Amount::from_str("100.00").unwrap(),
                net_amount: Amount::from_str("87.00").unwrap(),
                tax_amount: Amount::from_str("13.00").unwrap(),
                input_tax_rate: Some(crate::money::Rate::from_str("0.130000").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new("sales-line-1")),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sales-revision-line-1")),
                allocated_quantity: Some(quantity),
            },
        )
        .unwrap()
    }

    /// happy path：单号规范化、初始草稿、过账与冲正全链路。
    #[test]
    fn new_normalizes_no_and_drives_state_machine() {
        let mut receipt = PurchaseReceipt::new(PurchaseReceiptId::new("receipt-1"), receipt_data()).unwrap();
        assert_eq!(receipt.receipt_no, "PR-2026-001");
        assert_eq!(receipt.status, PurchaseReceiptState::Draft);
        assert!(receipt.is_editable());

        receipt
            .mark_posted(Instant::from_unix_secs(1_700_000_000), " operator-1 ")
            .unwrap();
        assert_eq!(receipt.status, PurchaseReceiptState::Posted);
        assert_eq!(receipt.posted_by.as_deref(), Some("operator-1"));
        assert_eq!(receipt.posted_at.unwrap().unix_secs(), 1_700_000_000);

        receipt.reverse().unwrap();
        assert_eq!(receipt.status, PurchaseReceiptState::Reversed);
    }

    /// 失败路径：必填空（单号空白）与超长。
    #[test]
    fn new_rejects_blank_or_overlong_no() {
        let blank = PurchaseReceiptData {
            receipt_no: "   ".to_string(),
            ..receipt_data()
        };
        assert!(PurchaseReceipt::new(PurchaseReceiptId::new("r2"), blank).is_err());

        let overlong = PurchaseReceiptData {
            receipt_no: "x".repeat(65),
            ..receipt_data()
        };
        assert!(PurchaseReceipt::new(PurchaseReceiptId::new("r3"), overlong).is_err());
    }

    /// 状态机：合法/非法/终态定向断言（含不可逆终态 REVERSED，逐边定向）。
    #[test]
    fn state_machine_directed_edges() {
        let mut receipt = PurchaseReceipt::new(PurchaseReceiptId::new("receipt-2"), receipt_data()).unwrap();
        assert!(receipt.reverse().is_err(), "草稿不能直接冲正");
        assert!(receipt
            .update(PurchaseReceiptUpdate {
                warehouse_id: Some(WarehouseId::new("wh-2")),
            })
            .is_ok());
        receipt
            .mark_posted(Instant::from_unix_secs(1_700_000_000), "operator-1")
            .unwrap();
        assert!(
            receipt
                .update(PurchaseReceiptUpdate { warehouse_id: None })
                .is_err(),
            "已过账不可编辑"
        );
        // from == to 幂等迁移恒合法（state.rs 契约）；POSTED 不可编辑由 update 把关。
        assert!(receipt
            .mark_posted(Instant::from_unix_secs(1_700_000_100), "o2")
            .is_ok());
        assert!(receipt.reverse().is_ok());
        assert!(
            receipt.reverse().is_ok(),
            "REVERSED 幂等迁移合法，且无法迁移到其他状态"
        );
    }

    /// 状态机：固定邻接矩阵的合法/非法迁移（幂等合法）。
    #[test]
    fn state_machine_transition_matrix() {
        assert!(ensure_transition(PurchaseReceiptState::Draft, PurchaseReceiptState::Draft).is_ok());
        assert!(ensure_transition(PurchaseReceiptState::Draft, PurchaseReceiptState::Posted).is_ok());
        assert!(ensure_transition(PurchaseReceiptState::Posted, PurchaseReceiptState::Reversed).is_ok());
        assert!(ensure_transition(PurchaseReceiptState::Draft, PurchaseReceiptState::Reversed).is_err());
        assert!(ensure_transition(PurchaseReceiptState::Posted, PurchaseReceiptState::Draft).is_err());
        assert!(ensure_transition(PurchaseReceiptState::Reversed, PurchaseReceiptState::Posted).is_err());
        assert!(ensure_transition(PurchaseReceiptState::Reversed, PurchaseReceiptState::Reversed).is_ok());
    }

    /// happy path：行创建成功，字段完整。
    #[test]
    fn line_new_succeeds() {
        let line = PurchaseReceiptLine::new(PurchaseReceiptLineId::new("line-1"), line_data()).unwrap();
        assert_eq!(line.line_no, 1);
        assert_eq!(line.qualified_quantity, Quantity::from_str("9").unwrap());
        assert_eq!(line.rejected_quantity, Quantity::from_str("1").unwrap());
        assert_eq!(line.quality_result, QualityResult::Partial);
    }

    /// 失败路径：数量越界（合计超过到货）与负数量。
    #[test]
    fn line_rejects_quantity_violations() {
        let over_sum = PurchaseReceiptLineData {
            qualified_quantity: Quantity::from_str("9.5").unwrap(),
            ..line_data()
        };
        assert!(PurchaseReceiptLine::new(PurchaseReceiptLineId::new("l2"), over_sum).is_err());

        let negative = PurchaseReceiptLineData {
            rejected_quantity: Quantity::from_str("-0.5").unwrap(),
            ..line_data()
        };
        assert!(PurchaseReceiptLine::new(PurchaseReceiptLineId::new("l3"), negative).is_err());

        let zero_line_no = PurchaseReceiptLineData {
            line_no: 0,
            ..line_data()
        };
        assert!(PurchaseReceiptLine::new(PurchaseReceiptLineId::new("l4"), zero_line_no).is_err());
    }

    /// 过账资格校验状态、版本、表头行归属、超收和预占分摊守恒。
    #[test]
    fn posting_rules_are_entity_owned() {
        let mut receipt = PurchaseReceipt::new(PurchaseReceiptId::new("receipt-1"), receipt_data()).unwrap();
        let line = PurchaseReceiptLine::new(PurchaseReceiptLineId::new("line-rule"), line_data()).unwrap();
        assert!(receipt.ensure_draft_version(receipt.base.version).is_ok());
        assert!(receipt.ensure_draft_version(receipt.base.version + 1).is_err());
        assert!(receipt.ensure_posting_lines(&[]).is_err());
        assert!(receipt.ensure_posting_lines(std::slice::from_ref(&line)).is_ok());
        let foreign_line = PurchaseReceiptLine::new(
            PurchaseReceiptLineId::new("foreign-line"),
            PurchaseReceiptLineData {
                purchase_receipt_id: PurchaseReceiptId::new("other-receipt"),
                ..line_data()
            },
        )
        .unwrap();
        assert!(receipt
            .ensure_posting_lines(std::slice::from_ref(&foreign_line))
            .is_err());

        let revision_line = revision_line(Quantity::from_str("10").unwrap());
        assert!(line
            .ensure_within_revision(&revision_line, Quantity::from_str("0").unwrap(),)
            .is_ok());
        assert!(line
            .ensure_within_revision(&revision_line, Quantity::from_str("1").unwrap(),)
            .is_err());
        let mut foreign_revision_line = revision_line.clone();
        foreign_revision_line.base.id = "other-po-line".to_string();
        assert!(line
            .ensure_within_revision(&foreign_revision_line, Quantity::from_str("0").unwrap())
            .is_err());

        let shares = line
            .reservation_shares(
                &[
                    Quantity::from_str("3").unwrap(),
                    Quantity::from_str("2").unwrap(),
                    Quantity::from_str("5").unwrap(),
                ],
                Quantity::from_str("10").unwrap(),
            )
            .unwrap();
        assert_eq!(shares[0], Quantity::from_str("2.7").unwrap());
        assert_eq!(shares[1], Quantity::from_str("1.8").unwrap());
        assert_eq!(shares[2], Quantity::from_str("4.5").unwrap());
        assert!(line
            .reservation_shares(
                &[Quantity::from_str("1").unwrap()],
                Quantity::from_str("0").unwrap()
            )
            .is_err());

        let mut received = HashMap::new();
        received.insert("po-line-1".to_string(), Quantity::from_str("10").unwrap());
        assert_eq!(
            PurchaseReceipt::fulfillment_progress(std::slice::from_ref(&revision_line), &received),
            ProgressStatus::Completed,
        );
        assert_eq!(
            PurchaseReceipt::fulfillment_progress(std::slice::from_ref(&revision_line), &HashMap::new()),
            ProgressStatus::Partial,
        );
        receipt
            .mark_posted(Instant::from_unix_secs(1_700_000_000), "operator-1")
            .unwrap();
        assert!(receipt.ensure_draft_version(receipt.base.version).is_err());
    }

    /// 采购来源履约资格校验采购状态、先款门槛与分配两端关联。
    #[test]
    fn purchase_fulfillment_eligibility_checks_loaded_context() {
        assert!(
            PurchaseFulfillmentEligibility::ensure_order_fulfillable(PurchaseOrderStatus::Effective,).is_ok()
        );
        assert!(
            PurchaseFulfillmentEligibility::ensure_order_fulfillable(PurchaseOrderStatus::Draft,).is_err()
        );

        let snapshot = PaymentTermSnapshot::new(
            "PREPAY_50".to_string(),
            true,
            Some(Amount::from_str("50.00").unwrap()),
            Some(crate::money::Rate::from_str("0.500000").unwrap()),
        )
        .unwrap();
        assert!(PurchaseFulfillmentEligibility::ensure_prepayment_satisfied(
            &snapshot,
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("50.00").unwrap(),
        )
        .is_ok());
        assert!(PurchaseFulfillmentEligibility::ensure_prepayment_satisfied(
            &snapshot,
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("49.99").unwrap(),
        )
        .is_err());
        let ratio_only = PaymentTermSnapshot::new(
            "PREPAY_100".to_string(),
            true,
            None,
            Some(crate::money::Rate::from_str("0.750000").unwrap()),
        )
        .unwrap();
        assert!(PurchaseFulfillmentEligibility::ensure_prepayment_satisfied(
            &ratio_only,
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("74.99").unwrap(),
        )
        .is_err());
        let gate_disabled = PaymentTermSnapshot::new(
            "NET-30".to_string(),
            false,
            Some(Amount::from_str("100.00").unwrap()),
            None,
        )
        .unwrap();
        assert!(PurchaseFulfillmentEligibility::ensure_prepayment_satisfied(
            &gate_disabled,
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("0.00").unwrap(),
        )
        .is_ok());

        let allocation = PurchaseLineSalesAllocation::new(
            crate::ids::PurchaseLineSalesAllocationId::new("allocation-1"),
            crate::purchase_order::PurchaseLineSalesAllocationData {
                purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("po-line-1"),
                sales_order_revision_line_id: SalesOrderRevisionLineId::new("sales-revision-line-1"),
                allocated_quantity: Quantity::from_str("10").unwrap(),
                allocated_cost_gross: Amount::from_str("100.00").unwrap(),
                allocated_cost_net: Amount::from_str("87.00").unwrap(),
            },
        )
        .unwrap();
        let purchase_line_ids = [PurchaseOrderRevisionLineId::new("po-line-1")];
        let sales_revision_line_id = SalesOrderRevisionLineId::new("sales-revision-line-1");
        let sales_order_line_id = SalesOrderLineId::new("sales-line-1");
        assert!(PurchaseFulfillmentEligibility::ensure_allocation_consistent(
            &allocation,
            &purchase_line_ids,
            Some((&sales_revision_line_id, &sales_order_line_id)),
            &sales_order_line_id,
        )
        .is_ok());
        assert!(PurchaseFulfillmentEligibility::ensure_allocation_consistent(
            &allocation,
            &[],
            Some((&sales_revision_line_id, &sales_order_line_id)),
            &sales_order_line_id,
        )
        .is_err());
        let other_sales_order_line_id = SalesOrderLineId::new("other-sales-line");
        assert!(PurchaseFulfillmentEligibility::ensure_allocation_consistent(
            &allocation,
            &purchase_line_ids,
            Some((&sales_revision_line_id, &other_sales_order_line_id)),
            &sales_order_line_id,
        )
        .is_err());
        assert!(PurchaseFulfillmentEligibility::ensure_allocation_consistent(
            &allocation,
            &purchase_line_ids,
            None,
            &sales_order_line_id,
        )
        .is_err());
    }

    /// 序列化：状态/质量结果枚举输出稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&PurchaseReceiptState::Posted).unwrap(),
            "\"POSTED\""
        );
        assert_eq!(
            serde_json::to_string(&QualityResult::Partial).unwrap(),
            "\"PARTIAL\""
        );
        assert_eq!(PurchaseReceiptState::Reversed.label(), "已冲正");

        let mut receipt = PurchaseReceipt::new(PurchaseReceiptId::new("receipt-3"), receipt_data()).unwrap();
        receipt
            .mark_posted(Instant::from_unix_secs(1_700_000_000), "operator-1")
            .unwrap();
        let roundtrip: PurchaseReceipt =
            bson::deserialize_from_document(bson::serialize_to_document(&receipt).unwrap()).unwrap();
        assert_eq!(roundtrip, receipt);
    }

    /// 采购收货单无审批约束：不得出现绑定字段或审批状态机。
    #[test]
    fn purchase_receipt_has_no_approval_binding_or_state_machine() {
        let receipt = PurchaseReceipt::new(PurchaseReceiptId::new("receipt-1"), receipt_data()).unwrap();
        let value = serde_json::to_value(&receipt).unwrap();
        let object = value.as_object().expect("入库单序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("pending_allocations"));
        assert_eq!(receipt.status, PurchaseReceiptState::Draft);
        assert_eq!(PurchaseReceiptState::Draft.as_str(), "DRAFT");
        assert_eq!(PurchaseReceiptState::Posted.as_str(), "POSTED");
        assert_eq!(PurchaseReceiptState::Reversed.as_str(), "REVERSED");

        let production = include_str!("purchase_receipt.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("approval_subject_version"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("PENDING_REVIEW"));
    }
}
