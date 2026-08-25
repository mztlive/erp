//! `stock_adjustment` / `stock_adjustment_line`：库存调整单及明细（数据模型 §6.7）。
//!
//! 状态机按 §6.7/§7.5：草稿 → 待仓储复核 → 待财务确认 → 已过账 → 已冲正，
//! 仓储复核/财务确认可驳回（`REJECTED` 修改后重新提交复核）；`POSTED` 后
//! 不可编辑。经办人与仓储复核人不得相同（§6.7）；盘盈、盘亏和损坏一律经过
//! 财务成本影响确认后才能过账（§6.7，路径由状态机保证）。过账在同一事务写
//! 库存流水、余额和必要预占释放，原出入库流水不改写——由 P3 完成（§8.2
//! 第 3 条）。

use std::collections::HashSet;
use std::str::FromStr;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SkuId, StockAdjustmentId, StockAdjustmentLineId, WarehouseId};
use crate::money::Quantity;
use crate::validation::normalize_required_text;

use super::stock_movement::{MovementDirection, MovementType};

/// 调整单号最大长度。
const ADJUSTMENT_NO_MAX_LEN: usize = 64;
/// 经办人/复核人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;
/// 原因说明最大长度。
const NOTE_MAX_LEN: usize = 512;
/// 调整明细行主键最大长度。
const LINE_ID_MAX_LEN: usize = 128;

/// 库存调整单状态（数据模型 §6.7：草稿、待仓储复核、待财务确认、已过账、驳回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StockAdjustmentState {
    /// 草稿。
    Draft,
    /// 待仓储复核。
    PendingWarehouseReview,
    /// 待财务确认（成本影响确认）。
    PendingFinanceReview,
    /// 已过账（不可编辑）。
    Posted,
    /// 驳回（修改后重新提交复核）。
    Rejected,
    /// 已冲正（不可逆终态）。
    Reversed,
    /// 审批中。
    InApproval,
}

impl StockAdjustmentState {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingWarehouseReview => "待仓储复核",
            Self::PendingFinanceReview => "待财务确认",
            Self::Posted => "已过账",
            Self::Rejected => "驳回",
            Self::Reversed => "已冲正",
            Self::InApproval => "审批中",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::PendingWarehouseReview => "PENDING_WAREHOUSE_REVIEW",
            Self::PendingFinanceReview => "PENDING_FINANCE_REVIEW",
            Self::Posted => "POSTED",
            Self::Rejected => "REJECTED",
            Self::Reversed => "REVERSED",
            Self::InApproval => "IN_APPROVAL",
        }
    }

    /// 判断是否可编辑（草稿与驳回）。
    ///
    /// # 返回
    /// 草稿或驳回状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        matches!(self, Self::Draft | Self::Rejected)
    }

    /// 返回尚未过账且会影响库存详情展示的状态集合。
    ///
    /// # 返回
    /// 返回草稿与审批中状态。
    pub fn pending_posting() -> &'static [Self] {
        &[Self::Draft, Self::InApproval]
    }
}

impl DocumentState for StockAdjustmentState {
    /// 固定邻接矩阵（§6.7/§7.5 定向链；`REVERSED` 为不可逆终态）。
    ///
    /// `REJECTED` 可在修改后重新提交仓储复核（§6.5.5 再次提交审核）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingWarehouseReview, Self::InApproval],
            Self::InApproval => &[Self::Posted, Self::Draft],
            Self::PendingWarehouseReview => &[Self::PendingFinanceReview, Self::Rejected],
            Self::PendingFinanceReview => &[Self::Posted, Self::Rejected],
            Self::Rejected => &[Self::PendingWarehouseReview],
            Self::Posted => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 调整原因类型（数据模型 §6.7：盘盈、盘亏、损坏）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdjustmentReasonType {
    /// 盘盈。
    StockGain,
    /// 盘亏。
    StockLoss,
    /// 损坏。
    Damage,
}

impl AdjustmentReasonType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::StockGain => "盘盈",
            Self::StockLoss => "盘亏",
            Self::Damage => "损坏",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StockGain => "STOCK_GAIN",
            Self::StockLoss => "STOCK_LOSS",
            Self::Damage => "DAMAGE",
        }
    }

    /// 返回该调整原因要求的库存方向。
    ///
    /// # 返回
    /// 盘盈返回增加，盘亏与损坏返回减少。
    pub fn movement_direction(self) -> MovementDirection {
        match self {
            Self::StockGain => MovementDirection::Increase,
            Self::StockLoss | Self::Damage => MovementDirection::Decrease,
        }
    }

    /// 返回该调整原因对应的正式流水类型。
    ///
    /// # 返回
    /// 返回盘盈、盘亏或损坏流水类型。
    pub fn movement_type(self) -> MovementType {
        match self {
            Self::StockGain => MovementType::StockGain,
            Self::StockLoss => MovementType::StockLoss,
            Self::Damage => MovementType::Damage,
        }
    }

    /// 校验调整明细方向与原因一致。
    ///
    /// # 参数
    /// * `direction` - 待校验的调整方向
    ///
    /// # 返回
    /// 方向一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 盘盈不是增加，或盘亏/损坏不是减少时返回错误。
    pub fn ensure_direction(self, direction: MovementDirection) -> Result<()> {
        let expected = self.movement_direction();
        if direction != expected {
            return Err(Error::from(format!(
                "调整原因 {} 的明细方向必须为 {}",
                self.label(),
                expected.as_str()
            )));
        }
        Ok(())
    }
}

/// 库存调整单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentData {
    /// 调整单号（全局唯一）。
    pub adjustment_no: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// 调整原因类型。
    pub reason_type: AdjustmentReasonType,
    /// 仓储经办人。
    pub prepared_by: String,
    /// 原因说明（可空）。
    pub note: Option<String>,
    /// 业务发生时间（可空；过账时缺省回退过账时刻）。
    pub occurred_at: Option<Instant>,
}

/// 库存调整单更新数据（仅草稿/驳回可更新）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentUpdate {
    /// 调整原因类型；`None` 表示不修改。
    pub reason_type: Option<AdjustmentReasonType>,
    /// 仓储复核人；`None` 表示不修改。
    pub reviewed_by: Option<String>,
    /// 成本影响确认人；`None` 表示不修改。
    pub finance_reviewed_by: Option<String>,
    /// 原因说明；`None` 表示不修改，空串清除。
    pub note: Option<String>,
    /// 业务发生时间；`None` 表示不修改。
    pub occurred_at: Option<Instant>,
}

/// 库存调整单实体（数据模型 §6.7）。
///
/// 经办人与仓储复核人不得相同（§6.7）；财务成本影响确认在
/// [`StockAdjustment::submit_for_finance_review`] 时登记，只有经过确认的
/// 调整单才能过账（§6.7）。已过账/已冲正不设业务软删除（§4.5.1）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct StockAdjustment {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 调整单号。
    pub adjustment_no: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// 调整原因类型。
    pub reason_type: AdjustmentReasonType,
    /// 当前状态。
    pub status: StockAdjustmentState,
    /// 仓储经办人。
    pub prepared_by: String,
    /// 仓储复核人。
    pub reviewed_by: Option<String>,
    /// 成本影响确认人。
    pub finance_reviewed_by: Option<String>,
    /// 原因说明（可空）。
    pub note: Option<String>,
    /// 业务发生时间（可空；过账时缺省回退过账时刻）。
    pub occurred_at: Option<Instant>,
    /// 审批提交版本，初值 0。
    #[serde(default)]
    pub approval_subject_version: u32,
}

impl StockAdjustment {
    /// 创建库存调整单（初始状态为草稿）。
    ///
    /// 完成调整单号与经办人规范化。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::StockAdjustmentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的调整单实体。
    ///
    /// # 错误
    /// 调整单号或经办人为空/超长时返回错误。
    pub fn new(id: StockAdjustmentId, data: StockAdjustmentData) -> Result<Self> {
        let adjustment_no = normalize_required_text(
            data.adjustment_no,
            "调整单号不能为空",
            ADJUSTMENT_NO_MAX_LEN,
            "调整单号过长",
        )?;
        let prepared_by = normalize_required_text(
            data.prepared_by,
            "仓储经办人不能为空",
            ACTOR_MAX_LEN,
            "仓储经办人过长",
        )?;
        let note = match data.note {
            Some(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    None
                } else if text.len() > NOTE_MAX_LEN {
                    return Err(Error::from("原因说明过长"));
                } else {
                    Some(text)
                }
            }
            None => None,
        };
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            adjustment_no,
            warehouse_id: data.warehouse_id,
            reason_type: data.reason_type,
            status: StockAdjustmentState::Draft,
            prepared_by,
            reviewed_by: None,
            finance_reviewed_by: None,
            note,
            occurred_at: data.occurred_at,
            approval_subject_version: 0,
        })
    }

    /// 更新库存调整单（仅草稿/驳回）。
    ///
    /// 复用 `new` 的文本规范化；经办人与仓储复核人不得相同（§6.7）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不可编辑，复核人与经办人相同，或文本规范化失败时返回错误。
    pub fn update(&mut self, update: StockAdjustmentUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(reason_type) = update.reason_type {
            self.reason_type = reason_type;
        }
        if let Some(reviewed_by) = update.reviewed_by {
            let reviewed_by =
                normalize_required_text(reviewed_by, "仓储复核人不能为空", ACTOR_MAX_LEN, "仓储复核人过长")?;
            ensure_reviewer_separation(&self.prepared_by, &reviewed_by)?;
            self.reviewed_by = Some(reviewed_by);
        }
        if let Some(finance_reviewed_by) = update.finance_reviewed_by {
            self.finance_reviewed_by = Some(normalize_required_text(
                finance_reviewed_by,
                "成本影响确认人不能为空",
                ACTOR_MAX_LEN,
                "成本影响确认人过长",
            )?);
        }
        if let Some(note) = update.note {
            let note = note.trim().to_string();
            if note.len() > NOTE_MAX_LEN {
                return Err(Error::from("原因说明过长"));
            }
            self.note = if note.is_empty() { None } else { Some(note) };
        }
        if let Some(occurred_at) = update.occurred_at {
            self.occurred_at = Some(occurred_at);
        }
        Ok(())
    }

    /// 提交仓储复核（草稿/驳回 → 待仓储复核）。
    ///
    /// 驳回的调整单在修改后重新提交复核（§6.5.5）；提交时登记仓储复核人，
    /// 并校验与经办人不同（§6.7 岗位分离）。
    ///
    /// # 参数
    /// * `reviewed_by` - 仓储复核人（账号或系统身份）
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许提交，复核人与经办人相同或为空/超长时返回错误。
    pub fn submit_for_warehouse_review(&mut self, reviewed_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.status, StockAdjustmentState::PendingWarehouseReview)?;
        let reviewed_by = normalize_required_text(
            reviewed_by.into(),
            "仓储复核人不能为空",
            ACTOR_MAX_LEN,
            "仓储复核人过长",
        )?;
        ensure_reviewer_separation(&self.prepared_by, &reviewed_by)?;
        self.reviewed_by = Some(reviewed_by);
        self.status = StockAdjustmentState::PendingWarehouseReview;
        Ok(())
    }

    /// 仓储复核通过，提交财务确认（待仓储复核 → 待财务确认）。
    ///
    /// # 参数
    /// * `finance_reviewed_by` - 成本影响确认人（账号或系统身份）
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移，或确认人为空/超长时返回错误。
    pub fn submit_for_finance_review(&mut self, finance_reviewed_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.status, StockAdjustmentState::PendingFinanceReview)?;
        let finance_reviewed_by = normalize_required_text(
            finance_reviewed_by.into(),
            "成本影响确认人不能为空",
            ACTOR_MAX_LEN,
            "成本影响确认人过长",
        )?;
        self.finance_reviewed_by = Some(finance_reviewed_by);
        self.status = StockAdjustmentState::PendingFinanceReview;
        Ok(())
    }

    /// 过账库存调整（待财务确认 → 已过账）。
    ///
    /// 盘盈、盘亏和损坏一律经过财务成本影响确认后才能过账（§6.7，允许结论
    /// 为零成本影响但不得跳过财务确认）；过账时写库存流水、余额和必要预占
    /// 释放由 P3 在同一事务完成（§8.2 第 3 条），原出入库流水不改写。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（未经财务确认）时返回错误。
    pub fn mark_posted(&mut self) -> Result<()> {
        ensure_transition(self.status, StockAdjustmentState::Posted)?;
        self.status = StockAdjustmentState::Posted;
        Ok(())
    }

    /// 驳回调整单（待仓储复核/待财务确认 → 驳回）。
    ///
    /// 驳回后修改调整单并重新提交仓储复核（§6.5.5 流程）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许驳回时返回错误。
    pub fn reject(&mut self) -> Result<()> {
        ensure_transition(self.status, StockAdjustmentState::Rejected)?;
        self.status = StockAdjustmentState::Rejected;
        Ok(())
    }

    /// 冲正库存调整（已过账 → 已冲正，终态）。
    ///
    /// `REVERSED` 表示存在正式反向事实（冲正流水），不删除原调整单
    /// （§4.5.1、§7.5）；反向事实由 P3 形成。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非已过账）时返回错误。
    pub fn reverse(&mut self) -> Result<()> {
        ensure_transition(self.status, StockAdjustmentState::Reversed)?;
        self.status = StockAdjustmentState::Reversed;
        Ok(())
    }

    /// 判断当前状态是否可编辑。
    ///
    /// # 返回
    /// 草稿或驳回状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        self.status.is_editable()
    }

    /// 判断当前乐观锁版本是否与期望版本一致。
    ///
    /// # 参数
    /// * `expected` - 调用方读取后携带的期望版本
    ///
    /// # 返回
    /// 当前版本等于期望版本时返回 `true`。
    pub fn matches_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 应用调整明细的最终值并校验整组规则。
    ///
    /// 校验行归属、重复行、数量正数、原因与方向一致性；提交审批时还可要求
    /// 命令完整覆盖全部持久化明细。校验失败时不会留下部分更新。
    ///
    /// # 参数
    /// * `lines` - 当前持久化明细的内存副本
    /// * `updates` - 待应用的明细更新
    /// * `require_all` - 是否要求覆盖全部明细
    ///
    /// # 返回
    /// 返回完成更新的明细副本，供调用方持久化。
    ///
    /// # 错误
    /// 行不属于本调整单、行重复、数量非法、方向不一致或未完整覆盖时返回错误。
    pub fn apply_line_updates(
        &self,
        lines: &mut [StockAdjustmentLine],
        updates: &[StockAdjustmentLineUpdate],
        require_all: bool,
    ) -> Result<Vec<StockAdjustmentLine>> {
        let mut staged = lines.to_vec();
        for line in &staged {
            if line.stock_adjustment_id.as_ref() != self.base.id.as_str() {
                return Err(Error::from("明细行不属于该调整单"));
            }
            self.reason_type.ensure_direction(line.direction)?;
        }
        let mut seen = HashSet::with_capacity(updates.len());
        let mut changed = Vec::with_capacity(updates.len());
        for update in updates {
            if !seen.insert(update.line_id.as_str()) {
                return Err(Error::from("调整明细行不得重复"));
            }
            let line = staged
                .iter_mut()
                .find(|line| line.base.id == update.line_id)
                .ok_or_else(|| Error::from("明细行不属于该调整单"))?;
            if line.stock_adjustment_id.as_ref() != self.base.id.as_str() {
                return Err(Error::from("明细行不属于该调整单"));
            }
            line.apply_update(self.reason_type, update.quantity, update.direction)?;
            changed.push(line.clone());
        }
        if require_all && seen.len() != staged.len() {
            return Err(Error::from("提交必须包含全部调整明细"));
        }
        lines.clone_from_slice(&staged);
        Ok(changed)
    }

    /// 校验当前状态可编辑。
    ///
    /// # 返回
    /// 可编辑返回 `Ok(())`。
    ///
    /// # 错误
    /// 待复核/待确认/已过账/已冲正不可编辑时返回错误。
    fn ensure_editable(&self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from(
                "待复核、待财务确认、已过账或已冲正的库存调整单不可编辑",
            ));
        }
        Ok(())
    }
}

/// 校验仓储复核人与经办人不同（岗位分离，§6.7）。
///
/// # 参数
/// * `prepared_by` - 仓储经办人
/// * `reviewed_by` - 仓储复核人
///
/// # 返回
/// 通过返回 `Ok(())`。
///
/// # 错误
/// 两者相同时返回错误。
fn ensure_reviewer_separation(prepared_by: &str, reviewed_by: &str) -> Result<()> {
    if prepared_by == reviewed_by {
        return Err(Error::from("仓储经办人与仓储复核人不得相同"));
    }
    Ok(())
}

/// 库存调整明细创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentLineData {
    /// 调整单。
    pub stock_adjustment_id: StockAdjustmentId,
    /// 调整 SKU。
    pub sku_id: SkuId,
    /// 调整数量（正数）。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

/// 已解析并完成基础校验的调整明细更新值对象。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentLineUpdate {
    /// 明细行主键。
    pub line_id: String,
    /// 调整数量。
    pub quantity: Quantity,
    /// 调整方向；空表示保持原方向。
    pub direction: Option<MovementDirection>,
}

impl StockAdjustmentLineUpdate {
    /// 从服务输入构造调整明细更新值对象。
    ///
    /// # 参数
    /// * `line_id` - 明细行主键
    /// * `quantity` - 定点数量字符串
    /// * `direction` - 可选调整方向
    ///
    /// # 返回
    /// 返回完成主键规范化与数量解析的更新值对象。
    ///
    /// # 错误
    /// 行主键为空/过长，或数量不是正数时返回错误。
    pub fn new(
        line_id: impl Into<String>,
        quantity: &str,
        direction: Option<MovementDirection>,
    ) -> Result<Self> {
        let line_id = normalize_required_text(
            line_id.into(),
            "明细行主键不能为空",
            LINE_ID_MAX_LEN,
            "明细行主键过长",
        )?;
        let quantity = Quantity::from_str(quantity)?;
        ensure_positive_quantity(quantity)?;
        Ok(Self {
            line_id,
            quantity,
            direction,
        })
    }
}

/// 库存调整明细实体（数据模型 §6.7 明细）。
///
/// 数量必须为正数；方向单独表达。明细调整与原因类型的方向一致性
/// （盘盈必增、盘亏/损坏必减）由 [`StockAdjustment::apply_line_updates`] 与
/// [`StockAdjustmentLine::new_for_reason`] 校验；状态可编辑性由调整单实体把关。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct StockAdjustmentLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 调整单。
    pub stock_adjustment_id: StockAdjustmentId,
    /// 调整 SKU。
    pub sku_id: SkuId,
    /// 调整数量。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

impl StockAdjustmentLine {
    /// 创建库存调整明细。
    ///
    /// 完成调整数量正数校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::StockAdjustmentLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的调整明细实体。
    ///
    /// # 错误
    /// 调整数量非正时返回错误。
    pub fn new(id: StockAdjustmentLineId, data: StockAdjustmentLineData) -> Result<Self> {
        ensure_positive_quantity(data.quantity)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stock_adjustment_id: data.stock_adjustment_id,
            sku_id: data.sku_id,
            quantity: data.quantity,
            direction: data.direction,
        })
    }

    /// 按调整原因创建库存调整明细。
    ///
    /// # 参数
    /// * `id` - 实体主键
    /// * `reason_type` - 调整原因
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回数量与方向均符合原因约束的明细实体。
    ///
    /// # 错误
    /// 数量非正，或方向与调整原因不一致时返回错误。
    pub fn new_for_reason(
        id: StockAdjustmentLineId,
        reason_type: AdjustmentReasonType,
        data: StockAdjustmentLineData,
    ) -> Result<Self> {
        reason_type.ensure_direction(data.direction)?;
        Self::new(id, data)
    }

    /// 应用调整明细数量与可选方向。
    ///
    /// # 参数
    /// * `reason_type` - 调整单当前原因
    /// * `quantity` - 新的正数数量
    /// * `direction` - 新方向；空表示保持现状
    ///
    /// # 返回
    /// 校验并更新成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 数量非正，或最终方向与调整原因不一致时返回错误。
    pub fn apply_update(
        &mut self,
        reason_type: AdjustmentReasonType,
        quantity: Quantity,
        direction: Option<MovementDirection>,
    ) -> Result<()> {
        ensure_positive_quantity(quantity)?;
        let direction = direction.unwrap_or(self.direction);
        reason_type.ensure_direction(direction)?;
        self.quantity = quantity;
        self.direction = direction;
        Ok(())
    }
}

/// 校验调整数量为正数。
///
/// # 参数
/// * `quantity` - 待校验数量
///
/// # 返回
/// 数量为正时返回 `Ok(())`。
///
/// # 错误
/// 数量为零或负数时返回错误。
fn ensure_positive_quantity(quantity: Quantity) -> Result<()> {
    if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("调整数量必须为正数"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::StockAdjustmentId;
    use std::str::FromStr;

    fn data() -> StockAdjustmentData {
        StockAdjustmentData {
            adjustment_no: " ADJ-2026-001 ".to_string(),
            warehouse_id: WarehouseId::new("wh-1"),
            reason_type: AdjustmentReasonType::StockLoss,
            prepared_by: " operator-1 ".to_string(),
            note: None,
            occurred_at: None,
        }
    }

    fn line_data() -> StockAdjustmentLineData {
        StockAdjustmentLineData {
            stock_adjustment_id: StockAdjustmentId::new("adj-1"),
            sku_id: SkuId::new("sku-1"),
            quantity: Quantity::from_str("2").unwrap(),
            direction: MovementDirection::Decrease,
        }
    }

    /// happy path：单号规范化、岗位分离、双审核与过账/冲正全链路。
    #[test]
    fn new_normalizes_and_drives_full_state_machine() {
        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("adj-1"), data()).unwrap();
        assert_eq!(adjustment.adjustment_no, "ADJ-2026-001");
        assert_eq!(adjustment.prepared_by, "operator-1");
        assert_eq!(adjustment.status, StockAdjustmentState::Draft);

        adjustment.submit_for_warehouse_review("reviewer-1").unwrap();
        assert_eq!(adjustment.reviewed_by.as_deref(), Some("reviewer-1"));
        assert_eq!(adjustment.status, StockAdjustmentState::PendingWarehouseReview);

        adjustment.submit_for_finance_review("finance-1").unwrap();
        assert_eq!(adjustment.finance_reviewed_by.as_deref(), Some("finance-1"));
        assert_eq!(adjustment.status, StockAdjustmentState::PendingFinanceReview);

        adjustment.mark_posted().unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Posted);
        adjustment.reverse().unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Reversed);
    }

    /// 失败路径：必填空、复核人=经办人、状态不允许的迁移。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = StockAdjustmentData {
            adjustment_no: "   ".to_string(),
            ..data()
        };
        assert!(StockAdjustment::new(StockAdjustmentId::new("a2"), blank_no).is_err());

        let blank_prepared = StockAdjustmentData {
            prepared_by: "  ".to_string(),
            ..data()
        };
        assert!(StockAdjustment::new(StockAdjustmentId::new("a3"), blank_prepared).is_err());

        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("a4"), data()).unwrap();
        assert!(
            adjustment.submit_for_warehouse_review("operator-1").is_err(),
            "复核人不得与经办人相同"
        );
        assert!(adjustment
            .update(StockAdjustmentUpdate {
                reviewed_by: Some("operator-1".to_string()),
                ..StockAdjustmentUpdate::default()
            })
            .is_err());
        assert!(adjustment.mark_posted().is_err(), "未经审核不能过账");
        assert!(adjustment.reverse().is_err(), "草稿不能冲正");
    }

    /// 驳回路径：复核/财务确认可驳回，驳回后修改并重新提交复核。
    #[test]
    fn reject_path_resubmits_after_rejection() {
        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("a5"), data()).unwrap();
        adjustment.submit_for_warehouse_review("reviewer-1").unwrap();
        adjustment.reject().unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Rejected);
        assert!(adjustment.is_editable(), "驳回后允许修改");

        adjustment.submit_for_warehouse_review("reviewer-2").unwrap();
        adjustment.submit_for_finance_review("finance-1").unwrap();
        adjustment.reject().unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Rejected);
        assert!(adjustment.mark_posted().is_err(), "驳回后不能过账");
    }

    /// 状态机：固定邻接矩阵的合法/非法迁移（含终态与幂等）。
    #[test]
    fn state_machine_transition_matrix() {
        assert!(ensure_transition(
            StockAdjustmentState::Draft,
            StockAdjustmentState::PendingWarehouseReview
        )
        .is_ok());
        assert!(ensure_transition(
            StockAdjustmentState::PendingWarehouseReview,
            StockAdjustmentState::PendingFinanceReview
        )
        .is_ok());
        assert!(ensure_transition(
            StockAdjustmentState::PendingWarehouseReview,
            StockAdjustmentState::Rejected
        )
        .is_ok());
        assert!(ensure_transition(
            StockAdjustmentState::PendingFinanceReview,
            StockAdjustmentState::Posted
        )
        .is_ok());
        assert!(ensure_transition(
            StockAdjustmentState::PendingFinanceReview,
            StockAdjustmentState::Rejected
        )
        .is_ok());
        assert!(
            ensure_transition(
                StockAdjustmentState::Rejected,
                StockAdjustmentState::PendingWarehouseReview
            )
            .is_ok(),
            "驳回后可修改并重新提交复核"
        );
        assert!(ensure_transition(StockAdjustmentState::Posted, StockAdjustmentState::Reversed).is_ok());
        assert!(ensure_transition(StockAdjustmentState::Draft, StockAdjustmentState::Posted).is_err());
        assert!(ensure_transition(StockAdjustmentState::Draft, StockAdjustmentState::Reversed).is_err());
        assert!(ensure_transition(StockAdjustmentState::Rejected, StockAdjustmentState::Posted).is_err());
        assert!(ensure_transition(StockAdjustmentState::Rejected, StockAdjustmentState::Draft).is_err());
        assert!(ensure_transition(StockAdjustmentState::Reversed, StockAdjustmentState::Posted).is_err());
        assert!(ensure_transition(StockAdjustmentState::Reversed, StockAdjustmentState::Reversed).is_ok());
        assert!(ensure_transition(StockAdjustmentState::Draft, StockAdjustmentState::Draft).is_ok());
    }

    /// happy path：调整明细创建成功。
    #[test]
    fn line_new_succeeds() {
        let line = StockAdjustmentLine::new(StockAdjustmentLineId::new("al-1"), line_data()).unwrap();
        assert_eq!(line.quantity, Quantity::from_str("2").unwrap());
        assert_eq!(line.direction, MovementDirection::Decrease);
    }

    /// 失败路径：数量越界（非正）。
    #[test]
    fn line_rejects_quantity_violations() {
        let zero_quantity = StockAdjustmentLineData {
            quantity: Quantity::from_str("0").unwrap(),
            ..line_data()
        };
        assert!(StockAdjustmentLine::new(StockAdjustmentLineId::new("al-2"), zero_quantity).is_err());

        let negative = StockAdjustmentLineData {
            quantity: Quantity::from_str("-1").unwrap(),
            ..line_data()
        };
        assert!(StockAdjustmentLine::new(StockAdjustmentLineId::new("al-3"), negative).is_err());
    }

    /// 原因规则：方向与正式流水类型由原因实体统一决定。
    #[test]
    fn reason_owns_direction_and_movement_type_rules() {
        assert_eq!(
            AdjustmentReasonType::StockGain.movement_direction(),
            MovementDirection::Increase
        );
        assert_eq!(
            AdjustmentReasonType::StockLoss.movement_type(),
            MovementType::StockLoss
        );
        assert!(AdjustmentReasonType::Damage
            .ensure_direction(MovementDirection::Decrease)
            .is_ok());
        assert!(AdjustmentReasonType::Damage
            .ensure_direction(MovementDirection::Increase)
            .is_err());
    }

    /// 明细更新：整组校验失败不产生部分修改，完整合法输入一次应用。
    #[test]
    fn line_updates_are_validated_atomically() {
        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("adj-1"), data()).unwrap();
        adjustment.base.version = 3;
        assert!(adjustment.matches_version(3));
        assert!(!adjustment.matches_version(2));

        let mut lines = vec![
            StockAdjustmentLine::new_for_reason(
                StockAdjustmentLineId::new("al-1"),
                adjustment.reason_type,
                line_data(),
            )
            .unwrap(),
            StockAdjustmentLine::new_for_reason(
                StockAdjustmentLineId::new("al-2"),
                adjustment.reason_type,
                StockAdjustmentLineData {
                    sku_id: SkuId::new("sku-2"),
                    ..line_data()
                },
            )
            .unwrap(),
        ];
        let original = lines.clone();
        let mut gain_adjustment = adjustment.clone();
        gain_adjustment.reason_type = AdjustmentReasonType::StockGain;
        assert!(gain_adjustment
            .apply_line_updates(&mut lines, &[], false)
            .is_err());
        assert_eq!(lines, original, "原因变更必须重验全部既有方向");

        let incomplete = vec![StockAdjustmentLineUpdate::new("al-1", "3", None).unwrap()];
        assert!(adjustment
            .apply_line_updates(&mut lines, &incomplete, true)
            .is_err());
        assert_eq!(lines, original, "完整性失败不得留下部分更新");

        let wrong_direction =
            vec![StockAdjustmentLineUpdate::new("al-1", "3", Some(MovementDirection::Increase)).unwrap()];
        assert!(adjustment
            .apply_line_updates(&mut lines, &wrong_direction, false)
            .is_err());
        assert_eq!(lines, original, "方向失败不得留下部分更新");

        let updates = vec![
            StockAdjustmentLineUpdate::new("al-1", "3", None).unwrap(),
            StockAdjustmentLineUpdate::new("al-2", "4", Some(MovementDirection::Decrease)).unwrap(),
        ];
        let changed = adjustment.apply_line_updates(&mut lines, &updates, true).unwrap();
        assert_eq!(changed.len(), 2);
        assert_eq!(lines[0].quantity, Quantity::from_str("3").unwrap());
        assert_eq!(lines[1].quantity, Quantity::from_str("4").unwrap());
    }

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&StockAdjustmentState::PendingFinanceReview).unwrap(),
            "\"PENDING_FINANCE_REVIEW\""
        );
        assert_eq!(
            serde_json::to_string(&AdjustmentReasonType::StockGain).unwrap(),
            "\"STOCK_GAIN\""
        );
        assert_eq!(AdjustmentReasonType::Damage.label(), "损坏");
        assert_eq!(StockAdjustmentState::Rejected.label(), "驳回");

        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("a6"), data()).unwrap();
        adjustment.submit_for_warehouse_review("reviewer-1").unwrap();
        let roundtrip: StockAdjustment =
            bson::deserialize_from_document(bson::serialize_to_document(&adjustment).unwrap()).unwrap();
        assert_eq!(roundtrip, adjustment);
    }
}
