//! `purchase_receipt` / `purchase_receipt_line`：采购入库单及行（数据模型 §6.7）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。
//!
//! 状态机按 §7.5（库存入库 DRAFT → POSTED → REVERSED，含终态 REVERSED）；
//! `POSTED` 后不可编辑，纠错只能冲正或采购退货（§6.7，跨聚合部分标注 P3）。
//! 公共字段按 §6.7 字典精确建模（`posted_at`/`posted_by`），组合 `BaseModel`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    PurchaseOrderId, PurchaseOrderRevisionLineId, PurchaseReceiptId, PurchaseReceiptLineId, WarehouseId,
};
use crate::money::Quantity;
use crate::validation::normalize_required_text;

/// 入库单号最大长度。
const RECEIPT_NO_MAX_LEN: usize = 64;
/// 经办人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

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
/// 和销售预占，累计有效收货不得超过当前有效采购数量——跨聚合校验由 P3 完成。
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
