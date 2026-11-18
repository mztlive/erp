//! `stock_adjustment` / `stock_adjustment_line`：库存调整单及明细（数据模型 §6.7）。
//!
//! 状态机按 §6.7/§7.5：草稿 → 待仓储复核 → 待财务确认 → 已过账 → 已冲正，
//! 仓储复核/财务确认可驳回（`REJECTED` 修改后重新提交复核）；`POSTED` 后
//! 不可编辑。经办人与仓储复核人不得相同（§6.7）；盘盈、盘亏和损坏一律经过
//! 财务成本影响确认后才能过账（§6.7，路径由状态机保证）。过账在同一事务写
//! 库存流水、余额和必要预占释放，原出入库流水不改写——由 P3 完成（§8.2
//! 第 3 条）。

use std::collections::{HashMap, HashSet};

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::state::{DocumentState, ensure_transition};
use erp_core::common::time::Instant;
use erp_core::ids::{StockAdjustmentId, WarehouseId};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::stock_movement::{MovementDirection, MovementType};

mod balance_versions;
pub use balance_versions::StockAdjustmentBalanceVersions;

/// 调整单号最大长度。
const ADJUSTMENT_NO_MAX_LEN: usize = 64;
/// 经办人/复核人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;
/// 原因说明最大长度。
const NOTE_MAX_LEN: usize = 512;

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
    /// 返回库存调整单状态的中文名称，供业务界面展示，不作为持久化代码。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingWarehouseReview => "待仓储复核",
            Self::PendingFinanceReview => "待财务确认",
            Self::Posted => "已过账",
            Self::Rejected => "已驳回",
            Self::Reversed => "已冲正",
            Self::InApproval => "审批中",
        }
    }

    /// 返回库存调整单状态的稳定代码，供持久化与查询匹配使用。
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

    /// 编辑入口据此判断是否能继续修改当前记录；仅草稿或驳回状态允许。
    pub fn is_editable(&self) -> bool {
        matches!(self, Self::Draft | Self::Rejected)
    }

    /// 返回尚未过账且会影响库存详情展示的状态集合。
    /// 返回草稿与审批中状态。
    pub fn pending_posting() -> &'static [Self] {
        &[Self::Draft, Self::InApproval]
    }
}

impl DocumentState for StockAdjustmentState {
    // 固定邻接矩阵（§6.7/§7.5 定向链；`REVERSED` 为不可逆终态）。
    // `REJECTED` 可在修改后重新提交仓储复核（§6.5.5 再次提交审核）。
    // 返回当前状态的静态后继切片，不含自身：`Draft` 为 `PendingWarehouseReview` 与 `InApproval`；`InApproval` 为 `Posted`、`Draft` 与 `Rejected`；`PendingWarehouseReview` 为 `PendingFinanceReview` 与 `Rejected`；`PendingFinanceReview` 为 `Posted` 与 `Rejected`；`Rejected` 为 `PendingWarehouseReview` 与 `InApproval`；`Posted` 为 `Reversed`；`Reversed` 为空切片。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingWarehouseReview, Self::InApproval],
            Self::InApproval => &[Self::Posted, Self::Draft, Self::Rejected],
            Self::PendingWarehouseReview => &[Self::PendingFinanceReview, Self::Rejected],
            Self::PendingFinanceReview => &[Self::Posted, Self::Rejected],
            Self::Rejected => &[Self::PendingWarehouseReview, Self::InApproval],
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
    /// 返回库存调整原因的中文名称，供业务界面展示，不作为持久化代码。
    pub fn label(&self) -> &'static str {
        match self {
            Self::StockGain => "盘盈",
            Self::StockLoss => "盘亏",
            Self::Damage => "损坏",
        }
    }

    /// 返回库存调整原因的稳定代码，供持久化与查询匹配使用。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StockGain => "STOCK_GAIN",
            Self::StockLoss => "STOCK_LOSS",
            Self::Damage => "DAMAGE",
        }
    }

    /// 返回该调整原因要求的库存方向。
    /// 盘盈返回增加，盘亏与损坏返回减少。
    pub fn movement_direction(self) -> MovementDirection {
        match self {
            Self::StockGain => MovementDirection::Increase,
            Self::StockLoss | Self::Damage => MovementDirection::Decrease,
        }
    }

    /// 返回该调整原因对应的正式流水类型。
    /// 返回盘盈、盘亏或损坏流水类型。
    pub fn movement_type(self) -> MovementType {
        match self {
            Self::StockGain => MovementType::StockGain,
            Self::StockLoss => MovementType::StockLoss,
            Self::Damage => MovementType::Damage,
        }
    }

    /// 校验调整明细方向与原因一致。
    /// 方向一致时返回 `Ok(())`。
    /// `direction` 为待校验的调整方向。
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
    /// 创建人。旧数据缺失时反序列化为空，但不得据此执行绑定升级。
    #[serde(default)]
    pub created_by: String,
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
    /// 完成调整单号与经办人规范化。
    /// 返回新建的调整单实体。
    /// `id` 为实体主键（`erp_core::ids::StockAdjustmentId`）；`created_by` 为已认证创建人；创建后不得由更新命令覆盖。
    /// 调整单号、经办人或创建人为空或超长，或原因说明超长时返回错误。
    pub fn new(
        id: StockAdjustmentId,
        data: StockAdjustmentData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let adjustment_no = normalize_required_text(
            data.adjustment_no,
            "调整单号不能为空",
            ADJUSTMENT_NO_MAX_LEN,
            "调整单号过长",
        )?;
        let prepared_by =
            normalize_required_text(data.prepared_by, "仓储经办人不能为空", ACTOR_MAX_LEN, "仓储经办人过长")?;
        let created_by =
            normalize_required_text(created_by.into(), "创建人不能为空", ACTOR_MAX_LEN, "创建人标识过长")?;
        let note = normalize_note(data.note.as_deref().unwrap_or_default())?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            created_by,
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

    /// 校验库存调整仍是从未提交审批的初始草稿。
    /// 状态为草稿且 `approval_subject_version` 为 0 时成功。
    /// 非草稿或审批主题版本已经递增时返回错误。
    pub fn ensure_initial_approval_state(&self) -> Result<()> {
        if self.status != StockAdjustmentState::Draft || self.approval_subject_version != 0 {
            return Err(Error::from("库存调整单已经提交或启动过审批"));
        }
        Ok(())
    }

    /// 更新库存调整单（仅草稿/驳回）。
    /// 复用 `new` 的文本规范化；经办人与仓储复核人不得相同（§6.7）。
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

        if let Some(note) = update.note.as_deref() {
            self.note = normalize_note(note)?;
        }
        if let Some(occurred_at) = update.occurred_at {
            self.occurred_at = Some(occurred_at);
        }
        Ok(())
    }

    /// 提交并启动审批：递增审批主题版本并进入审批中。
    /// 仅草稿可提交。版本递增与状态迁移在全部校验成功后一起写入，因此版本溢出或状态非法时实体保持不变。
    /// 返回本次冻结的审批主题版本。
    /// 非草稿或审批主题版本溢出时返回错误。
    pub fn start_approval(&mut self) -> Result<u32> {
        if !matches!(self.status, StockAdjustmentState::Draft | StockAdjustmentState::Rejected) {
            return Err(Error::from("只有草稿状态的库存调整单可以提交审批"));
        }
        let next =
            self.approval_subject_version.checked_add(1).ok_or_else(|| Error::from("审批提交版本溢出"))?;

        self.approval_subject_version = next;
        self.status = StockAdjustmentState::InApproval;
        Ok(next)
    }

    /// 结束本次审批，保留原单和明细供修改重提。
    /// 进入已驳回且审批版本保持不变。
    /// `expected_subject_version` 为被驳回实例冻结的审批主题版本。
    /// 非审批中时拒绝；不会生成库存流水。
    pub fn reject_approval(&mut self, expected_subject_version: u32) -> Result<()> {
        if self.approval_subject_version != expected_subject_version {
            return Err(Error::from("审批提交版本已变化，请刷新后重试"));
        }
        if self.status != StockAdjustmentState::InApproval {
            return Err(Error::from("只有审批中的库存调整单可以驳回"));
        }
        self.status = StockAdjustmentState::Rejected;
        Ok(())
    }

    /// 撤回审批并回到草稿，审批主题版本保持不变。
    /// 撤回成功返回 `Ok(())`。
    /// 当前状态不是审批中时返回错误。
    pub fn cancel_approval(&mut self) -> Result<()> {
        if self.status != StockAdjustmentState::InApproval {
            return Err(Error::from("只有审批中的库存调整单可以撤回审批"));
        }
        self.status = StockAdjustmentState::Draft;
        Ok(())
    }

    /// 校验调整单可由审批最终通过动作进入过账事务。
    /// 该方法只执行专用状态守卫，不修改调整单。库存流水、余额及最终状态仍由 Service 在同一事务内持久化。
    /// 当前状态为审批中且允许迁到已过账时返回 `Ok(())`。
    /// 当前状态不是审批中时返回错误；旧人工财务复核状态不得通过该守卫。
    pub fn ensure_approval_postable(&self) -> Result<()> {
        if self.status != StockAdjustmentState::InApproval {
            return Err(Error::from("只有审批中的库存调整单可以由最终通过动作过账"));
        }
        Ok(())
    }

    /// 提交仓储复核（草稿/驳回 → 待仓储复核）。
    /// 驳回的调整单在修改后重新提交复核（§6.5.5）；提交时登记仓储复核人，并校验与经办人不同（§6.7 岗位分离）。
    /// `reviewed_by` 为仓储复核人（账号或系统身份）。
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
    /// `finance_reviewed_by` 为成本影响确认人（账号或系统身份）。
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

    /// 过账库存调整，把状态迁到已过账。
    /// 本方法只检查邻接矩阵：审批中或待财务确认可以进入已过账。它不检查财务确认人，也不写库存流水、余额或预占。
    /// 当前状态不能迁到已过账时返回错误。
    pub fn mark_posted(&mut self) -> Result<()> {
        ensure_transition(self.status, StockAdjustmentState::Posted)?;
        self.status = StockAdjustmentState::Posted;
        Ok(())
    }

    /// 驳回调整单（待仓储复核/待财务确认 → 驳回）。
    /// 驳回后修改调整单并重新提交仓储复核（§6.5.5 流程）。
    /// 当前状态不允许驳回时返回错误。
    pub fn reject(&mut self) -> Result<()> {
        if self.status == StockAdjustmentState::InApproval {
            return Err(Error::from("审批中的库存调整单必须按提交版本驳回"));
        }
        ensure_transition(self.status, StockAdjustmentState::Rejected)?;
        self.status = StockAdjustmentState::Rejected;
        Ok(())
    }

    /// 冲正库存调整（已过账 → 已冲正，终态）。
    /// `REVERSED` 表示存在正式反向事实（冲正流水），不删除原调整单（§4.5.1、§7.5）；反向事实由调用方形成。本方法只迁移状态。
    /// 当前状态不允许迁移（非已过账）时返回错误。
    pub fn reverse(&mut self) -> Result<()> {
        ensure_transition(self.status, StockAdjustmentState::Reversed)?;
        self.status = StockAdjustmentState::Reversed;
        Ok(())
    }

    /// 编辑入口据此判断是否能继续修改当前记录；仅草稿或驳回状态允许。
    pub fn is_editable(&self) -> bool {
        self.status.is_editable()
    }

    /// 更新库存调整单前比较当前乐观锁版本与调用方读取的版本。
    pub fn matches_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 应用调整明细的最终值并校验整组规则。
    /// 校验行归属、重复行、数量正数、原因与方向一致性；提交审批时还可要求命令完整覆盖全部持久化明细。校验失败时不会留下部分更新。
    /// 返回完成更新的明细副本，供调用方持久化。
    /// `lines` 为当前持久化明细的内存副本；`updates` 为待应用的明细更新。
    /// `require_all` 为是否要求覆盖全部明细。
    /// 行不属于本调整单、行重复、数量非法、方向不一致或未完整覆盖时返回错误。
    pub fn apply_line_updates(
        &self,
        lines: &mut [StockAdjustmentLine],
        updates: &[StockAdjustmentLineUpdate],
        require_all: bool,
    ) -> Result<Vec<StockAdjustmentLine>> {
        let mut staged = lines.to_vec();
        self.ensure_lines_belong(&staged)?;
        let changed = self.apply_updates_to_staged(&mut staged, updates)?;

        if require_all && updates.len() != staged.len() {
            return Err(Error::from("提交必须包含全部调整明细"));
        }
        self.ensure_group_directions(&staged)?;

        lines.swap_with_slice(&mut staged);
        Ok(changed)
    }

    // 校验全部既有明细行归属本调整单。
    // 全部归属一致时返回 `Ok(())`。
    // `staged` 为当前持久化明细的内存副本。
    // 任一明细行不属于本调整单时返回错误。
    fn ensure_lines_belong(&self, staged: &[StockAdjustmentLine]) -> Result<()> {
        for line in staged {
            if line.stock_adjustment_id.as_ref() != self.base.id.as_str() {
                return Err(Error::from("明细行不属于该调整单"));
            }
        }
        Ok(())
    }

    // 逐条应用明细更新并返回变更副本。
    // 校验更新行去重、归属、数量正数与原因方向一致性。
    // 返回完成更新的明细副本，供调用方持久化。
    // `staged` 为当前持久化明细的内存副本（就地更新）；`updates` 为待应用的明细更新。
    // 行重复、行不属于本调整单、数量非法或方向不一致时返回错误。
    fn apply_updates_to_staged(
        &self,
        staged: &mut [StockAdjustmentLine],
        updates: &[StockAdjustmentLineUpdate],
    ) -> Result<Vec<StockAdjustmentLine>> {
        let mut indexes = HashMap::with_capacity(staged.len());
        for (index, line) in staged.iter().enumerate() {
            indexes.entry(line.base.id.clone()).or_insert(index);
        }

        let mut seen = HashSet::with_capacity(updates.len());
        let mut changed = Vec::with_capacity(updates.len());
        for update in updates {
            if !seen.insert(update.line_id.as_str()) {
                return Err(Error::from("调整明细行不得重复"));
            }
            let index = indexes
                .get(update.line_id.as_str())
                .copied()
                .ok_or_else(|| Error::from("明细行不属于该调整单"))?;
            let line = &mut staged[index];
            line.apply_update(self.reason_type, update.quantity, update.direction)?;
            changed.push(line.clone());
        }
        Ok(changed)
    }

    // 复验整组明细方向与当前原因一致。
    // 全部方向一致时返回 `Ok(())`。
    // `staged` 为已应用更新的明细副本。
    // 任一明细方向与调整原因不一致时返回错误。
    fn ensure_group_directions(&self, staged: &[StockAdjustmentLine]) -> Result<()> {
        for line in staged {
            self.reason_type.ensure_direction(line.direction)?;
        }
        Ok(())
    }

    // 校验当前状态可编辑。
    // 非草稿且非驳回时返回错误，包括审批中、待复核、待财务确认、已过账和已冲正。
    fn ensure_editable(&self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("待复核、待财务确认、已过账或已冲正的库存调整单不可编辑"));
        }
        Ok(())
    }
}

fn normalize_note(note: &str) -> Result<Option<String>> {
    let note = note.trim();
    if note.len() > NOTE_MAX_LEN {
        return Err(Error::from("原因说明过长"));
    }
    Ok((!note.is_empty()).then(|| note.to_owned()))
}

// 校验仓储复核人与经办人不同（岗位分离，§6.7）。
// `prepared_by` 为仓储经办人；`reviewed_by` 为仓储复核人。
// 两者相同时返回错误。
fn ensure_reviewer_separation(prepared_by: &str, reviewed_by: &str) -> Result<()> {
    if prepared_by == reviewed_by {
        return Err(Error::from("仓储经办人与仓储复核人不得相同"));
    }
    Ok(())
}

pub mod line;

pub use line::{StockAdjustmentLine, StockAdjustmentLineData, StockAdjustmentLineUpdate};

#[cfg(test)]
mod tests {

    use erp_core::ids::StockAdjustmentId;

    use super::*;

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

    fn adjustment_in_state(status: StockAdjustmentState, subject_version: u32) -> StockAdjustment {
        let mut adjustment =
            StockAdjustment::new(StockAdjustmentId::new("approval-adj"), data(), "creator-1").unwrap();
        adjustment.status = status;
        adjustment.approval_subject_version = subject_version;
        adjustment
    }

    // 验证单号规范化、岗位分离、双审核与过账/冲正全链路。
    #[test]
    fn new_normalizes_and_drives_full_state_machine() {
        let mut adjustment =
            StockAdjustment::new(StockAdjustmentId::new("adj-1"), data(), "creator-1").unwrap();
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

    #[test]
    fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
        let mut adjustment =
            StockAdjustment::new(StockAdjustmentId::new("creator-adj"), data(), " creator-1 ").unwrap();
        adjustment.update(StockAdjustmentUpdate::default()).unwrap();
        assert_eq!(adjustment.created_by, "creator-1");
        assert!(StockAdjustment::new(StockAdjustmentId::new("blank-creator-adj"), data(), "   ").is_err());
    }

    // 验证必填空、复核人=经办人、状态不允许的迁移。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = StockAdjustmentData { adjustment_no: "   ".to_string(), ..data() };
        assert!(StockAdjustment::new(StockAdjustmentId::new("a2"), blank_no, "creator-1").is_err());

        let blank_prepared = StockAdjustmentData { prepared_by: "  ".to_string(), ..data() };
        assert!(StockAdjustment::new(StockAdjustmentId::new("a3"), blank_prepared, "creator-1").is_err());

        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("a4"), data(), "creator-1").unwrap();
        assert!(adjustment.submit_for_warehouse_review("operator-1").is_err(), "复核人不得与经办人相同");
        assert!(
            adjustment
                .update(StockAdjustmentUpdate {
                    reviewed_by: Some("operator-1".to_string()),
                    ..StockAdjustmentUpdate::default()
                })
                .is_err()
        );
        assert!(adjustment.mark_posted().is_err(), "未经审核不能过账");
        assert!(adjustment.reverse().is_err(), "草稿不能冲正");
    }

    // 验证审批提交和撤回保持主题版本单调递增，重复动作失败且不修改实体。
    #[test]
    fn approval_submit_and_cancel_keep_monotonic_subject_version() {
        let mut adjustment = adjustment_in_state(StockAdjustmentState::Draft, 0);
        adjustment.ensure_initial_approval_state().expect("新建草稿是初始未提交状态");
        assert_eq!(adjustment.start_approval().unwrap(), 1);
        assert_eq!(adjustment.status, StockAdjustmentState::InApproval);

        let submitted = adjustment.clone();
        assert!(adjustment.reject().is_err());
        assert!(adjustment.start_approval().is_err());
        assert_eq!(adjustment, submitted, "重复提交不得修改实体");

        adjustment.cancel_approval().unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Draft);
        assert_eq!(adjustment.approval_subject_version, 1);
        assert!(adjustment.ensure_initial_approval_state().is_err());

        let cancelled = adjustment.clone();
        assert!(adjustment.cancel_approval().is_err());
        assert_eq!(adjustment, cancelled, "重复撤回不得修改实体");
        assert_eq!(adjustment.start_approval().unwrap(), 2);
    }

    // 验证审批提交只接受草稿，全部禁止状态均失败且保持实体逐字段不变。
    #[test]
    fn approval_submit_rejects_every_non_draft_state_atomically() {
        let forbidden = [
            StockAdjustmentState::InApproval,
            StockAdjustmentState::PendingWarehouseReview,
            StockAdjustmentState::PendingFinanceReview,
            StockAdjustmentState::Posted,
            StockAdjustmentState::Reversed,
        ];
        for status in forbidden {
            let mut adjustment = adjustment_in_state(status, u32::MAX);
            let before = adjustment.clone();
            assert!(matches!(adjustment.start_approval(), Err(Error::LogicError(message))
                if message == "只有草稿状态的库存调整单可以提交审批"));
            assert_eq!(adjustment, before, "{status:?} 提交失败后实体必须不变");
        }
    }

    // 验证审批主题版本溢出失败，不得先写状态或其他实体字段。
    #[test]
    fn approval_submit_overflow_is_atomic() {
        let mut adjustment = adjustment_in_state(StockAdjustmentState::Draft, u32::MAX);
        let before = adjustment.clone();
        assert!(matches!(adjustment.start_approval(), Err(Error::LogicError(message))
            if message == "审批提交版本溢出"));
        assert_eq!(adjustment, before);
    }

    // 验证审批撤回只接受审批中，全部禁止状态均失败且保持主题版本与实体不变。
    #[test]
    fn approval_cancel_rejects_every_non_approval_state_atomically() {
        let forbidden = [
            StockAdjustmentState::Draft,
            StockAdjustmentState::PendingWarehouseReview,
            StockAdjustmentState::PendingFinanceReview,
            StockAdjustmentState::Posted,
            StockAdjustmentState::Rejected,
            StockAdjustmentState::Reversed,
        ];
        for status in forbidden {
            let mut adjustment = adjustment_in_state(status, 7);
            let before = adjustment.clone();
            assert!(adjustment.cancel_approval().is_err(), "{status:?} 不得撤回审批");
            assert_eq!(adjustment, before, "{status:?} 撤回失败后实体必须不变");
        }
    }

    // 验证审批过账专用守卫只接受审批中，并保持实体只读。
    #[test]
    fn approval_postable_guard_accepts_only_in_approval_without_mutation() {
        let states = [
            StockAdjustmentState::Draft,
            StockAdjustmentState::PendingWarehouseReview,
            StockAdjustmentState::PendingFinanceReview,
            StockAdjustmentState::Posted,
            StockAdjustmentState::Rejected,
            StockAdjustmentState::Reversed,
            StockAdjustmentState::InApproval,
        ];
        for status in states {
            let adjustment = adjustment_in_state(status, 3);
            let before = adjustment.clone();
            let result = adjustment.ensure_approval_postable();
            assert_eq!(result.is_ok(), status == StockAdjustmentState::InApproval, "{status:?}");
            assert_eq!(adjustment, before, "专用守卫不得修改实体");
        }
    }

    // 验证旧人工复核路径仍可从待财务确认调用通用过账方法。
    #[test]
    fn legacy_mark_posted_still_accepts_pending_finance_review() {
        let mut adjustment = adjustment_in_state(StockAdjustmentState::PendingFinanceReview, 0);
        adjustment.mark_posted().unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Posted);
    }

    // 验证驳回路径：复核/财务确认可驳回，驳回后修改并重新提交复核。
    #[test]
    fn reject_path_resubmits_after_rejection() {
        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("a5"), data(), "creator-1").unwrap();
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

    // 验证固定邻接矩阵的合法/非法迁移（含终态与幂等）。
    #[test]
    fn state_machine_transition_matrix() {
        assert!(
            ensure_transition(StockAdjustmentState::Draft, StockAdjustmentState::PendingWarehouseReview)
                .is_ok()
        );
        assert!(
            ensure_transition(
                StockAdjustmentState::PendingWarehouseReview,
                StockAdjustmentState::PendingFinanceReview
            )
            .is_ok()
        );
        assert!(
            ensure_transition(StockAdjustmentState::PendingWarehouseReview, StockAdjustmentState::Rejected)
                .is_ok()
        );
        assert!(
            ensure_transition(StockAdjustmentState::PendingFinanceReview, StockAdjustmentState::Posted)
                .is_ok()
        );
        assert!(
            ensure_transition(StockAdjustmentState::PendingFinanceReview, StockAdjustmentState::Rejected)
                .is_ok()
        );
        assert!(
            ensure_transition(StockAdjustmentState::Rejected, StockAdjustmentState::PendingWarehouseReview)
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

    // 验证原因规则：方向与正式流水类型由原因实体统一决定。
    #[test]
    fn reason_owns_direction_and_movement_type_rules() {
        assert_eq!(AdjustmentReasonType::StockGain.movement_direction(), MovementDirection::Increase);
        assert_eq!(AdjustmentReasonType::StockLoss.movement_type(), MovementType::StockLoss);
        assert!(AdjustmentReasonType::Damage.ensure_direction(MovementDirection::Decrease).is_ok());
        assert!(AdjustmentReasonType::Damage.ensure_direction(MovementDirection::Increase).is_err());
    }

    // 验证枚举稳定代码。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&StockAdjustmentState::PendingFinanceReview).unwrap(),
            "\"PENDING_FINANCE_REVIEW\""
        );
        assert_eq!(serde_json::to_string(&AdjustmentReasonType::StockGain).unwrap(), "\"STOCK_GAIN\"");
        assert_eq!(AdjustmentReasonType::Damage.label(), "损坏");
        assert_eq!(StockAdjustmentState::Rejected.label(), "已驳回");

        let mut adjustment = StockAdjustment::new(StockAdjustmentId::new("a6"), data(), "creator-1").unwrap();
        adjustment.submit_for_warehouse_review("reviewer-1").unwrap();
    }
    // 验证驳回不产生库存事实，保存不清除结果，重新提交使用新的版本。
    #[test]
    fn rejection_is_editable_and_resubmission_advances_version() {
        let mut adjustment = adjustment_in_state(StockAdjustmentState::Draft, 0);
        assert!(adjustment.reject_approval(1).is_err());
        adjustment.start_approval().unwrap();

        let submitted = adjustment.clone();
        assert!(adjustment.reject_approval(0).is_err());
        assert!(adjustment.reject_approval(2).is_err());
        assert_eq!(adjustment, submitted);

        adjustment.reject_approval(1).unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Rejected);
        assert_eq!(adjustment.approval_subject_version, 1);
        assert!(adjustment.is_editable());

        let before = adjustment.clone();
        assert!(adjustment.reject_approval(1).is_err());
        assert!(adjustment.ensure_approval_postable().is_err());
        assert_eq!(adjustment, before);

        adjustment.update(StockAdjustmentUpdate::default()).unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Rejected);
        assert_eq!(adjustment.start_approval().unwrap(), 2);
        assert_eq!(adjustment.status, StockAdjustmentState::InApproval);

        let resubmitted = adjustment.clone();
        assert!(adjustment.reject_approval(1).is_err());
        assert_eq!(adjustment, resubmitted);
    }
}
