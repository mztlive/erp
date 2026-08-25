//! `sales_change_submission` 与 `sales_change_submission_line`（数据模型 §6.5）。
//!
//! 变更提交保存拟变更后的**完整目标头和行**，字段与 `sales_order_submission` 相同，
//! 并增加 `sales_change_order_id`、`submission_no`、`base_revision_id`。草稿自动
//! 保存仍使用 `sales_order_working_copy`；发起影响确认时才形成不可变变更提交。
//! `(sales_change_order_id, submission_no)` 唯一，提交头行形成后不可更新。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    ContractRevisionId, CustomerAccountId, PartyId, SalesChangeOrderId, SalesChangeSubmissionId,
    SalesChangeSubmissionLineId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
    SalesOrderWorkingCopyId, SkuId,
};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::snapshot::HeaderSnapshots;
use super::types::{
    build_line_groups, validate_line_list, BusinessType, GoodsLineFields, LineSummary, LineType,
    VoucherLineDraft, WelfareScenario,
};

/// 提交人标识最大长度。
const SUBMITTER_MAX_LEN: usize = 128;
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

/// 变更提交状态（与销售提交同构：审核中、已通过、已驳回、因重新提交失效）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubmissionStatus {
    /// 审核中。
    InReview,
    /// 已通过。
    Approved,
    /// 已驳回。
    Rejected,
    /// 因重新提交失效。
    Superseded,
}

impl SubmissionStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::InReview => "审核中",
            Self::Approved => "已通过",
            Self::Rejected => "已驳回",
            Self::Superseded => "因重新提交失效",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InReview => "IN_REVIEW",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

impl DocumentState for SubmissionStatus {
    /// 审核中可被通过、驳回或因新提交失效；其余为终态（§6.5）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::InReview => &[Self::Approved, Self::Rejected, Self::Superseded],
            Self::Approved | Self::Rejected | Self::Superseded => &[],
        }
    }
}

/// 变更提交创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeSubmissionData {
    /// 所属销售变更单。
    pub sales_change_order_id: SalesChangeOrderId,
    /// 提交序号（每次修改拟变更内容都形成新的提交）。
    pub submission_no: u32,
    /// 基准版本（发起时当前版本）。
    pub base_revision_id: SalesOrderRevisionId,
    /// 原销售单。
    pub sales_order_id: SalesOrderId,
    /// 形成提交的草稿。
    pub working_copy_id: SalesOrderWorkingCopyId,
    /// 形成提交的草稿服务端确认版本。
    pub working_copy_version: u32,
    /// 业务性质（与销售单一致）。
    pub business_type: BusinessType,
    /// 目标客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 目标合同版本。
    pub contract_revision_id: Option<ContractRevisionId>,
    /// 目标结算主体。
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
    /// 目标行汇总（含税）。
    pub gross_amount: Amount,
    /// 目标行汇总（不含税）。
    pub net_amount: Amount,
    /// 目标行汇总（税额）。
    pub tax_amount: Amount,
    /// 提交审计时间。
    pub submitted_at: Instant,
    /// 提交审计人。
    pub submitted_by: String,
    /// 行清单（列表去重与跨行断言在 `new` 内完成）。
    pub lines: Vec<SalesChangeSubmissionLineData>,
}

impl SalesChangeSubmissionData {
    /// 从销售变更工作副本构建不可变变更提交数据。
    ///
    /// # 参数
    /// * `change_order` - 当前销售变更单
    /// * `working_copy` - 该变更单冻结的销售工作副本
    /// * `lines` - 工作副本的全部冻结明细
    /// * `submission_no` - 本次严格递增的提交序号
    /// * `submitted_at` - 提交时间
    /// * `submitted_by` - 提交人
    ///
    /// # 返回
    /// 返回完成 D13 → D14 类型映射、快照复制与金额汇总后的提交数据。
    ///
    /// # 错误
    /// 工作副本与变更单关系不一致、明细不属于工作副本或字段组非法时返回错误。
    pub fn from_sales_working_copy(
        change_order: &super::SalesChangeOrder,
        working_copy: &crate::sales_order::SalesOrderWorkingCopy,
        lines: &[crate::sales_order::SalesOrderWorkingCopyLine],
        submission_no: u32,
        submitted_at: Instant,
        submitted_by: impl Into<String>,
    ) -> Result<Self> {
        let change_order_id = SalesChangeOrderId::new(change_order.base.id.clone());
        if working_copy.working_purpose != crate::sales_order::WorkingPurpose::SalesChange
            || working_copy.sales_change_order_id.as_ref() != Some(&change_order_id)
            || working_copy.sales_order_id != change_order.sales_order_id
            || working_copy.base_revision_id.as_ref() != Some(&change_order.base_revision_id)
        {
            return Err(Error::from("变更工作副本与销售变更单关系不一致"));
        }
        let working_copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
        if lines
            .iter()
            .any(|line| !line.belongs_to_working_copy(&working_copy_id))
        {
            return Err(Error::from("变更工作副本明细归属不一致"));
        }
        let line_data = lines
            .iter()
            .map(SalesChangeSubmissionLineData::from_sales_working_copy)
            .collect::<Result<Vec<_>>>()?;
        let (gross_amount, net_amount, tax_amount) =
            crate::sales_order::SalesOrderWorkingCopyLine::amount_totals(lines);
        Ok(Self {
            sales_change_order_id: change_order_id,
            submission_no,
            base_revision_id: change_order.base_revision_id.clone(),
            sales_order_id: change_order.sales_order_id.clone(),
            working_copy_id,
            working_copy_version: working_copy.draft_version,
            business_type: working_copy.business_type.into(),
            customer_id: working_copy.customer_id.clone(),
            contract_revision_id: working_copy.contract_revision_id.clone(),
            settlement_party_id: working_copy.settlement_party_id.clone(),
            snapshot: super::snapshot::HeaderSnapshotData {
                customer_name: working_copy.customer_snapshot.customer_name.clone(),
                contract_no: working_copy
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: working_copy
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: working_copy.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: working_copy.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: working_copy.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: working_copy.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: working_copy.project_name.clone(),
            business_remark: working_copy.business_remark.clone(),
            voucher_category_sku_id: working_copy.voucher_category_sku_id.clone(),
            voucher_expiry_at: working_copy.voucher_expiry_at,
            gross_amount,
            net_amount,
            tax_amount,
            submitted_at,
            submitted_by: submitted_by.into(),
            lines: line_data,
        })
    }
}

/// 变更提交实体（不可变目标快照，数据模型 §6.5）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesChangeSubmission {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SubmissionStatus>,
    /// 所属销售变更单。
    pub sales_change_order_id: SalesChangeOrderId,
    /// 提交序号。
    pub submission_no: u32,
    /// 基准版本。
    pub base_revision_id: SalesOrderRevisionId,
    /// 原销售单。
    pub sales_order_id: SalesOrderId,
    /// 形成提交的草稿。
    pub working_copy_id: SalesOrderWorkingCopyId,
    /// 形成提交的草稿服务端确认版本。
    pub working_copy_version: u32,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 目标客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 目标合同版本。
    pub contract_revision_id: Option<ContractRevisionId>,
    /// 目标结算主体。
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
    /// 目标行汇总（含税）。
    pub gross_amount: Amount,
    /// 目标行汇总（不含税）。
    pub net_amount: Amount,
    /// 目标行汇总（税额）。
    pub tax_amount: Amount,
    /// 提交审计时间。
    pub submitted_at: Instant,
    /// 提交审计人。
    pub submitted_by: String,
}

impl PartialEq for SalesChangeSubmission {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_change_order_id == other.sales_change_order_id
            && self.submission_no == other.submission_no
            && self.base_revision_id == other.base_revision_id
            && self.sales_order_id == other.sales_order_id
            && self.working_copy_id == other.working_copy_id
            && self.working_copy_version == other.working_copy_version
            && self.business_type == other.business_type
            && self.customer_id == other.customer_id
            && self.contract_revision_id == other.contract_revision_id
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
            && self.submitted_at == other.submitted_at
            && self.submitted_by == other.submitted_by
    }
}

impl Eq for SalesChangeSubmission {}

impl SalesChangeSubmission {
    /// 创建变更提交（不可变目标快照）。
    ///
    /// 完成全部文本字段的校验与规范化，并强制不变式：卡券类目与履约期限同时
    /// 提供或同时省略、`gross = net + tax` 精确成立、行清单按 [`validate_line_list`]
    /// 去重并断言行类型与业务性质一致。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesChangeSubmissionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的变更提交（`InReview`）。
    ///
    /// # 错误
    /// 必填为空、超长、关联不一致、金额三元组不成立或行清单非法时返回错误。
    pub fn new(id: SalesChangeSubmissionId, data: SalesChangeSubmissionData) -> Result<Self> {
        if data.submission_no == 0 {
            return Err(Error::from("提交序号必须为正整数"));
        }
        if data.working_copy_version == 0 {
            return Err(Error::from("草稿版本必须为正整数"));
        }
        let submitted_by = normalize_required_text(
            data.submitted_by,
            "提交人不能为空",
            SUBMITTER_MAX_LEN,
            "提交人过长",
        )?;
        let snapshots = HeaderSnapshots::build(&data.snapshot)?;
        let project_name = normalize_optional_text(data.project_name, "项目名称", PROJECT_NAME_MAX_LEN)?;
        let business_remark =
            normalize_optional_text(data.business_remark, "业务备注", BUSINESS_REMARK_MAX_LEN)?;
        if data.voucher_category_sku_id.is_some() != data.voucher_expiry_at.is_some() {
            return Err(Error::from("卡券类目与卡券履约期限必须同时提供或同时省略"));
        }
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
            stable: StableBase::new(SubmissionStatus::InReview, submitted_by.clone()),
            sales_change_order_id: data.sales_change_order_id,
            submission_no: data.submission_no,
            base_revision_id: data.base_revision_id,
            sales_order_id: data.sales_order_id,
            working_copy_id: data.working_copy_id,
            working_copy_version: data.working_copy_version,
            business_type: data.business_type,
            customer_id: data.customer_id,
            contract_revision_id: data.contract_revision_id,
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
            submitted_at: data.submitted_at,
            submitted_by,
        })
    }

    /// 更新变更提交。
    ///
    /// 提交头行形成后不可更新（数据模型 §6.5：每次修改拟变更内容都形成新的变更
    /// 提交并使旧复核失效）；此方法恒拒绝，保留签名以表达不可变性契约。
    ///
    /// # 参数
    /// * `_data` - 更新数据（被拒绝）
    ///
    /// # 返回
    /// 恒返回 `Err`。
    ///
    /// # 错误
    /// 恒返回「变更提交形成后不可更新」错误。
    pub fn update(&mut self, _data: SalesChangeSubmissionData) -> Result<()> {
        Err(Error::from("变更提交形成后不可更新"))
    }

    /// 由当前最大提交序号计算下一次变更提交序号。
    ///
    /// # 参数
    /// * `current_max` - 当前已冻结提交中的最大序号；尚无提交时为 `0`
    ///
    /// # 返回
    /// 返回严格递增的下一提交序号。
    ///
    /// # 错误
    /// 当前序号达到 `u32::MAX` 时返回错误。
    pub fn next_submission_no(current_max: u32) -> Result<u32> {
        current_max
            .checked_add(1)
            .ok_or_else(|| Error::from("变更提交序号溢出"))
    }

    /// 通过变更提交（`InReview → Approved`）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审核中状态时返回 [`Error::InvalidStateTransition`]。
    pub fn approve(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition(SubmissionStatus::Approved, updated_by)
    }

    /// 驳回变更提交（`InReview → Rejected`）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审核中状态时返回 [`Error::InvalidStateTransition`]。
    pub fn reject(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition(SubmissionStatus::Rejected, updated_by)
    }

    /// 标记因新提交失效（`InReview → Superseded`；所有复核必须引用同一个
    /// `sales_change_submission_id`，旧复核随新提交失效，§6.5）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审核中状态时返回 [`Error::InvalidStateTransition`]。
    pub fn mark_superseded(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition(SubmissionStatus::Superseded, updated_by)
    }

    /// 执行一次固定状态机迁移。
    ///
    /// # 参数
    /// * `to` - 目标状态
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 迁移非法时返回 [`Error::InvalidStateTransition`]。
    fn transition(&mut self, to: SubmissionStatus, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, to)?;
        self.stable.status = to;
        self.stable.touch(updated_by);
        Ok(())
    }
}

/// 变更提交行创建数据（行字段组按 `line_type` 二选一）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeSubmissionLineData {
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

impl SalesChangeSubmissionLineData {
    /// 从销售工作副本行映射变更提交行数据。
    ///
    /// # 参数
    /// * `line` - D13 销售工作副本冻结行
    ///
    /// # 返回
    /// 返回 D14 变更提交所需的同形字段组与公共快照。
    ///
    /// # 错误
    /// 行类型与字段组不一致，或任一必填字段缺失时返回错误。
    pub fn from_sales_working_copy(line: &crate::sales_order::SalesOrderWorkingCopyLine) -> Result<Self> {
        let goods = if line.line_type == crate::sales_order::LineType::GoodsService {
            Some(GoodsLineFields {
                sku_id: line
                    .sku_id
                    .clone()
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少商品字段组", line.line_no)))?,
                sku_revision_id: line
                    .sku_revision_id
                    .clone()
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少 SKU 修订", line.line_no)))?,
                welfare_scenario: line.welfare_scenario.map(Into::into),
                service_region: line.service_region.clone(),
                fulfillment_due_at: line
                    .fulfillment_due_at
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少履约期限", line.line_no)))?,
                quantity: line
                    .quantity
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少数量", line.line_no)))?,
                base_unit_code: line
                    .base_unit_code
                    .clone()
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少单位", line.line_no)))?,
                unit_price_gross: line
                    .unit_price_gross
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少含税单价", line.line_no)))?,
            })
        } else {
            None
        };
        let voucher = if line.line_type == crate::sales_order::LineType::Voucher {
            Some(VoucherLineDraft {
                face_value: line
                    .face_value
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少卡券字段组", line.line_no)))?,
                card_count: line
                    .card_count
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少卡张数", line.line_no)))?,
                unit_price_gross: line
                    .unit_price_gross
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少卡券成交单价", line.line_no)))?,
                face_value_total: line
                    .face_value_total
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少面额小计", line.line_no)))?,
                transaction_amount: line
                    .transaction_amount
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少成交金额", line.line_no)))?,
                gift_amount: line
                    .gift_amount
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少配赠金额", line.line_no)))?,
                gift_rate: line.gift_rate,
                card_form: line
                    .card_form
                    .map(Into::into)
                    .ok_or_else(|| Error::from(format!("第 {} 行缺少卡形态", line.line_no)))?,
            })
        } else {
            None
        };
        Ok(Self {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: line.line_type.into(),
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods,
            voucher,
        })
    }
}

/// 变更提交行实体（不可变目标明细，数据模型 §6.5）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesChangeSubmissionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属变更提交。
    pub sales_change_submission_id: SalesChangeSubmissionId,
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
    /// 采购责任解析使用的服务区域。
    pub service_region: Option<String>,
    /// 公司对客户承诺完成本明细交付或服务的最晚时间。
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

impl SalesChangeSubmissionLine {
    /// 创建变更提交行。
    ///
    /// 完成文本字段校验与规范化，行金额三元组按
    /// [`crate::money::line_amounts`] 统一计算（§4.2 逐行舍入）；卡券行按 §6.4
    /// 校验面额小计、成交金额与配赠金额一致性。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesChangeSubmissionLineId`）
    /// * `sales_change_submission_id` - 所属变更提交
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的变更提交行。
    ///
    /// # 错误
    /// 行号为零、必填为空、超长、行类型与字段组不一致或卡券金额不一致时返回错误。
    pub fn new(
        id: SalesChangeSubmissionLineId,
        sales_change_submission_id: SalesChangeSubmissionId,
        data: SalesChangeSubmissionLineData,
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
            sales_change_submission_id,
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
            service_region: built.goods.as_ref().and_then(|g| g.service_region.clone()),
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

/// 校验目标行汇总金额三元组恒等式（§4.2 规则 2）。
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
fn validate_amount_triple(gross_amount: Amount, net_amount: Amount, tax_amount: Amount) -> Result<()> {
    if gross_amount.to_decimal() != net_amount.to_decimal() + tax_amount.to_decimal() {
        return Err(Error::from("表头金额必须满足 gross = net + tax"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::ids::{SalesOrderWorkingCopyId, SkuRevisionId};
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
            service_region: Some("east".to_string()),
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    fn line_data(line_no: u32) -> SalesChangeSubmissionLineData {
        SalesChangeSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: " 年货礼盒 ".to_string(),
            spec_snapshot: None,
            unit_snapshot: None,
            goods: Some(goods_line()),
            voucher: None,
        }
    }

    fn header_data() -> SalesChangeSubmissionData {
        SalesChangeSubmissionData {
            sales_change_order_id: SalesChangeOrderId::new("co-1"),
            submission_no: 1,
            base_revision_id: SalesOrderRevisionId::new("rev-1"),
            sales_order_id: SalesOrderId::new("o-1"),
            working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 5,
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
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
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            submitted_at: Instant::from_unix_secs(1_795_000_000),
            submitted_by: " sales-1 ".to_string(),
            lines: vec![line_data(1)],
        }
    }

    #[test]
    fn new_trims_and_initializes_in_review() {
        let submission =
            SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), header_data()).unwrap();

        assert_eq!(submission.submitted_by, "sales-1");
        assert_eq!(submission.sales_change_order_id, SalesChangeOrderId::new("co-1"));
        assert_eq!(submission.base_revision_id, SalesOrderRevisionId::new("rev-1"));
        assert_eq!(submission.stable.status(), SubmissionStatus::InReview);
        assert_eq!(submission.customer_snapshot.customer_name, "东方企业");
    }

    #[test]
    fn new_rejects_blank_and_broken_invariants() {
        let blank_submitter = SalesChangeSubmissionData {
            submitted_by: "   ".to_string(),
            ..header_data()
        };
        assert!(SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), blank_submitter).is_err());

        let zero_no = SalesChangeSubmissionData {
            submission_no: 0,
            ..header_data()
        };
        assert!(SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), zero_no).is_err());

        let half_voucher = SalesChangeSubmissionData {
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: None,
            ..header_data()
        };
        assert!(SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), half_voucher).is_err());

        let broken_amount = SalesChangeSubmissionData {
            tax_amount: amt("3.91"),
            ..header_data()
        };
        assert!(SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), broken_amount).is_err());

        let duplicated = SalesChangeSubmissionData {
            lines: vec![line_data(1), line_data(1)],
            ..header_data()
        };
        assert!(SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), duplicated).is_err());
    }

    #[test]
    fn change_submission_sequence_and_sales_line_mapping_are_entity_owned() {
        assert_eq!(SalesChangeSubmission::next_submission_no(0).unwrap(), 1);
        assert_eq!(SalesChangeSubmission::next_submission_no(2).unwrap(), 3);
        assert!(SalesChangeSubmission::next_submission_no(u32::MAX).is_err());

        let line = crate::sales_order::SalesOrderWorkingCopyLine {
            base: BaseModel::new("wcl-1".to_string()),
            working_copy_id: crate::ids::SalesOrderWorkingCopyId::new("wc-1"),
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            line_no: 1,
            line_type: crate::sales_order::LineType::GoodsService,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: "商品".to_string(),
            spec_snapshot: None,
            unit_snapshot: Some("件".to_string()),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(crate::ids::SkuRevisionId::new("skurev-1")),
            welfare_scenario: Some(crate::sales_order::WelfareScenario::MealSubsidy),
            service_region: None,
            fulfillment_due_at: Some(Instant::from_unix_secs(1_800_000_000)),
            quantity: Some(qty("3.000000")),
            base_unit_code: Some("件".to_string()),
            unit_price_gross: Some(price("9.9900")),
            face_value: None,
            card_count: None,
            face_value_total: None,
            transaction_amount: None,
            gift_amount: None,
            gift_rate: None,
            card_form: None,
        };
        let mapped = SalesChangeSubmissionLineData::from_sales_working_copy(&line).unwrap();
        assert_eq!(mapped.line_type, LineType::GoodsService);
        let goods = mapped.goods.unwrap();
        assert_eq!(goods.sku_id.as_ref(), "sku-1");
        assert_eq!(goods.welfare_scenario, Some(WelfareScenario::MealSubsidy));
        let change_order = super::super::SalesChangeOrder::new(
            SalesChangeOrderId::new("co-1"),
            super::super::SalesChangeOrderData {
                sales_order_id: SalesOrderId::new("o-1"),
                base_revision_id: SalesOrderRevisionId::new("rev-1"),
                change_type: super::super::SalesChangeType::Quantity,
                reason: "调整数量".to_string(),
            },
            "sales-1",
        )
        .unwrap();
        let working_copy = crate::sales_order::SalesOrderWorkingCopy {
            base: BaseModel::new("wc-1".to_string()),
            stable: StableBase::new(crate::sales_order::WorkingCopyStatus::Editing, "sales-1"),
            sales_order_id: SalesOrderId::new("o-1"),
            working_purpose: crate::sales_order::WorkingPurpose::SalesChange,
            sales_change_order_id: Some(SalesChangeOrderId::new("co-1")),
            base_revision_id: Some(SalesOrderRevisionId::new("rev-1")),
            draft_version: 2,
            content_hash: "hash-1".to_string(),
            editor_user_id: "sales-1".to_string(),
            business_type: crate::sales_order::BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: Some(crate::ids::ContractId::new("contract-1")),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: PartyId::new("party-1"),
            customer_snapshot: crate::sales_order::CustomerSnapshot {
                customer_name: "东方企业".to_string(),
            },
            contract_snapshot: Some(crate::sales_order::ContractSnapshot {
                contract_no: "HT-1".to_string(),
            }),
            settlement_party_snapshot: Some(crate::sales_order::SettlementPartySnapshot {
                settlement_party_name: "结算中心".to_string(),
            }),
            payment_term_snapshot: crate::sales_order::PaymentTermSnapshot {
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结30天".to_string(),
            },
            invoice_requirement_snapshot: crate::sales_order::InvoiceRequirementSnapshot {
                invoice_type: "专票".to_string(),
                tax_point: "6".to_string(),
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            target_mall_id: None,
            receivable_due_date: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
        };
        let data = SalesChangeSubmissionData::from_sales_working_copy(
            &change_order,
            &working_copy,
            std::slice::from_ref(&line),
            3,
            Instant::from_unix_secs(1_800_000_000),
            "sales-1",
        )
        .unwrap();
        assert_eq!(data.submission_no, 3);
        assert_eq!(data.working_copy_version, 2);
        assert_eq!(data.gross_amount, amt("29.97"));

        let mut unrelated_copy = working_copy;
        unrelated_copy.base.id = "wc-2".to_string();
        assert!(SalesChangeSubmissionData::from_sales_working_copy(
            &change_order,
            &unrelated_copy,
            std::slice::from_ref(&line),
            3,
            Instant::from_unix_secs(1_800_000_000),
            "sales-1",
        )
        .is_err());
    }

    #[test]
    fn status_machine_approve_reject_supersede() {
        let mut submission =
            SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), header_data()).unwrap();

        submission.reject("reviewer").unwrap();
        assert_eq!(submission.stable.status(), SubmissionStatus::Rejected);
        assert!(submission.approve("reviewer").is_err());
        assert!(submission.mark_superseded("system").is_err());

        let mut approved =
            SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-2"), header_data()).unwrap();
        approved.approve("reviewer").unwrap();
        assert!(approved.reject("reviewer").is_err());
        assert!(ensure_transition(SubmissionStatus::Superseded, SubmissionStatus::InReview).is_err());
    }

    #[test]
    fn update_is_rejected_for_immutable_submission() {
        let mut submission =
            SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), header_data()).unwrap();
        assert!(submission.update(header_data()).is_err());
    }

    #[test]
    fn line_new_computes_amounts_per_row() {
        let line = SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new("csl-1"),
            SalesChangeSubmissionId::new("cs-1"),
            line_data(1),
        )
        .unwrap();

        assert_eq!(line.gross_amount, amt("29.97"));
        assert_eq!(line.net_amount, amt("26.07"));
        assert_eq!(line.tax_amount, amt("3.90"));
        assert_eq!(
            line.gross_amount.to_decimal(),
            line.net_amount.to_decimal() + line.tax_amount.to_decimal(),
            "gross = net + tax 逐行成立"
        );
    }

    #[test]
    fn line_new_rejects_zero_no_and_mismatch() {
        let zero = SalesChangeSubmissionLineData {
            line_no: 0,
            ..line_data(1)
        };
        assert!(SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new("csl-1"),
            SalesChangeSubmissionId::new("cs-1"),
            zero
        )
        .is_err());

        let mismatch = SalesChangeSubmissionLineData {
            line_type: LineType::Voucher,
            goods: None,
            voucher: None,
            ..line_data(1)
        };
        assert!(SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new("csl-1"),
            SalesChangeSubmissionId::new("cs-1"),
            mismatch
        )
        .is_err());
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let submission =
            SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), header_data()).unwrap();
        let roundtrip: SalesChangeSubmission =
            bson::deserialize_from_document(bson::serialize_to_document(&submission).unwrap()).unwrap();
        assert_eq!(roundtrip, submission);
    }
}
