//! `sales_order_revision`、`sales_order_revision_line` 及两个子类型
//! （数据模型 §6.4）。
//!
//! 生效内容写不可变 `sales_order_revision` 及结构化 `*_revision_line`（§4.4）；
//! 正式版本业务字段不可更新，只允许修复非业务性元数据并留审计（P3）。版本内联
//! 客户名称、合同编号、结算主体、税务、付款条件、商品名称、规格、单位等结构化
//! 快照（P1 §2.2），禁止 JSON blob。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    ContractRevisionId, PartyRevisionId, SalesOrderGoodsServiceLineRevisionId, SalesOrderId, SalesOrderLineId,
    SalesOrderRevisionId, SalesOrderRevisionLineId, SalesOrderVoucherLineRevisionId, SkuId, SkuRevisionId,
};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::amount_validation::validate_amount_triple;
use super::snapshot::HeaderSnapshots;
use super::types::{CardForm, GoodsLineFields, LineType, WelfareScenario};
use super::working_copy::SalesOrderWorkingCopyLineData;

/// 内容指纹最大长度。
const CONTENT_HASH_MAX_LEN: usize = 128;
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
/// 基础单位代码最大长度。
const BASE_UNIT_CODE_MAX_LEN: usize = 32;

/// 版本来源（数据模型：`ERP_APPROVAL`、`SALES_CHANGE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RevisionSource {
    /// ERP 审批通过。
    ErpApproval,
    /// 销售变更。
    SalesChange,
}

impl RevisionSource {
    /// 返回来源的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ErpApproval => "ERP 审批",
            Self::SalesChange => "销售变更",
        }
    }

    /// 返回来源的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ErpApproval => "ERP_APPROVAL",
            Self::SalesChange => "SALES_CHANGE",
        }
    }
}

/// 销售版本创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderRevisionData {
    /// 稳定销售单。
    pub sales_order_id: SalesOrderId,
    /// 聚合内从 1 递增的版本号。
    pub revision_no: u32,
    /// 版本来源。
    pub revision_source: RevisionSource,
    /// 前一生效版本。
    pub previous_revision_id: Option<SalesOrderRevisionId>,
    /// 本版全部商业字段的规范化指纹。
    pub content_hash: String,
    /// 生效时客户基础资料版本。
    pub customer_revision_id: Option<PartyRevisionId>,
    /// 生效时合同版本。
    pub contract_revision_id: Option<ContractRevisionId>,
    /// 表头结构化快照入参（客户/合同/结算/付款/开票）。
    pub snapshot: super::snapshot::HeaderSnapshotData,
    /// 客户项目名称（一期来源 `entry_name` 的正式落点，不纳入内容指纹）。
    pub project_name: Option<String>,
    /// 业务备注（不纳入内容指纹）。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU（卡券单必填，非卡券单为空）。
    pub voucher_category_sku_id: Option<SkuId>,
    /// 卡券履约期限（卡券单必填，非卡券单为空；保留来源精确到期时间）。
    pub voucher_expiry_at: Option<Instant>,
    /// 已舍入行汇总（含税）。
    pub gross_amount: Amount,
    /// 已舍入行汇总（不含税）。
    pub net_amount: Amount,
    /// 已舍入行汇总（税额）。
    pub tax_amount: Amount,
    /// 生效时间。
    pub effective_at: Instant,
    /// 入账时间。
    pub recorded_at: Instant,
}

/// 销售版本实体（不可变修订，数据模型 §6.4）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 稳定销售单。
    pub sales_order_id: SalesOrderId,
    /// 版本来源。
    pub revision_source: RevisionSource,
    /// 前一生效版本。
    pub previous_revision_id: Option<SalesOrderRevisionId>,
    /// 本版全部商业字段的规范化指纹。
    pub content_hash: String,
    /// 生效时客户基础资料版本。
    pub customer_revision_id: Option<PartyRevisionId>,
    /// 生效时合同版本。
    pub contract_revision_id: Option<ContractRevisionId>,
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
    /// 已舍入行汇总（含税）。
    pub gross_amount: Amount,
    /// 已舍入行汇总（不含税）。
    pub net_amount: Amount,
    /// 已舍入行汇总（税额）。
    pub tax_amount: Amount,
    /// 生效时间。
    pub effective_at: Instant,
    /// 入账时间。
    pub recorded_at: Instant,
}

impl SalesOrderRevision {
    /// 创建销售版本（不可变）。
    ///
    /// 完成内容指纹与快照字段的校验与规范化，并强制两条不变式：
    /// - 卡券类目与履约期限必须同时提供或同时省略（§6.4 卡券单必填规则；
    ///   「是否卡券单」由关联销售单判定，属于跨文档校验，P3 在形成版本时复核）；
    /// - `gross = net + tax` 精确成立（§4.2 表头只汇总已舍入行金额）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的销售版本实体。
    ///
    /// # 错误
    /// 必填为空、超长、关联不一致或金额三元组不成立时返回错误。
    pub fn new(id: SalesOrderRevisionId, data: SalesOrderRevisionData) -> Result<Self> {
        if data.revision_no == 0 {
            return Err(Error::from("版本号必须为正整数"));
        }
        let content_hash = normalize_required_text(
            data.content_hash,
            "内容指纹不能为空",
            CONTENT_HASH_MAX_LEN,
            "内容指纹过长",
        )?;
        let snapshots = HeaderSnapshots::build(&data.snapshot)?;
        let project_name = normalize_optional_text(data.project_name, "项目名称", PROJECT_NAME_MAX_LEN)?;
        let business_remark =
            normalize_optional_text(data.business_remark, "业务备注", BUSINESS_REMARK_MAX_LEN)?;
        if data.voucher_category_sku_id.is_some() != data.voucher_expiry_at.is_some() {
            return Err(Error::from("卡券类目与卡券履约期限必须同时提供或同时省略"));
        }
        validate_amount_triple(data.gross_amount, data.net_amount, data.tax_amount)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            sales_order_id: data.sales_order_id,
            revision_source: data.revision_source,
            previous_revision_id: data.previous_revision_id,
            content_hash,
            customer_revision_id: data.customer_revision_id,
            contract_revision_id: data.contract_revision_id,
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
            effective_at: data.effective_at,
            recorded_at: data.recorded_at,
        })
    }

    /// 更新销售版本。
    ///
    /// 正式版本业务字段不可更新（数据模型 §6.4：只允许修复非业务性元数据并留
    /// 审计，属 P3 服务职责）；此方法恒拒绝，保留签名以表达不可变性契约。
    ///
    /// # 参数
    /// * `_data` - 更新数据（被拒绝）
    ///
    /// # 返回
    /// 恒返回 `Err`。
    ///
    /// # 错误
    /// 恒返回「正式版本不可更新」错误。
    pub fn update(&mut self, _data: SalesOrderRevisionData) -> Result<()> {
        Err(Error::from("正式销售版本业务字段不可更新"))
    }

    /// 判断聚合内版本号是否与调用方期望一致。
    ///
    /// # 参数
    /// * `expected_revision_no` - 调用方读取到的销售版本号
    ///
    /// # 返回
    /// 当前版本号与期望版本号一致时返回 `true`。
    pub fn matches_revision_no(&self, expected_revision_no: u32) -> bool {
        self.revision.revision_no == expected_revision_no
    }

    /// 由当前最大版本号计算下一销售版本号。
    ///
    /// # 参数
    /// * `current_max` - 当前销售单全部正式版本中的最大版本号；尚无版本时为 `0`
    ///
    /// # 返回
    /// 返回严格递增的下一版本号。
    ///
    /// # 错误
    /// 当前版本号达到 `u32::MAX` 时返回错误。
    pub fn next_revision_no(current_max: u32) -> Result<u32> {
        current_max
            .checked_add(1)
            .ok_or_else(|| Error::from("销售版本号溢出"))
    }

    /// 返回卡券执行投影所需的表头履约期限。
    ///
    /// 卡券投影要求类目 SKU 与履约期限同时存在；任一缺失即不可投影。
    ///
    /// # 返回
    /// 返回表头履约期限。
    ///
    /// # 错误
    /// 类目或履约期限缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 唯一卡券行数量由仓储结果决定，不在本方法判定。本方法不触及持久化。
    pub fn required_voucher_expiry(&self) -> Result<Instant> {
        match (&self.voucher_category_sku_id, self.voucher_expiry_at) {
            (Some(_), Some(expiry)) => Ok(expiry),
            _ => Err(Error::from("非卡券销售单无法建立执行投影")),
        }
    }
}

/// 公共行版本创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderRevisionLineData {
    /// 所属销售版本。
    pub sales_order_revision_id: SalesOrderRevisionId,
    /// 稳定明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 本版展示顺序。
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
}

/// 公共行版本实体（数据模型 §6.4：`(sales_order_revision_id, sales_order_line_id)`
/// 唯一；金额来自子类型行，跨行一致性（公共行金额 = 子类型行金额）属于跨行断言，
/// P3 在形成版本时校验）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderRevisionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属销售版本。
    pub sales_order_revision_id: SalesOrderRevisionId,
    /// 稳定明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 本版展示顺序。
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
}

impl SalesOrderRevisionLine {
    /// 创建公共行版本。
    ///
    /// 完成文本字段校验与规范化，并强制行级金额恒等式 `gross = net + tax`
    /// （§4.2 规则 1，逐行舍入）。「公共行 `gross_amount` 必须等于对应子类型行
    /// 金额」属于跨行断言（§6.4），P3 在形成版本时校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderRevisionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的公共行版本。
    ///
    /// # 错误
    /// 行号为零、必填为空、超长或金额三元组不成立时返回错误。
    pub fn new(id: SalesOrderRevisionLineId, data: SalesOrderRevisionLineData) -> Result<Self> {
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
        validate_amount_triple(data.gross_amount, data.net_amount, data.tax_amount)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sales_order_revision_id: data.sales_order_revision_id,
            sales_order_line_id: data.sales_order_line_id,
            line_no: data.line_no,
            line_type: data.line_type,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            sales_tax_rate: data.sales_tax_rate,
            item_name_snapshot,
            spec_snapshot,
            unit_snapshot,
        })
    }

    /// 将公共行与实物服务子行还原为销售变更工作副本行数据。
    ///
    /// # 参数
    /// * `goods` - 与当前公共行一对一的实物服务子行
    ///
    /// # 返回
    /// 返回可用于创建变更工作副本行的冻结数据。
    ///
    /// # 错误
    /// 公共行不是实物服务类型，或子行未引用当前公共行时返回错误。
    pub fn to_goods_working_copy_data(
        &self,
        goods: &SalesOrderGoodsServiceLineRevision,
    ) -> Result<SalesOrderWorkingCopyLineData> {
        if self.line_type != LineType::GoodsService {
            return Err(Error::from(format!(
                "销售单当前版本第 {} 行不是实物服务行",
                self.line_no
            )));
        }
        if goods.revision_line_id.as_ref() != self.base.id {
            return Err(Error::from(format!(
                "销售单当前版本第 {} 行与实物服务快照不匹配",
                self.line_no
            )));
        }
        Ok(SalesOrderWorkingCopyLineData {
            sales_order_line_id: self.sales_order_line_id.clone(),
            line_no: self.line_no,
            line_type: self.line_type,
            sales_tax_rate: self.sales_tax_rate,
            item_name_snapshot: self.item_name_snapshot.clone(),
            spec_snapshot: self.spec_snapshot.clone(),
            unit_snapshot: self.unit_snapshot.clone(),
            goods: Some(GoodsLineFields {
                sku_id: goods.sku_id.clone(),
                sku_revision_id: goods.sku_revision_id.clone(),
                welfare_scenario: goods.welfare_scenario,
                service_region: goods.service_region.clone(),
                fulfillment_due_at: goods.fulfillment_due_at,
                quantity: goods.quantity,
                base_unit_code: goods.base_unit_code.clone(),
                unit_price_gross: goods.unit_price_gross,
            }),
            voucher: None,
        })
    }
}

/// 实物及服务行版本创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderGoodsServiceLineRevisionData {
    /// 与公共行一对一。
    pub revision_line_id: SalesOrderRevisionLineId,
    /// 正式销售项 SKU。
    pub sku_id: SkuId,
    /// 精确 SKU 修订；销售提交时重新校验销售资格（P3 服务职责）。
    pub sku_revision_id: SkuRevisionId,
    /// 福利场景。
    pub welfare_scenario: Option<WelfareScenario>,
    /// 采购责任解析使用的服务区域。
    pub service_region: Option<String>,
    /// 公司对客户承诺完成本明细交付或服务的最晚时间。
    pub fulfillment_due_at: Instant,
    /// 基础单位数量。
    pub quantity: Quantity,
    /// 基础单位代码。
    pub base_unit_code: String,
    /// 含税成交单价快照；不随后续 `sku_revision.sales_visible_price_gross` 变化。
    pub unit_price_gross: UnitPrice,
}

/// 实物及服务行版本实体（数据模型 §6.4，与公共行一对一）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderGoodsServiceLineRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 与公共行一对一。
    pub revision_line_id: SalesOrderRevisionLineId,
    /// 正式销售项 SKU。
    pub sku_id: SkuId,
    /// 精确 SKU 修订。
    pub sku_revision_id: SkuRevisionId,
    /// 福利场景。
    pub welfare_scenario: Option<WelfareScenario>,
    /// 采购责任解析使用的服务区域。
    pub service_region: Option<String>,
    /// 公司对客户承诺完成本明细交付或服务的最晚时间。
    pub fulfillment_due_at: Instant,
    /// 基础单位数量。
    pub quantity: Quantity,
    /// 基础单位代码。
    pub base_unit_code: String,
    /// 含税成交单价快照。
    pub unit_price_gross: UnitPrice,
}

impl SalesOrderGoodsServiceLineRevision {
    /// 创建实物及服务行版本。
    ///
    /// 完成基础单位代码的校验与规范化（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderGoodsServiceLineRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的实物及服务行版本。
    ///
    /// # 错误
    /// 基础单位代码为空或超长时返回错误。
    pub fn new(
        id: SalesOrderGoodsServiceLineRevisionId,
        data: SalesOrderGoodsServiceLineRevisionData,
    ) -> Result<Self> {
        let base_unit_code = normalize_required_text(
            data.base_unit_code,
            "基础单位不能为空",
            BASE_UNIT_CODE_MAX_LEN,
            "基础单位过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision_line_id: data.revision_line_id,
            sku_id: data.sku_id,
            sku_revision_id: data.sku_revision_id,
            welfare_scenario: data.welfare_scenario,
            service_region: data.service_region,
            fulfillment_due_at: data.fulfillment_due_at,
            quantity: data.quantity,
            base_unit_code,
            unit_price_gross: data.unit_price_gross,
        })
    }
}

/// 卡券行版本创建数据（面额小计、成交金额与配赠金额由实体按 §6.4 公式推导）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderVoucherLineRevisionData {
    /// 与公共行一对一。
    pub revision_line_id: SalesOrderRevisionLineId,
    /// 单卡面额。
    pub face_value: Amount,
    /// 卡张数（正整数）。
    pub card_count: u32,
    /// 单卡含税成交单价；一期来源 `sell_price` 的结构化落点。
    pub unit_price_gross: UnitPrice,
    /// 卡形态。
    pub card_form: CardForm,
}

/// 卡券行版本实体（数据模型 §6.4，与公共行一对一）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderVoucherLineRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 与公共行一对一。
    pub revision_line_id: SalesOrderRevisionLineId,
    /// 单卡面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 单卡含税成交单价。
    pub unit_price_gross: UnitPrice,
    /// 面额乘张数。
    pub face_value_total: Amount,
    /// 最终成交金额。
    pub transaction_amount: Amount,
    /// 配赠金额。
    pub gift_amount: Amount,
    /// 配赠率。
    pub gift_rate: Rate,
    /// 卡形态。
    pub card_form: CardForm,
}

impl SalesOrderVoucherLineRevision {
    /// 创建卡券行版本。
    ///
    /// 按数据模型 §6.4 推导并强制：`face_value_total = face_value × card_count`、
    /// `transaction_amount = round(unit_price_gross × card_count)`、
    /// `gift_amount = face_value_total − transaction_amount`、
    /// `gift_rate = gift_amount / transaction_amount`（成交金额为零时拒绝生效）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderVoucherLineRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的卡券行版本。
    ///
    /// # 错误
    /// 卡张数为零或成交金额为零时返回错误。
    pub fn new(id: SalesOrderVoucherLineRevisionId, data: SalesOrderVoucherLineRevisionData) -> Result<Self> {
        if data.card_count == 0 {
            return Err(Error::from("卡券行卡张数必须为正整数"));
        }
        let amounts =
            super::types::derive_voucher_amounts(data.face_value, data.card_count, data.unit_price_gross)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision_line_id: data.revision_line_id,
            face_value: data.face_value,
            card_count: data.card_count,
            unit_price_gross: data.unit_price_gross,
            face_value_total: amounts.face_value_total,
            transaction_amount: amounts.transaction_amount,
            gift_amount: amounts.gift_amount,
            gift_rate: amounts.gift_rate,
            card_form: data.card_form,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::money::Amount;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn header_data() -> SalesOrderRevisionData {
        SalesOrderRevisionData {
            sales_order_id: SalesOrderId::new("o-1"),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            previous_revision_id: None,
            content_hash: " abc123def456 ".to_string(),
            customer_revision_id: Some(PartyRevisionId::new("party-rev-1")),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
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
            effective_at: Instant::from_unix_secs(1_800_000_000),
            recorded_at: Instant::from_unix_secs(1_800_000_100),
        }
    }

    #[test]
    fn new_trims_snapshots_and_keeps_revision_metadata() {
        let revision = SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), header_data()).unwrap();

        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.content_hash, "abc123def456");
        assert_eq!(revision.customer_snapshot.customer_name, "东方企业");
        assert_eq!(revision.contract_snapshot.unwrap().contract_no, "HT-2026-0088");
        assert_eq!(
            revision.settlement_party_snapshot.unwrap().settlement_party_name,
            "集团结算中心"
        );
        assert_eq!(revision.payment_term_snapshot.payment_term_name, "月结 30 天");
        assert_eq!(revision.invoice_requirement_snapshot.tax_point, "6");
        assert_eq!(revision.project_name.as_deref(), Some("端午福利项目"));
        assert_eq!(revision.effective_at.unix_secs(), 1_800_000_000);
        assert_eq!(revision.recorded_at.unix_secs(), 1_800_000_100);
        assert_eq!(revision.revision_source, RevisionSource::ErpApproval);
        assert!(revision.voucher_category_sku_id.is_none());
        assert!(revision.voucher_expiry_at.is_none());
    }

    #[test]
    fn new_rejects_blank_overlong_and_broken_invariants() {
        let blank_hash = SalesOrderRevisionData {
            content_hash: "   ".to_string(),
            ..header_data()
        };
        assert!(SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), blank_hash).is_err());

        let overlong_remark = SalesOrderRevisionData {
            business_remark: Some("x".repeat(1025)),
            ..header_data()
        };
        assert!(SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), overlong_remark).is_err());

        let zero_no = SalesOrderRevisionData {
            revision_no: 0,
            ..header_data()
        };
        assert!(SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), zero_no).is_err());

        let half_voucher = SalesOrderRevisionData {
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: None,
            ..header_data()
        };
        assert!(SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), half_voucher).is_err());

        let broken_amount = SalesOrderRevisionData {
            gross_amount: amt("29.98"),
            ..header_data()
        };
        assert!(SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), broken_amount).is_err());
    }

    #[test]
    fn required_voucher_expiry_covers_category_and_deadline_matrix() {
        let both = SalesOrderRevision::new(
            SalesOrderRevisionId::new("rev-v"),
            SalesOrderRevisionData {
                voucher_category_sku_id: Some(SkuId::new("vcat-1")),
                voucher_expiry_at: Some(Instant::from_unix_secs(1_850_000_000)),
                ..header_data()
            },
        )
        .unwrap();
        assert_eq!(both.required_voucher_expiry().unwrap().unix_secs(), 1_850_000_000);

        let mut missing_expiry = both.clone();
        missing_expiry.voucher_expiry_at = None;
        assert_eq!(
            missing_expiry.required_voucher_expiry().unwrap_err().to_string(),
            "非卡券销售单无法建立执行投影"
        );

        let mut missing_category = both.clone();
        missing_category.voucher_category_sku_id = None;
        assert_eq!(
            missing_category
                .required_voucher_expiry()
                .unwrap_err()
                .to_string(),
            "非卡券销售单无法建立执行投影"
        );

        let neither = SalesOrderRevision::new(SalesOrderRevisionId::new("rev-g"), header_data()).unwrap();
        assert_eq!(
            neither.required_voucher_expiry().unwrap_err().to_string(),
            "非卡券销售单无法建立执行投影"
        );
    }

    #[test]
    fn revision_number_and_change_copy_mapping_are_entity_owned() {
        let revision = SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), header_data()).unwrap();
        assert!(revision.matches_revision_no(1));
        assert!(!revision.matches_revision_no(2));
        assert_eq!(SalesOrderRevision::next_revision_no(0).unwrap(), 1);
        assert_eq!(SalesOrderRevision::next_revision_no(1).unwrap(), 2);
        assert!(SalesOrderRevision::next_revision_no(u32::MAX).is_err());

        let line =
            SalesOrderRevisionLine::new(SalesOrderRevisionLineId::new("rl-1"), revision_line_data()).unwrap();
        let goods = SalesOrderGoodsServiceLineRevision::new(
            SalesOrderGoodsServiceLineRevisionId::new("gs-1"),
            data(),
        )
        .unwrap();
        let mapped = line.to_goods_working_copy_data(&goods).unwrap();
        assert_eq!(mapped.line_no, 1);
        assert_eq!(mapped.goods.unwrap().sku_id.as_ref(), "sku-1");

        let unrelated = SalesOrderGoodsServiceLineRevision::new(
            SalesOrderGoodsServiceLineRevisionId::new("gs-2"),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new("rl-2"),
                ..data()
            },
        )
        .unwrap();
        assert!(line.to_goods_working_copy_data(&unrelated).is_err());
    }

    #[test]
    fn update_is_rejected_for_immutable_revision() {
        let mut revision =
            SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), header_data()).unwrap();
        assert!(revision.update(header_data()).is_err());
    }

    #[test]
    fn revision_line_keeps_amount_triple_consistency() {
        let line = SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new("rl-1"),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_line_id: SalesOrderLineId::new("line-1"),
                line_no: 1,
                line_type: LineType::GoodsService,
                gross_amount: amt("29.97"),
                net_amount: amt("26.07"),
                tax_amount: amt("3.90"),
                sales_tax_rate: Rate::from_str("0.130000").unwrap(),
                item_name_snapshot: " 年货礼盒 ".to_string(),
                spec_snapshot: Some(" 10kg ".to_string()),
                unit_snapshot: None,
            },
        )
        .unwrap();

        assert_eq!(line.item_name_snapshot, "年货礼盒");
        assert_eq!(line.spec_snapshot.as_deref(), Some("10kg"));
        assert_eq!(
            line.gross_amount.to_decimal(),
            line.net_amount.to_decimal() + line.tax_amount.to_decimal()
        );
    }

    #[test]
    fn revision_line_rejects_zero_no_and_broken_triple() {
        let zero = SalesOrderRevisionLineData {
            line_no: 0,
            ..revision_line_data()
        };
        assert!(SalesOrderRevisionLine::new(SalesOrderRevisionLineId::new("rl-1"), zero).is_err());

        let broken = SalesOrderRevisionLineData {
            net_amount: amt("26.06"),
            ..revision_line_data()
        };
        assert!(SalesOrderRevisionLine::new(SalesOrderRevisionLineId::new("rl-1"), broken).is_err());
    }

    fn revision_line_data() -> SalesOrderRevisionLineData {
        SalesOrderRevisionLineData {
            sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            line_no: 1,
            line_type: LineType::GoodsService,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            item_name_snapshot: "年货礼盒".to_string(),
            spec_snapshot: None,
            unit_snapshot: None,
        }
    }

    #[test]
    fn goods_service_line_revision_normalizes_unit() {
        let line = SalesOrderGoodsServiceLineRevision::new(
            SalesOrderGoodsServiceLineRevisionId::new("gs-1"),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new("rl-1"),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skurev-1"),
                welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
                service_region: Some("EAST".to_string()),
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: Quantity::from_str("3.000000").unwrap(),
                base_unit_code: " 箱 ".to_string(),
                unit_price_gross: price("9.9900"),
            },
        )
        .unwrap();

        assert_eq!(line.base_unit_code, "箱");
        assert_eq!(line.unit_price_gross, price("9.9900"));

        let blank_unit = SalesOrderGoodsServiceLineRevisionData {
            base_unit_code: "   ".to_string(),
            ..data()
        };
        assert!(SalesOrderGoodsServiceLineRevision::new(
            SalesOrderGoodsServiceLineRevisionId::new("gs-2"),
            blank_unit
        )
        .is_err());
    }

    fn data() -> SalesOrderGoodsServiceLineRevisionData {
        SalesOrderGoodsServiceLineRevisionData {
            revision_line_id: SalesOrderRevisionLineId::new("rl-1"),
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: None,
            service_region: None,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str("3.000000").unwrap(),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    #[test]
    fn voucher_line_revision_derives_amounts_and_gift_rate() {
        let line = SalesOrderVoucherLineRevision::new(
            SalesOrderVoucherLineRevisionId::new("v-1"),
            SalesOrderVoucherLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new("rl-1"),
                face_value: amt("100.00"),
                card_count: 3,
                unit_price_gross: price("90.0000"),
                card_form: CardForm::Physical,
            },
        )
        .unwrap();

        assert_eq!(line.face_value_total, amt("300.00"), "面额乘张数");
        assert_eq!(line.transaction_amount, amt("270.00"), "单价乘张数按约定舍入");
        assert_eq!(line.gift_amount, amt("30.00"), "面额小计减成交金额");
        assert_eq!(line.gift_rate.to_decimal().to_string(), "0.111111");
        assert_eq!(line.card_form, CardForm::Physical);
    }

    #[test]
    fn voucher_line_revision_rejects_zero_count_and_zero_transaction() {
        let zero_count = SalesOrderVoucherLineRevisionData {
            card_count: 0,
            ..voucher_data()
        };
        assert!(
            SalesOrderVoucherLineRevision::new(SalesOrderVoucherLineRevisionId::new("v-1"), zero_count)
                .is_err()
        );

        // 成交金额为零时拒绝生效，不保存无定义比率（§6.4）。
        let zero_transaction = SalesOrderVoucherLineRevisionData {
            unit_price_gross: price("0.0000"),
            ..voucher_data()
        };
        assert!(SalesOrderVoucherLineRevision::new(
            SalesOrderVoucherLineRevisionId::new("v-1"),
            zero_transaction
        )
        .is_err());
    }

    fn voucher_data() -> SalesOrderVoucherLineRevisionData {
        SalesOrderVoucherLineRevisionData {
            revision_line_id: SalesOrderRevisionLineId::new("rl-1"),
            face_value: amt("100.00"),
            card_count: 3,
            unit_price_gross: price("90.0000"),
            card_form: CardForm::Electronic,
        }
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let revision = SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), header_data()).unwrap();
        let roundtrip: SalesOrderRevision =
            bson::deserialize_from_document(bson::serialize_to_document(&revision).unwrap()).unwrap();
        assert_eq!(roundtrip, revision);

        let line =
            SalesOrderVoucherLineRevision::new(SalesOrderVoucherLineRevisionId::new("v-1"), voucher_data())
                .unwrap();
        let roundtrip_line: SalesOrderVoucherLineRevision =
            bson::deserialize_from_document(bson::serialize_to_document(&line).unwrap()).unwrap();
        assert_eq!(roundtrip_line, line);
    }
}
