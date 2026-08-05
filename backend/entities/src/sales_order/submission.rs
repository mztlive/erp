//! `sales_order_submission` 与 `sales_order_submission_line`（数据模型 §6.5）。
//!
//! 提交快照用于冻结审批对象，不等同于正式销售版本；被驳回的提交永久保留但不进入
//! 经营台账。提交头、明细形成后不可修改；全部审批和采购二次确认引用具体
//! `submission_id`。最终通过时，事务把该提交的结构化字段原样写为正式
//! `sales_order_revision` 和版本明细（P3 服务编排）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    ContractRevisionId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId, SalesOrderSubmissionId,
    SalesOrderSubmissionLineId, SalesOrderWorkingCopyId, SkuId,
};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::snapshot::HeaderSnapshots;
use super::types::{
    build_line_groups, validate_line_list, BusinessType, FulfillmentMode, GoodsLineFields, LineSummary,
    LineType, VoucherLineDraft, WelfareScenario,
};
use super::working_copy::validate_amount_triple;

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

/// 提交状态（数据模型 §6.5：审核中、已通过、已驳回、因重新提交失效）。
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
    /// 审核中可被通过、驳回或因新提交失效；其余为终态（旧提交不复用，§6.5）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::InReview => &[Self::Approved, Self::Rejected, Self::Superseded],
            Self::Approved | Self::Rejected | Self::Superseded => &[],
        }
    }
}

/// 提交创建数据（由提交事务把工作副本头、行原样复制而来）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderSubmissionData {
    /// 稳定销售单。
    pub sales_order_id: SalesOrderId,
    /// 提交序号（每次销售修改后重新提交必须新建序号）。
    pub submission_no: u32,
    /// 形成提交的草稿。
    pub working_copy_id: SalesOrderWorkingCopyId,
    /// 形成提交的草稿服务端确认版本。
    pub working_copy_version: u32,
    /// 业务性质（与销售单一致）。
    pub business_type: BusinessType,
    /// 提交时客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 提交时合同版本。
    pub contract_revision_id: Option<ContractRevisionId>,
    /// 提交时结算主体。
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
    /// 提交行汇总（含税）。
    pub gross_amount: Amount,
    /// 提交行汇总（不含税）。
    pub net_amount: Amount,
    /// 提交行汇总（税额）。
    pub tax_amount: Amount,
    /// 提交审计时间。
    pub submitted_at: Instant,
    /// 提交审计人。
    pub submitted_by: String,
    /// 行清单（列表去重与跨行断言在 `new` 内完成）。
    pub lines: Vec<SalesOrderSubmissionLineData>,
}

/// 提交实体（不可变提交快照，数据模型 §6.5）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesOrderSubmission {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SubmissionStatus>,
    /// 稳定销售单。
    pub sales_order_id: SalesOrderId,
    /// 提交序号。
    pub submission_no: u32,
    /// 形成提交的草稿。
    pub working_copy_id: SalesOrderWorkingCopyId,
    /// 形成提交的草稿服务端确认版本。
    pub working_copy_version: u32,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 提交时客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 提交时合同版本。
    pub contract_revision_id: Option<ContractRevisionId>,
    /// 提交时结算主体。
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
    /// 提交行汇总（含税）。
    pub gross_amount: Amount,
    /// 提交行汇总（不含税）。
    pub net_amount: Amount,
    /// 提交行汇总（税额）。
    pub tax_amount: Amount,
    /// 提交审计时间。
    pub submitted_at: Instant,
    /// 提交审计人。
    pub submitted_by: String,
}

impl PartialEq for SalesOrderSubmission {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_order_id == other.sales_order_id
            && self.submission_no == other.submission_no
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

impl Eq for SalesOrderSubmission {}

impl SalesOrderSubmission {
    /// 创建提交（不可变快照）。
    ///
    /// 完成全部文本字段的校验与规范化，并强制不变式：卡券类目与履约期限同时
    /// 提供或同时省略、`gross = net + tax` 精确成立、行清单按 [`validate_line_list`]
    /// 去重并断言行类型与业务性质一致（§6.4/§6.5 跨行断言）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderSubmissionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交实体（`InReview`）。
    ///
    /// # 错误
    /// 必填为空、超长、关联不一致、金额三元组不成立或行清单非法时返回错误。
    pub fn new(id: SalesOrderSubmissionId, data: SalesOrderSubmissionData) -> Result<Self> {
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
            sales_order_id: data.sales_order_id,
            submission_no: data.submission_no,
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

    /// 更新提交。
    ///
    /// 提交头、明细形成后不可修改（数据模型 §6.5）；此方法恒拒绝，保留签名以表达
    /// 不可变性契约。
    ///
    /// # 参数
    /// * `_data` - 更新数据（被拒绝）
    ///
    /// # 返回
    /// 恒返回 `Err`。
    ///
    /// # 错误
    /// 恒返回「提交快照不可更新」错误。
    pub fn update(&mut self, _data: SalesOrderSubmissionData) -> Result<()> {
        Err(Error::from("提交快照形成后不可修改"))
    }

    /// 通过提交（`InReview → Approved`）。
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

    /// 驳回提交（`InReview → Rejected`）。
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

    /// 标记因重新提交失效（`InReview → Superseded`）。
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

/// 提交行创建数据（行字段组按 `line_type` 二选一）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderSubmissionLineData {
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

/// 提交行实体（不可变提交快照明细，数据模型 §6.5）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderSubmissionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属提交。
    pub submission_id: SalesOrderSubmissionId,
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

impl SalesOrderSubmissionLine {
    /// 创建提交行。
    ///
    /// 完成文本字段校验与规范化，行金额三元组按
    /// [`crate::money::line_amounts`] 统一计算（§4.2 逐行舍入）；卡券行按 §6.4
    /// 校验面额小计、成交金额与配赠金额一致性。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderSubmissionLineId`）
    /// * `submission_id` - 所属提交
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交行。
    ///
    /// # 错误
    /// 行号为零、必填为空、超长、行类型与字段组不一致或卡券金额不一致时返回错误。
    pub fn new(
        id: SalesOrderSubmissionLineId,
        submission_id: SalesOrderSubmissionId,
        data: SalesOrderSubmissionLineData,
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
            submission_id,
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
            fulfillment_mode: FulfillmentMode::CompanyWarehouse,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    fn line_data(line_no: u32) -> SalesOrderSubmissionLineData {
        SalesOrderSubmissionLineData {
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

    fn header_data() -> SalesOrderSubmissionData {
        SalesOrderSubmissionData {
            sales_order_id: SalesOrderId::new("o-1"),
            submission_no: 1,
            working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 3,
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
            business_remark: Some(" 按合同执行 ".to_string()),
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            submitted_at: Instant::from_unix_secs(1_790_000_000),
            submitted_by: " sales-1 ".to_string(),
            lines: vec![line_data(1)],
        }
    }

    #[test]
    fn new_trims_and_initializes_in_review() {
        let submission =
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), header_data()).unwrap();

        assert_eq!(submission.submitted_by, "sales-1");
        assert_eq!(submission.business_remark.as_deref(), Some("按合同执行"));
        assert_eq!(submission.customer_snapshot.customer_name, "东方企业");
        assert_eq!(submission.contract_snapshot.unwrap().contract_no, "HT-2026-0088");
        assert_eq!(submission.stable.status(), SubmissionStatus::InReview);
        assert_eq!(submission.working_copy_version, 3);
    }

    #[test]
    fn new_rejects_blank_and_broken_invariants() {
        let blank_submitter = SalesOrderSubmissionData {
            submitted_by: "   ".to_string(),
            ..header_data()
        };
        assert!(SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), blank_submitter).is_err());

        let zero_submission_no = SalesOrderSubmissionData {
            submission_no: 0,
            ..header_data()
        };
        assert!(SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), zero_submission_no).is_err());

        let half_voucher = SalesOrderSubmissionData {
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: None,
            ..header_data()
        };
        assert!(SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), half_voucher).is_err());

        let broken_amount = SalesOrderSubmissionData {
            net_amount: amt("26.08"),
            ..header_data()
        };
        assert!(SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), broken_amount).is_err());

        let duplicated_lines = SalesOrderSubmissionData {
            lines: vec![line_data(1), line_data(1)],
            ..header_data()
        };
        assert!(SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), duplicated_lines).is_err());
    }

    #[test]
    fn voucher_submission_enforces_single_voucher_line() {
        let voucher_data = SalesOrderSubmissionData {
            business_type: BusinessType::Voucher,
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: Some(Instant::from_unix_secs(1_850_000_000)),
            lines: vec![
                SalesOrderSubmissionLineData {
                    line_type: LineType::Voucher,
                    goods: None,
                    voucher: Some(super::super::types::VoucherLineDraft {
                        face_value: amt("100.00"),
                        card_count: 3,
                        unit_price_gross: price("90.0000"),
                        face_value_total: amt("300.00"),
                        transaction_amount: amt("270.00"),
                        gift_amount: amt("30.00"),
                        gift_rate: None,
                        card_form: super::super::types::CardForm::Electronic,
                    }),
                    ..line_data(1)
                },
                SalesOrderSubmissionLineData {
                    line_type: LineType::Voucher,
                    goods: None,
                    voucher: Some(super::super::types::VoucherLineDraft {
                        face_value: amt("100.00"),
                        card_count: 2,
                        unit_price_gross: price("90.0000"),
                        face_value_total: amt("200.00"),
                        transaction_amount: amt("180.00"),
                        gift_amount: amt("20.00"),
                        gift_rate: None,
                        card_form: super::super::types::CardForm::Electronic,
                    }),
                    ..line_data(2)
                },
            ],
            ..header_data()
        };
        assert!(SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), voucher_data).is_err());

        let goods_line_in_voucher_order = SalesOrderSubmissionData {
            business_type: BusinessType::Voucher,
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: Some(Instant::from_unix_secs(1_850_000_000)),
            ..header_data()
        };
        assert!(
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), goods_line_in_voucher_order)
                .is_err()
        );
    }

    #[test]
    fn status_machine_approve_reject_supersede() {
        let mut submission =
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), header_data()).unwrap();

        submission.reject("reviewer").unwrap();
        assert_eq!(submission.stable.status(), SubmissionStatus::Rejected);
        assert!(submission.approve("reviewer").is_err(), "驳回后不可通过");
        assert!(
            submission.mark_superseded("system").is_err(),
            "驳回后不可标记失效"
        );

        let mut approved =
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-2"), header_data()).unwrap();
        approved.approve("reviewer").unwrap();
        assert!(approved.reject("reviewer").is_err(), "通过为终态");

        let mut superseded =
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-3"), header_data()).unwrap();
        superseded.mark_superseded("system").unwrap();
        assert!(SubmissionStatus::Approved.allowed_next().is_empty());
        assert!(SubmissionStatus::Superseded.allowed_next().is_empty());
        assert!(ensure_transition(SubmissionStatus::Rejected, SubmissionStatus::InReview).is_err());
    }

    #[test]
    fn update_is_rejected_for_immutable_submission() {
        let mut submission =
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), header_data()).unwrap();
        assert!(submission.update(header_data()).is_err());
    }

    #[test]
    fn line_new_computes_amounts_per_row() {
        let line = SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("sl-1"),
            SalesOrderSubmissionId::new("s-1"),
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
        assert_eq!(line.item_name_snapshot, "年货礼盒");
        assert!(line.face_value.is_none());
    }

    #[test]
    fn line_new_rejects_zero_no_and_mismatch() {
        let zero = SalesOrderSubmissionLineData {
            line_no: 0,
            ..line_data(1)
        };
        assert!(SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("sl-1"),
            SalesOrderSubmissionId::new("s-1"),
            zero
        )
        .is_err());

        let mismatch = SalesOrderSubmissionLineData {
            line_type: LineType::Voucher,
            goods: None,
            voucher: None,
            ..line_data(1)
        };
        assert!(SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("sl-1"),
            SalesOrderSubmissionId::new("s-1"),
            mismatch
        )
        .is_err());
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let submission =
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), header_data()).unwrap();
        let roundtrip: SalesOrderSubmission =
            bson::from_document(bson::to_document(&submission).unwrap()).unwrap();
        assert_eq!(roundtrip, submission);
    }
}
