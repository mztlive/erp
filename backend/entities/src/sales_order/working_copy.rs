//! `sales_order_working_copy` 与 `sales_order_working_copy_line`（数据模型 §6.5）。
//!
//! 工作副本承载页面自动保存的可编辑草稿，不是提交快照或正式版本；同一销售单和
//! 编辑目的同时最多一个有效工作副本（唯一性由仓储/索引保证）。提交事务把草稿
//! 头、行原样复制成不可变 `sales_order_submission`，再把工作副本标记已提交。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    ContractId, CustomerAccountId, PartyId, SalesChangeOrderId, SalesOrderId, SalesOrderLineId,
    SalesOrderRevisionId, SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId, SkuId,
};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::snapshot::HeaderSnapshots;
use super::types::{
    build_line_groups, validate_line_list, BusinessType, FulfillmentMode, GoodsLineFields, LineSummary,
    LineType, VoucherLineDraft, WelfareScenario,
};

/// 内容指纹最大长度。
const CONTENT_HASH_MAX_LEN: usize = 128;
/// 编辑人标识最大长度。
const EDITOR_MAX_LEN: usize = 128;
/// 项目名称最大长度。
const PROJECT_NAME_MAX_LEN: usize = 256;
/// 业务备注最大长度。
const BUSINESS_REMARK_MAX_LEN: usize = 1024;
/// 销售项名称快照最大长度。
const ITEM_NAME_MAX_LEN: usize = 256;
/// 规格快照最大长度。
const SPEC_MAX_LEN: usize = 256;
/// 单位快照最大长度。
const UNIT_MAX_LEN: usize = 64;

/// 工作副本编辑目的（数据模型 §6.5：首次提交或销售变更）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkingPurpose {
    /// 首次提交。
    FirstSubmission,
    /// 销售变更。
    SalesChange,
}

impl WorkingPurpose {
    /// 返回编辑目的的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::FirstSubmission => "首次提交",
            Self::SalesChange => "销售变更",
        }
    }

    /// 返回编辑目的的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FirstSubmission => "FIRST_SUBMISSION",
            Self::SalesChange => "SALES_CHANGE",
        }
    }
}

/// 工作副本状态（数据模型 §6.5：编辑中、已提交、已放弃、冲突）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkingCopyStatus {
    /// 编辑中。
    Editing,
    /// 已提交。
    Submitted,
    /// 已放弃。
    Abandoned,
    /// 冲突。
    Conflict,
}

impl WorkingCopyStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Editing => "编辑中",
            Self::Submitted => "已提交",
            Self::Abandoned => "已放弃",
            Self::Conflict => "冲突",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Editing => "EDITING",
            Self::Submitted => "SUBMITTED",
            Self::Abandoned => "ABANDONED",
            Self::Conflict => "CONFLICT",
        }
    }
}

impl DocumentState for WorkingCopyStatus {
    /// 编辑中可提交/放弃/标记冲突；冲突解决后回到编辑中；已提交/已放弃为终态
    /// （驳回后以原提交复制出新的工作副本，旧副本不复用，§6.5）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Editing => &[Self::Submitted, Self::Abandoned, Self::Conflict],
            Self::Conflict => &[Self::Editing],
            Self::Submitted | Self::Abandoned => &[],
        }
    }
}

/// 工作副本创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderWorkingCopyData {
    /// 稳定销售单。
    pub sales_order_id: SalesOrderId,
    /// 编辑目的。
    pub working_purpose: WorkingPurpose,
    /// 销售变更单（仅 `SalesChange` 目的必填，与目的必须一致）。
    pub sales_change_order_id: Option<SalesChangeOrderId>,
    /// 已生效单变更时的基准版本；首次创建为空。
    pub base_revision_id: Option<SalesOrderRevisionId>,
    /// 初始草稿版本（从 1 递增）。
    pub draft_version: u32,
    /// 完整内容指纹。
    pub content_hash: String,
    /// 当前草稿责任人。
    pub editor_user_id: String,
    /// 业务性质（与销售单一致；校验行类型与卡券恰好一行断言）。
    pub business_type: BusinessType,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 合同稳定身份。
    pub contract_id: Option<ContractId>,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 表头结构化快照入参（客户/合同/结算/付款/开票）。
    pub snapshot: super::snapshot::HeaderSnapshotData,
    /// 客户项目名称。
    pub project_name: Option<String>,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU（卡券单必填，非卡券单为空）。
    pub voucher_category_sku_id: Option<SkuId>,
    /// 卡券履约期限（卡券单必填，非卡券单为空）。
    pub voucher_expiry_at: Option<Instant>,
    /// 草稿行汇总（含税）。
    pub gross_amount: Amount,
    /// 草稿行汇总（不含税）。
    pub net_amount: Amount,
    /// 草稿行汇总（税额）。
    pub tax_amount: Amount,
    /// 行清单（列表去重与跨行断言在 `new` 内完成）。
    pub lines: Vec<SalesOrderWorkingCopyLineData>,
}

/// 工作副本更新数据（草稿编辑保存时整表头覆盖）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesOrderWorkingCopyUpdate {
    /// 完整内容指纹；`None` 表示不修改。
    pub content_hash: Option<String>,
    /// 客户稳定身份；`None` 表示不修改。
    pub customer_id: Option<CustomerAccountId>,
    /// 合同稳定身份；`None` 表示不修改。
    pub contract_id: Option<ContractId>,
    /// 结算主体；`None` 表示不修改。
    pub settlement_party_id: Option<PartyId>,
    /// 表头结构化快照入参；`None` 表示不修改。
    pub snapshot: Option<super::snapshot::HeaderSnapshotData>,
    /// 项目名称；`None` 表示不修改。
    pub project_name: Option<String>,
    /// 业务备注；`None` 表示不修改。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU；`None` 表示不修改。
    pub voucher_category_sku_id: Option<SkuId>,
    /// 卡券履约期限；`None` 表示不修改。
    pub voucher_expiry_at: Option<Instant>,
    /// 草稿行汇总（含税）；`None` 表示不修改。
    pub gross_amount: Option<Amount>,
    /// 草稿行汇总（不含税）；`None` 表示不修改。
    pub net_amount: Option<Amount>,
    /// 草稿行汇总（税额）；`None` 表示不修改。
    pub tax_amount: Option<Amount>,
}

/// 工作副本实体（可编辑草稿，数据模型 §6.5）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesOrderWorkingCopy {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<WorkingCopyStatus>,
    /// 稳定销售单。
    pub sales_order_id: SalesOrderId,
    /// 编辑目的。
    pub working_purpose: WorkingPurpose,
    /// 销售变更单。
    pub sales_change_order_id: Option<SalesChangeOrderId>,
    /// 基准版本（首次创建为空）。
    pub base_revision_id: Option<SalesOrderRevisionId>,
    /// 草稿版本（每次服务端保存递增）。
    pub draft_version: u32,
    /// 完整内容指纹。
    pub content_hash: String,
    /// 当前草稿责任人。
    pub editor_user_id: String,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 合同稳定身份。
    pub contract_id: Option<ContractId>,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 客户名称快照。
    pub customer_snapshot: super::snapshot::CustomerSnapshot,
    /// 合同编号快照。
    pub contract_snapshot: Option<super::snapshot::ContractSnapshot>,
    /// 结算主体名称快照。
    pub settlement_party_snapshot: Option<super::snapshot::SettlementPartySnapshot>,
    /// 结构化付款条件快照。
    pub payment_term_snapshot: super::snapshot::PaymentTermSnapshot,
    /// 结构化开票要求快照。
    pub invoice_requirement_snapshot: super::snapshot::InvoiceRequirementSnapshot,
    /// 客户项目名称。
    pub project_name: Option<String>,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU。
    pub voucher_category_sku_id: Option<SkuId>,
    /// 卡券履约期限。
    pub voucher_expiry_at: Option<Instant>,
    /// 草稿行汇总（含税）。
    pub gross_amount: Amount,
    /// 草稿行汇总（不含税）。
    pub net_amount: Amount,
    /// 草稿行汇总（税额）。
    pub tax_amount: Amount,
}

impl PartialEq for SalesOrderWorkingCopy {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_order_id == other.sales_order_id
            && self.working_purpose == other.working_purpose
            && self.sales_change_order_id == other.sales_change_order_id
            && self.base_revision_id == other.base_revision_id
            && self.draft_version == other.draft_version
            && self.content_hash == other.content_hash
            && self.editor_user_id == other.editor_user_id
            && self.business_type == other.business_type
            && self.customer_id == other.customer_id
            && self.contract_id == other.contract_id
            && self.settlement_party_id == other.settlement_party_id
            && self.customer_snapshot == other.customer_snapshot
            && self.contract_snapshot == other.contract_snapshot
            && self.settlement_party_snapshot == other.settlement_party_snapshot
            && self.payment_term_snapshot == other.payment_term_snapshot
            && self.invoice_requirement_snapshot == other.invoice_requirement_snapshot
            && self.project_name == other.project_name
            && self.business_remark == other.business_remark
            && self.voucher_category_sku_id == other.voucher_category_sku_id
            && self.voucher_expiry_at == other.voucher_expiry_at
            && self.gross_amount == other.gross_amount
            && self.net_amount == other.net_amount
            && self.tax_amount == other.tax_amount
    }
}

impl Eq for SalesOrderWorkingCopy {}

impl SalesOrderWorkingCopy {
    /// 创建工作副本。
    ///
    /// 完成全部表头文本字段的校验与规范化（trim、非空、长度上限），并强制两条
    /// 关联一致性不变式：
    /// - `sales_change_order_id` 与编辑目的必须匹配（变更目的必填、首次提交必空）；
    /// - 卡券类目与履约期限必须同时提供或同时省略（§6.4 卡券单必填规则）；
    /// - `gross = net + tax` 精确成立；
    /// - 行清单按 [`validate_line_list`] 去重并断言行类型与业务性质一致。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderWorkingCopyId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人
    ///
    /// # 返回
    /// 返回新建的工作副本实体（`Editing`）。
    ///
    /// # 错误
    /// 必填为空、超长、关联不一致、金额三元组不成立或行清单非法时返回错误。
    pub fn new(
        id: SalesOrderWorkingCopyId,
        data: SalesOrderWorkingCopyData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        if data.draft_version == 0 {
            return Err(Error::from("草稿版本必须为正整数"));
        }
        let content_hash = normalize_required_text(
            data.content_hash,
            "内容指纹不能为空",
            CONTENT_HASH_MAX_LEN,
            "内容指纹过长",
        )?;
        let editor_user_id = normalize_required_text(
            data.editor_user_id,
            "编辑人不能为空",
            EDITOR_MAX_LEN,
            "编辑人过长",
        )?;
        let snapshots = HeaderSnapshots::build(&data.snapshot)?;
        let project_name = normalize_optional_text(data.project_name, "项目名称", PROJECT_NAME_MAX_LEN)?;
        let business_remark =
            normalize_optional_text(data.business_remark, "业务备注", BUSINESS_REMARK_MAX_LEN)?;
        Self::validate_associations(
            data.working_purpose,
            data.sales_change_order_id.clone(),
            data.voucher_category_sku_id.clone(),
            data.voucher_expiry_at,
        )?;
        validate_amount_triple(data.gross_amount, data.net_amount, data.tax_amount)?;
        let lines = data
            .lines
            .iter()
            .map(|line| LineSummary {
                line_no: line.line_no,
                line_id: line.sales_order_line_id.clone(),
                line_type: line.line_type,
            })
            .collect::<Vec<_>>();
        validate_line_list(data.business_type, &lines)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(WorkingCopyStatus::Editing, created_by),
            sales_order_id: data.sales_order_id,
            working_purpose: data.working_purpose,
            sales_change_order_id: data.sales_change_order_id,
            base_revision_id: data.base_revision_id,
            draft_version: data.draft_version,
            content_hash,
            editor_user_id,
            business_type: data.business_type,
            customer_id: data.customer_id,
            contract_id: data.contract_id,
            settlement_party_id: data.settlement_party_id,
            customer_snapshot: snapshots.customer_snapshot,
            contract_snapshot: snapshots.contract_snapshot,
            settlement_party_snapshot: snapshots.settlement_party_snapshot,
            payment_term_snapshot: snapshots.payment_term_snapshot,
            invoice_requirement_snapshot: snapshots.invoice_requirement_snapshot,
            project_name,
            business_remark,
            voucher_category_sku_id: data.voucher_category_sku_id,
            voucher_expiry_at: data.voucher_expiry_at,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
        })
    }

    /// 更新草稿表头（仅 `Editing` 状态允许）。
    ///
    /// 复用 `new` 的规范化与关联一致性校验；`sales_order_id`/`working_purpose`/
    /// `sales_change_order_id`/`base_revision_id`/`business_type` 是身份与基准字段，
    /// 不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非 `Editing`、必填为空、超长或金额三元组不成立时返回错误。
    pub fn update(
        &mut self,
        update: SalesOrderWorkingCopyUpdate,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        ensure_transition(self.stable.status, WorkingCopyStatus::Editing)?;
        if let Some(content_hash) = update.content_hash {
            self.content_hash = normalize_required_text(
                content_hash,
                "内容指纹不能为空",
                CONTENT_HASH_MAX_LEN,
                "内容指纹过长",
            )?;
        }
        if let Some(customer_id) = update.customer_id {
            self.customer_id = customer_id;
        }
        if let Some(contract_id) = update.contract_id {
            self.contract_id = Some(contract_id);
        }
        if let Some(settlement_party_id) = update.settlement_party_id {
            self.settlement_party_id = settlement_party_id;
        }
        if let Some(snapshot) = update.snapshot {
            let snapshots = HeaderSnapshots::build(&snapshot)?;
            self.customer_snapshot = snapshots.customer_snapshot;
            self.contract_snapshot = snapshots.contract_snapshot;
            self.settlement_party_snapshot = snapshots.settlement_party_snapshot;
            self.payment_term_snapshot = snapshots.payment_term_snapshot;
            self.invoice_requirement_snapshot = snapshots.invoice_requirement_snapshot;
        }
        if let Some(project_name) = update.project_name {
            self.project_name =
                normalize_optional_text(Some(project_name), "项目名称", PROJECT_NAME_MAX_LEN)?;
        }
        if let Some(business_remark) = update.business_remark {
            self.business_remark =
                normalize_optional_text(Some(business_remark), "业务备注", BUSINESS_REMARK_MAX_LEN)?;
        }
        if let Some(voucher_category_sku_id) = update.voucher_category_sku_id {
            self.voucher_category_sku_id = Some(voucher_category_sku_id);
        }
        if let Some(voucher_expiry_at) = update.voucher_expiry_at {
            self.voucher_expiry_at = Some(voucher_expiry_at);
        }
        if let Some(gross_amount) = update.gross_amount {
            self.gross_amount = gross_amount;
        }
        if let Some(net_amount) = update.net_amount {
            self.net_amount = net_amount;
        }
        if let Some(tax_amount) = update.tax_amount {
            self.tax_amount = tax_amount;
        }
        Self::validate_associations(
            self.working_purpose,
            self.sales_change_order_id.clone(),
            self.voucher_category_sku_id.clone(),
            self.voucher_expiry_at,
        )?;
        validate_amount_triple(self.gross_amount, self.net_amount, self.tax_amount)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 保存草稿：递增 `draft_version` 并更新内容指纹（自动保存条件更新基础）。
    ///
    /// # 参数
    /// * `content_hash` - 新内容指纹
    /// * `editor_user_id` - 责任人
    ///
    /// # 返回
    /// 保存成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非 `Editing`，或指纹/编辑人为空、超长时返回错误。
    pub fn save_draft(
        &mut self,
        content_hash: impl Into<String>,
        editor_user_id: impl Into<String>,
    ) -> Result<()> {
        ensure_transition(self.stable.status, WorkingCopyStatus::Editing)?;
        self.content_hash = normalize_required_text(
            content_hash.into(),
            "内容指纹不能为空",
            CONTENT_HASH_MAX_LEN,
            "内容指纹过长",
        )?;
        self.editor_user_id = normalize_required_text(
            editor_user_id.into(),
            "编辑人不能为空",
            EDITOR_MAX_LEN,
            "编辑人过长",
        )?;
        self.draft_version += 1;
        self.stable.touch(self.editor_user_id.clone());
        Ok(())
    }

    /// 提交草稿（`Editing → Submitted`，提交事务锁定草稿后标记）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非编辑中状态时返回 [`Error::InvalidStateTransition`]。
    pub fn submit(&mut self) -> Result<()> {
        ensure_transition(self.stable.status, WorkingCopyStatus::Submitted)?;
        self.stable.status = WorkingCopyStatus::Submitted;
        Ok(())
    }

    /// 放弃草稿（`Editing → Abandoned`）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非编辑中状态时返回 [`Error::InvalidStateTransition`]。
    pub fn abandon(&mut self) -> Result<()> {
        ensure_transition(self.stable.status, WorkingCopyStatus::Abandoned)?;
        self.stable.status = WorkingCopyStatus::Abandoned;
        Ok(())
    }

    /// 标记冲突（`Editing → Conflict`；基础资料变化不静默改写已保存草稿，§6.5）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非编辑中状态时返回 [`Error::InvalidStateTransition`]。
    pub fn mark_conflict(&mut self) -> Result<()> {
        ensure_transition(self.stable.status, WorkingCopyStatus::Conflict)?;
        self.stable.status = WorkingCopyStatus::Conflict;
        Ok(())
    }

    /// 解决冲突回到编辑中（`Conflict → Editing`）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非冲突状态时返回 [`Error::InvalidStateTransition`]。
    pub fn resolve_conflict(&mut self) -> Result<()> {
        ensure_transition(self.stable.status, WorkingCopyStatus::Editing)?;
        self.stable.status = WorkingCopyStatus::Editing;
        Ok(())
    }

    /// 校验编辑目的、卡券表头字段的关联一致性。
    ///
    /// # 参数
    /// * `working_purpose` - 编辑目的
    /// * `sales_change_order_id` - 销售变更单
    /// * `voucher_category_sku_id` - 卡券类目 SKU
    /// * `voucher_expiry_at` - 卡券履约期限
    ///
    /// # 返回
    /// 一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 变更目的缺变更单、首次提交携带变更单，或卡券类目与履约期限不同时出现时
    /// 返回错误。
    fn validate_associations(
        working_purpose: WorkingPurpose,
        sales_change_order_id: Option<SalesChangeOrderId>,
        voucher_category_sku_id: Option<SkuId>,
        voucher_expiry_at: Option<Instant>,
    ) -> Result<()> {
        let is_change = matches!(working_purpose, WorkingPurpose::SalesChange);
        if sales_change_order_id.is_some() != is_change {
            return Err(Error::from(
                "销售变更编辑目的必须关联销售变更单，首次提交不得关联",
            ));
        }
        if voucher_category_sku_id.is_some() != voucher_expiry_at.is_some() {
            return Err(Error::from("卡券类目与卡券履约期限必须同时提供或同时省略"));
        }
        Ok(())
    }
}

/// 工作副本行创建数据（行字段组按 `line_type` 二选一）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderWorkingCopyLineData {
    /// 稳定明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 销售项名称快照。
    pub item_name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 单位快照。
    pub unit_snapshot: Option<String>,
    /// 实物及服务字段组。
    pub goods: Option<GoodsLineFields>,
    /// 卡券字段组。
    pub voucher: Option<VoucherLineDraft>,
}

/// 工作副本行实体（数据模型 §6.5：`(working_copy_id, sales_order_line_id)` 唯一）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderWorkingCopyLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属工作副本。
    pub working_copy_id: SalesOrderWorkingCopyId,
    /// 稳定明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 行含税金额。
    pub gross_amount: Amount,
    /// 行不含税金额。
    pub net_amount: Amount,
    /// 行税额。
    pub tax_amount: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 销售项名称快照。
    pub item_name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 单位快照。
    pub unit_snapshot: Option<String>,
    /// 正式销售项 SKU。
    pub sku_id: Option<SkuId>,
    /// 精确 SKU 修订。
    pub sku_revision_id: Option<crate::ids::SkuRevisionId>,
    /// 福利场景。
    pub welfare_scenario: Option<WelfareScenario>,
    /// 履约方式。
    pub fulfillment_mode: Option<FulfillmentMode>,
    /// 本明细履约期限。
    pub fulfillment_due_at: Option<Instant>,
    /// 基础单位数量。
    pub quantity: Option<Quantity>,
    /// 基础单位代码。
    pub base_unit_code: Option<String>,
    /// 含税成交单价快照。
    pub unit_price_gross: Option<UnitPrice>,
    /// 单卡面额。
    pub face_value: Option<Amount>,
    /// 卡张数。
    pub card_count: Option<u32>,
    /// 面额小计。
    pub face_value_total: Option<Amount>,
    /// 最终成交金额。
    pub transaction_amount: Option<Amount>,
    /// 配赠金额。
    pub gift_amount: Option<Amount>,
    /// 配赠率。
    pub gift_rate: Option<Rate>,
    /// 卡形态。
    pub card_form: Option<super::types::CardForm>,
}

impl SalesOrderWorkingCopyLine {
    /// 创建工作副本行。
    ///
    /// 完成文本字段校验与规范化，行金额三元组按
    /// [`crate::money::line_amounts`] 统一计算（§4.2 逐行舍入）；卡券行按 §6.4
    /// 校验面额小计、成交金额与配赠金额一致性。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderWorkingCopyLineId`）
    /// * `working_copy_id` - 所属工作副本
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的工作副本行。
    ///
    /// # 错误
    /// 行号为零、必填为空、超长、行类型与字段组不一致或卡券金额不一致时返回错误。
    pub fn new(
        id: SalesOrderWorkingCopyLineId,
        working_copy_id: SalesOrderWorkingCopyId,
        data: SalesOrderWorkingCopyLineData,
    ) -> Result<Self> {
        if data.line_no == 0 {
            return Err(Error::from("行号必须为正整数"));
        }
        let item_name_snapshot = normalize_required_text(
            data.item_name_snapshot,
            "销售项名称不能为空",
            ITEM_NAME_MAX_LEN,
            "销售项名称过长",
        )?;
        let spec_snapshot = normalize_optional_text(data.spec_snapshot, "规格", SPEC_MAX_LEN)?;
        let unit_snapshot = normalize_optional_text(data.unit_snapshot, "单位", UNIT_MAX_LEN)?;
        let built = build_line_groups(data.line_type, data.goods, data.voucher, data.sales_tax_rate)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            working_copy_id,
            sales_order_line_id: data.sales_order_line_id,
            line_no: data.line_no,
            line_type: data.line_type,
            gross_amount: built.gross_amount,
            net_amount: built.net_amount,
            tax_amount: built.tax_amount,
            sales_tax_rate: data.sales_tax_rate,
            item_name_snapshot,
            spec_snapshot,
            unit_snapshot,
            sku_id: built.goods.as_ref().map(|g| g.sku_id.clone()),
            sku_revision_id: built.goods.as_ref().map(|g| g.sku_revision_id.clone()),
            welfare_scenario: built.goods.as_ref().and_then(|g| g.welfare_scenario),
            fulfillment_mode: built.goods.as_ref().map(|g| g.fulfillment_mode),
            fulfillment_due_at: built.goods.as_ref().map(|g| g.fulfillment_due_at),
            quantity: built.goods.as_ref().map(|g| g.quantity),
            base_unit_code: built.goods.as_ref().map(|g| g.base_unit_code.clone()),
            unit_price_gross: built
                .goods
                .as_ref()
                .map(|g| g.unit_price_gross)
                .or_else(|| built.voucher.as_ref().map(|v| v.unit_price_gross)),
            face_value: built.voucher.as_ref().map(|v| v.face_value),
            card_count: built.voucher.as_ref().map(|v| v.card_count),
            face_value_total: built.voucher.as_ref().map(|v| v.face_value_total),
            transaction_amount: built.voucher.as_ref().map(|v| v.transaction_amount),
            gift_amount: built.voucher.as_ref().map(|v| v.gift_amount),
            gift_rate: built.voucher.as_ref().map(|v| v.gift_rate),
            card_form: built.voucher.as_ref().map(|v| v.card_form),
        })
    }
}

/// 校验表头金额三元组恒等式（§4.2 规则 2：表头只汇总已舍入的行金额）。
///
/// # 参数
/// * `gross_amount` - 含税合计
/// * `net_amount` - 不含税合计
/// * `tax_amount` - 税额合计
///
/// # 返回
/// 恒等式成立时返回 `Ok(())`。
///
/// # 错误
/// `gross != net + tax` 时返回错误。
pub(crate) fn validate_amount_triple(
    gross_amount: Amount,
    net_amount: Amount,
    tax_amount: Amount,
) -> Result<()> {
    if gross_amount.to_decimal() != net_amount.to_decimal() + tax_amount.to_decimal() {
        return Err(Error::from("表头金额必须满足 gross = net + tax"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::ids::{SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId, SkuRevisionId};
    use crate::money::Quantity;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn goods_line() -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            fulfillment_mode: FulfillmentMode::CompanyWarehouse,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    fn line_data(line_no: u32) -> SalesOrderWorkingCopyLineData {
        SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: " 年货礼盒 ".to_string(),
            spec_snapshot: Some(" 10kg ".to_string()),
            unit_snapshot: Some(" 箱 ".to_string()),
            goods: Some(goods_line()),
            voucher: None,
        }
    }

    fn header_data() -> SalesOrderWorkingCopyData {
        SalesOrderWorkingCopyData {
            sales_order_id: SalesOrderId::new("o-1"),
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: None,
            base_revision_id: None,
            draft_version: 1,
            content_hash: " abc123def456 ".to_string(),
            editor_user_id: " user-1 ".to_string(),
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: Some(ContractId::new("contract-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: super::super::snapshot::HeaderSnapshotData {
                customer_name: " 东方企业 ".to_string(),
                contract_no: Some(" HT-2026-0088 ".to_string()),
                settlement_party_name: Some(" 集团结算中心 ".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: " 月结 30 天 ".to_string(),
                invoice_type: " 增值税专用发票 ".to_string(),
                tax_point: " 6 ".to_string(),
            },
            project_name: Some(" 端午福利项目 ".to_string()),
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            lines: vec![line_data(1)],
        }
    }

    #[test]
    fn new_trims_and_normalizes() {
        let copy = SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), header_data(), "admin-1")
            .unwrap();

        assert_eq!(copy.content_hash, "abc123def456");
        assert_eq!(copy.editor_user_id, "user-1");
        assert_eq!(copy.customer_snapshot.customer_name, "东方企业");
        assert_eq!(copy.contract_snapshot.unwrap().contract_no, "HT-2026-0088");
        assert_eq!(copy.payment_term_snapshot.payment_term_name, "月结 30 天");
        assert_eq!(copy.project_name.as_deref(), Some("端午福利项目"));
        assert_eq!(copy.stable.status(), WorkingCopyStatus::Editing);
        assert_eq!(copy.draft_version, 1);
    }

    #[test]
    fn new_rejects_blank_and_overlong_fields() {
        let blank_hash = SalesOrderWorkingCopyData {
            content_hash: "   ".to_string(),
            ..header_data()
        };
        assert!(
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), blank_hash, "admin-1").is_err()
        );

        let overlong_editor = SalesOrderWorkingCopyData {
            editor_user_id: "x".repeat(129),
            ..header_data()
        };
        assert!(
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), overlong_editor, "admin-1")
                .is_err()
        );

        let empty_lines = SalesOrderWorkingCopyData {
            lines: vec![],
            ..header_data()
        };
        assert!(
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), empty_lines, "admin-1").is_err()
        );
    }

    #[test]
    fn new_rejects_broken_associations_and_amounts() {
        // 变更目的必须带变更单
        let change_without_order = SalesOrderWorkingCopyData {
            working_purpose: WorkingPurpose::SalesChange,
            sales_change_order_id: None,
            base_revision_id: Some(SalesOrderRevisionId::new("rev-1")),
            ..header_data()
        };
        assert!(SalesOrderWorkingCopy::new(
            SalesOrderWorkingCopyId::new("wc-1"),
            change_without_order,
            "admin-1"
        )
        .is_err());

        // 首次提交不得带变更单
        let first_with_order = SalesOrderWorkingCopyData {
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: Some(SalesChangeOrderId::new("co-1")),
            ..header_data()
        };
        assert!(SalesOrderWorkingCopy::new(
            SalesOrderWorkingCopyId::new("wc-1"),
            first_with_order,
            "admin-1"
        )
        .is_err());

        // 卡券类目与履约期限必须成对
        let half_voucher = SalesOrderWorkingCopyData {
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: None,
            ..header_data()
        };
        assert!(
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), half_voucher, "admin-1")
                .is_err()
        );

        // 金额三元组不一致
        let broken_amount = SalesOrderWorkingCopyData {
            tax_amount: amt("3.91"),
            ..header_data()
        };
        assert!(
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), broken_amount, "admin-1")
                .is_err()
        );
    }

    #[test]
    fn new_rejects_duplicate_lines_in_list() {
        let duplicated = SalesOrderWorkingCopyData {
            lines: vec![line_data(1), line_data(1)],
            ..header_data()
        };
        assert!(
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), duplicated, "admin-1").is_err()
        );
    }

    #[test]
    fn update_applies_fields_and_keeps_identity() {
        let mut copy =
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), header_data(), "admin-1")
                .unwrap();
        copy.update(
            SalesOrderWorkingCopyUpdate {
                project_name: Some(" 中秋福利项目 ".to_string()),
                gross_amount: Some(amt("59.94")),
                net_amount: Some(amt("52.14")),
                tax_amount: Some(amt("7.80")),
                ..Default::default()
            },
            "admin-2",
        )
        .unwrap();

        assert_eq!(copy.project_name.as_deref(), Some("中秋福利项目"));
        assert_eq!(copy.sales_order_id, SalesOrderId::new("o-1"));
        assert_eq!(copy.working_purpose, WorkingPurpose::FirstSubmission);
        assert_eq!(copy.stable.updated_by, "admin-2");
    }

    #[test]
    fn update_rejects_when_submitted_and_broken_amount() {
        let mut copy =
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), header_data(), "admin-1")
                .unwrap();
        copy.submit().unwrap();
        assert!(copy
            .update(SalesOrderWorkingCopyUpdate::default(), "admin-2")
            .is_err());

        let mut another =
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-2"), header_data(), "admin-1")
                .unwrap();
        assert!(another
            .update(
                SalesOrderWorkingCopyUpdate {
                    tax_amount: Some(amt("9.99")),
                    ..Default::default()
                },
                "admin-2",
            )
            .is_err());
    }

    #[test]
    fn status_machine_allows_legal_and_rejects_illegal_transitions() {
        let mut copy =
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), header_data(), "admin-1")
                .unwrap();

        copy.mark_conflict().unwrap();
        assert_eq!(copy.stable.status(), WorkingCopyStatus::Conflict);
        copy.resolve_conflict().unwrap();
        assert_eq!(copy.stable.status(), WorkingCopyStatus::Editing);
        copy.abandon().unwrap();
        assert_eq!(copy.stable.status(), WorkingCopyStatus::Abandoned);
        assert!(copy.submit().is_err(), "已放弃不可提交");

        let mut submitted =
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-2"), header_data(), "admin-1")
                .unwrap();
        submitted.submit().unwrap();
        assert!(submitted.abandon().is_err(), "已提交为终态");
        assert!(WorkingCopyStatus::Submitted.allowed_next().is_empty());
        assert!(ensure_transition(WorkingCopyStatus::Abandoned, WorkingCopyStatus::Editing).is_err());
    }

    #[test]
    fn save_draft_increments_version_and_updates_hash() {
        let mut copy =
            SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), header_data(), "admin-1")
                .unwrap();
        copy.save_draft(" new-hash ".to_string(), " user-1 ".to_string())
            .unwrap();

        assert_eq!(copy.draft_version, 2);
        assert_eq!(copy.content_hash, "new-hash");
        assert_eq!(copy.editor_user_id, "user-1");
        assert_eq!(copy.stable.updated_by, "user-1");

        copy.submit().unwrap();
        assert!(copy.save_draft("x".to_string(), "user-1").is_err());
    }

    #[test]
    fn line_new_computes_amounts_and_normalizes() {
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(1),
        )
        .unwrap();

        assert_eq!(line.item_name_snapshot, "年货礼盒");
        assert_eq!(line.gross_amount, amt("29.97"));
        assert_eq!(line.net_amount, amt("26.07"));
        assert_eq!(line.tax_amount, amt("3.90"));
        assert_eq!(
            line.gross_amount.to_decimal(),
            line.net_amount.to_decimal() + line.tax_amount.to_decimal(),
            "gross = net + tax 逐行成立"
        );
        assert_eq!(line.base_unit_code.as_deref(), Some("箱"));
        assert_eq!(line.unit_price_gross, Some(price("9.9900")));
        assert!(line.face_value.is_none());
        assert!(line.card_count.is_none());
    }

    #[test]
    fn line_new_rejects_zero_no_mismatch_and_inconsistent_voucher() {
        let zero = SalesOrderWorkingCopyLineData {
            line_no: 0,
            ..line_data(1)
        };
        assert!(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            zero
        )
        .is_err());

        let mismatch = SalesOrderWorkingCopyLineData {
            line_type: LineType::Voucher,
            goods: None,
            voucher: None,
            ..line_data(1)
        };
        assert!(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            mismatch
        )
        .is_err());

        let blank_item = SalesOrderWorkingCopyLineData {
            item_name_snapshot: "   ".to_string(),
            ..line_data(1)
        };
        assert!(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            blank_item
        )
        .is_err());
    }

    #[test]
    fn voucher_line_builds_with_derived_gift_rate() {
        let voucher = VoucherLineDraft {
            face_value: amt("100.00"),
            card_count: 3,
            unit_price_gross: price("90.0000"),
            face_value_total: amt("300.00"),
            transaction_amount: amt("270.00"),
            gift_amount: amt("30.00"),
            gift_rate: None,
            card_form: super::super::types::CardForm::Electronic,
        };
        let data = SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            line_no: 1,
            line_type: LineType::Voucher,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: "福利卡".to_string(),
            spec_snapshot: None,
            unit_snapshot: Some("张".to_string()),
            goods: None,
            voucher: Some(voucher),
        };
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            data,
        )
        .unwrap();

        assert_eq!(line.transaction_amount, Some(amt("270.00")));
        assert_eq!(line.face_value_total, Some(amt("300.00")));
        assert_eq!(line.gift_amount, Some(amt("30.00")));
        assert_eq!(line.gift_rate.unwrap().to_decimal().to_string(), "0.111111");
        assert_eq!(line.gross_amount, amt("270.00"), "公共行金额等于成交金额");
    }
}
