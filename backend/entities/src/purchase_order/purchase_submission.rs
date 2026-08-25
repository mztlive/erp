//! `purchase_order_submission` / `purchase_order_submission_line`（数据模型 §6.6）。
//!
//! 提交是不可变采购内容快照：进入待审核后头、行冻结；财务审批、工作任务及
//! `workflow_action` 必须引用具体提交，不得审批可变采购主表（§6.6）。
//! 提交没有 `fact_no`/`occurred_at`/`recorded_at` 语义字段，按 §6.6 字典精确建模，
//! 不套用 `FactBase`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{
    ProcurementConfirmationLineId, PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
    SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
    SupplierAccountId, SupplierCommercialProfileRevisionId,
};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::purchase_order::line_common::{normalize_and_validate_line, PurchaseLineDataRef};
use crate::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseLineType, PurchaseType};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 提交序号最大长度。
const SUBMISSION_NO_MAX_LEN: usize = 64;
/// 操作人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;
/// 结构化审核原因代码最大长度。
const REVIEW_REASON_CODE_MAX_LEN: usize = 64;
/// 审核意见最大长度。
const REVIEW_COMMENT_MAX_LEN: usize = 512;

/// 提交状态（§6.6：草稿、待审核、已通过、已驳回、因重新提交失效）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SubmissionStatus {
    /// 草稿。
    Draft,
    /// 待审核。
    Pending,
    /// 已通过。
    Approved,
    /// 已驳回。
    Rejected,
    /// 因重新提交失效。
    Superseded,
}

/// 财务审核强类型结论。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "review_result", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseOrderReviewDecision {
    /// 审核通过；可以记录补充说明，不携带驳回原因代码。
    Approved {
        /// 补充说明。
        comment: Option<String>,
    },
    /// 审核驳回；必须记录结构化原因代码。
    Rejected {
        /// 结构化驳回原因代码。
        reason_code: String,
        /// 补充说明。
        comment: Option<String>,
    },
}

impl SubmissionStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Pending => "待审核",
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
            Self::Draft => "DRAFT",
            Self::Pending => "PENDING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

/// 采购提交创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionData {
    /// 所属采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 提交序号（聚合内唯一）。
    pub submission_no: String,
    /// 供应商（拆单维度）。
    pub supplier_id: SupplierAccountId,
    /// 采购类型（拆单维度）。
    pub purchase_type: PurchaseType,
    /// 履约责任（拆单维度）。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 提交时供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 提交时供应商快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 付款条件和先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
}

/// 采购提交更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PurchaseOrderSubmissionUpdate {
    /// 供应商版本；`None` 表示不修改。
    pub supplier_revision_id: Option<SupplierCommercialProfileRevisionId>,
    /// 供应商快照；`None` 表示不修改。
    pub supplier_snapshot: Option<SupplierSnapshot>,
    /// 付款条件门禁快照；`None` 表示不修改。
    pub payment_term_snapshot: Option<PaymentTermSnapshot>,
}

/// 采购提交实体（不可变提交，数据模型 §6.6）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseOrderSubmission {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 提交序号。
    pub submission_no: String,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 提交时供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 提交时供应商快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 付款条件和先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
    /// 提交状态。
    pub status: SubmissionStatus,
    /// 提交审计时间；与 `submitted_by` 成对出现。
    pub submitted_at: Option<Instant>,
    /// 提交审计人；与 `submitted_at` 成对出现。
    pub submitted_by: Option<String>,
    /// 财务审核时间；与 `reviewed_by` 成对出现。
    pub reviewed_at: Option<Instant>,
    /// 财务审核人。
    pub reviewed_by: Option<String>,
    /// 驳回时的结构化原因代码；通过时为空。
    pub review_reason_code: Option<String>,
    /// 审核补充说明。
    pub review_comment: Option<String>,
}

impl PurchaseOrderSubmission {
    /// 创建采购提交。
    ///
    /// 完成 `submission_no` 校验与规范化，并强制表头金额守恒
    /// （`gross = net + tax`，§4.2 铁律 4；行汇总只汇总已舍入的行金额，由 P3 提供）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseOrderSubmissionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交实体（初始状态 `Draft`）。
    ///
    /// # 错误
    /// 提交序号为空/超长，或表头金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseOrderSubmissionId, data: PurchaseOrderSubmissionData) -> Result<Self> {
        let submission_no = normalize_required_text(
            data.submission_no,
            "提交序号不能为空",
            SUBMISSION_NO_MAX_LEN,
            "提交序号过长",
        )?;
        ensure_header_triple(
            data.gross_amount,
            data.net_amount,
            data.tax_amount,
            &submission_no,
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_order_id: data.purchase_order_id,
            submission_no,
            supplier_id: data.supplier_id,
            purchase_type: data.purchase_type,
            fulfillment_responsibility: data.fulfillment_responsibility,
            supplier_revision_id: data.supplier_revision_id,
            supplier_snapshot: data.supplier_snapshot,
            payment_term_snapshot: data.payment_term_snapshot,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            status: SubmissionStatus::Draft,
            submitted_at: None,
            submitted_by: None,
            reviewed_at: None,
            reviewed_by: None,
            review_reason_code: None,
            review_comment: None,
        })
    }

    /// 计算同一采购单的下一个正式提交序号。
    ///
    /// 仅识别 `SUB-{n}` 形态的历史提交，忽略草稿或旧格式编号；新编号固定为
    /// 六位十进制序号。
    ///
    /// # 参数
    /// * `existing` - 同一采购单的既有提交
    ///
    /// # 返回
    /// 返回下一个 `SUB-000001` 形态的提交序号。
    ///
    /// # 错误
    /// 最大合法序号已经达到 `u32::MAX` 时返回领域错误。
    pub fn next_submission_no(existing: &[Self]) -> Result<String> {
        let max_no = existing
            .iter()
            .filter_map(|submission| parse_sequence(&submission.submission_no, "SUB-"))
            .max()
            .unwrap_or(0);
        let next = max_no
            .checked_add(1)
            .ok_or_else(|| Error::from("采购提交序号溢出"))?;
        Ok(format!("SUB-{next:06}"))
    }

    /// 从可编辑草稿派生并冻结一个新的正式提交。
    ///
    /// # 参数
    /// * `id` - 新正式提交稳定身份
    /// * `submission_no` - 聚合内下一个正式提交序号
    /// * `draft` - 当前可编辑草稿提交
    /// * `submitted_at` - 冻结时间
    /// * `submitted_by` - 提交人
    ///
    /// # 返回
    /// 返回内容与草稿一致、状态为待审核的新提交。
    ///
    /// # 错误
    /// 草稿状态非法、提交序号非法、金额不守恒或提交人非法时返回领域错误。
    pub fn freeze_from_draft(
        id: PurchaseOrderSubmissionId,
        submission_no: String,
        draft: &Self,
        submitted_at: Instant,
        submitted_by: impl Into<String>,
    ) -> Result<Self> {
        draft.ensure_draft()?;
        let mut formal = Self::new(
            id,
            PurchaseOrderSubmissionData {
                purchase_order_id: draft.purchase_order_id.clone(),
                submission_no,
                supplier_id: draft.supplier_id.clone(),
                purchase_type: draft.purchase_type,
                fulfillment_responsibility: draft.fulfillment_responsibility,
                supplier_revision_id: draft.supplier_revision_id.clone(),
                supplier_snapshot: draft.supplier_snapshot.clone(),
                payment_term_snapshot: draft.payment_term_snapshot.clone(),
                gross_amount: draft.gross_amount,
                net_amount: draft.net_amount,
                tax_amount: draft.tax_amount,
            },
        )?;
        formal.submit(submitted_at, submitted_by)?;
        Ok(formal)
    }

    /// 更新提交内容。
    ///
    /// 只允许在 `Draft` 状态编辑（§6.6：进入待审核时头、行冻结）；
    /// `submission_no` 与拆单维度字段创建后不可修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿时返回错误。
    pub fn update(&mut self, update: PurchaseOrderSubmissionUpdate) -> Result<()> {
        self.ensure_draft()?;
        if let Some(revision_id) = update.supplier_revision_id {
            self.supplier_revision_id = revision_id;
        }
        if let Some(snapshot) = update.supplier_snapshot {
            self.supplier_snapshot = snapshot;
        }
        if let Some(snapshot) = update.payment_term_snapshot {
            self.payment_term_snapshot = snapshot;
        }
        Ok(())
    }

    /// 提交财务审核。
    ///
    /// 从草稿进入待审核并写入提交审计；提交后头行冻结。
    ///
    /// # 参数
    /// * `submitted_at` - 提交时间
    /// * `submitted_by` - 提交人
    ///
    /// # 返回
    /// 提交成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿时返回错误。
    pub fn submit(&mut self, submitted_at: Instant, submitted_by: impl Into<String>) -> Result<()> {
        self.ensure_draft()?;
        self.status = SubmissionStatus::Pending;
        self.submitted_at = Some(submitted_at);
        self.submitted_by = Some(normalize_required_text(
            submitted_by.into(),
            "提交人不能为空",
            ACTOR_MAX_LEN,
            "提交人标识过长",
        )?);
        Ok(())
    }

    /// 记录财务审核正式结论与处理审计。
    ///
    /// 只能对待审核提交执行；通过 → `Approved`，驳回 → `Rejected`。
    ///
    /// # 参数
    /// * `decision` - 强类型审核结论
    /// * `reviewed_at` - 审核时间
    /// * `reviewed_by` - 审核人
    ///
    /// # 返回
    /// 记录成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是待审核时返回错误。
    pub fn record_review(
        &mut self,
        decision: PurchaseOrderReviewDecision,
        reviewed_at: Instant,
        reviewed_by: impl Into<String>,
    ) -> Result<()> {
        if self.status != SubmissionStatus::Pending {
            return Err(Error::from("只有待审核的提交才能记录审核结论"));
        }
        let reviewed_by = normalize_required_text(
            reviewed_by.into(),
            "审核人不能为空",
            ACTOR_MAX_LEN,
            "审核人标识过长",
        )?;
        let (status, reason_code, comment) = match decision {
            PurchaseOrderReviewDecision::Approved { comment } => (
                SubmissionStatus::Approved,
                None,
                normalize_optional_text(comment, "审核说明", REVIEW_COMMENT_MAX_LEN)?,
            ),
            PurchaseOrderReviewDecision::Rejected { reason_code, comment } => (
                SubmissionStatus::Rejected,
                Some(normalize_required_text(
                    reason_code,
                    "驳回原因代码不能为空",
                    REVIEW_REASON_CODE_MAX_LEN,
                    "驳回原因代码过长",
                )?),
                normalize_optional_text(comment, "审核说明", REVIEW_COMMENT_MAX_LEN)?,
            ),
        };
        self.status = status;
        self.reviewed_at = Some(reviewed_at);
        self.reviewed_by = Some(reviewed_by);
        self.review_reason_code = reason_code;
        self.review_comment = comment;
        Ok(())
    }

    /// 校验提交仍处于待审核状态。
    ///
    /// # 返回
    /// 待审核状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 提交已处理、失效或仍是草稿时返回领域错误。
    pub fn ensure_pending(&self) -> Result<()> {
        if self.status != SubmissionStatus::Pending {
            return Err(Error::from("提交已审核或已失效，请勿重复生效"));
        }
        Ok(())
    }

    /// 判断审核人与提交人是否满足职责分离。
    ///
    /// # 参数
    /// * `reviewer_id` - 当前审核人账号 ID
    ///
    /// # 返回
    /// 审核人与已记录提交人不同时返回 `true`；未记录提交人也返回 `true`。
    pub fn reviewer_is_separated(&self, reviewer_id: &str) -> bool {
        self.submitted_by.as_deref() != Some(reviewer_id)
    }

    /// 返回当前提交在对象中心使用的内容来源代码。
    ///
    /// # 返回
    /// 草稿返回 `DRAFT`，其他不可变提交状态返回 `SUBMISSION`。
    pub fn content_source(&self) -> &'static str {
        if self.status == SubmissionStatus::Draft {
            "DRAFT"
        } else {
            "SUBMISSION"
        }
    }

    /// 校验提交表头金额等于冻结明细汇总。
    ///
    /// # 参数
    /// * `lines` - 属于本提交的冻结明细
    ///
    /// # 返回
    /// 三项表头金额均与逐行汇总一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一行不属于本提交，或含税、不含税、税额任一汇总不一致时返回领域错误。
    pub fn ensure_line_totals(&self, lines: &[PurchaseOrderSubmissionLine]) -> Result<()> {
        let mut gross = Amount::try_from(rust_decimal::Decimal::ZERO).expect("零金额合法");
        let mut net = gross;
        let mut tax = gross;
        for line in lines {
            if line.purchase_order_submission_id.as_ref() != self.base.id {
                return Err(Error::from("采购提交明细不属于当前提交"));
            }
            gross = gross.checked_add(line.gross_amount);
            net = net.checked_add(line.net_amount);
            tax = tax.checked_add(line.tax_amount);
        }
        if gross != self.gross_amount || net != self.net_amount || tax != self.tax_amount {
            return Err(Error::from("采购提交表头金额与冻结明细汇总不一致"));
        }
        Ok(())
    }

    /// 标记因重新提交失效（§6.6：修改内容必须新建提交并使旧复核失效）。
    ///
    /// # 返回
    /// 标记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 提交已通过并已形成正式事实时不允许失效，返回错误（由 P3 校验下游事实）。
    pub fn mark_superseded(&mut self) -> Result<()> {
        if self.status == SubmissionStatus::Approved {
            return Err(Error::from("已通过的提交不得标记失效"));
        }
        self.status = SubmissionStatus::Superseded;
        Ok(())
    }

    /// 校验当前状态为草稿。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿状态时返回错误。
    pub fn ensure_draft(&self) -> Result<()> {
        if self.status != SubmissionStatus::Draft {
            return Err(Error::from("只有草稿状态的提交可以编辑"));
        }
        Ok(())
    }
}

/// 采购提交行创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionLineData {
    /// 所属提交。
    pub purchase_order_submission_id: PurchaseOrderSubmissionId,
    /// 行号（从 1 递增）。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU；物流费用行为空。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本；物流费用行为空。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 商品名称快照；物流费用行为空。
    pub product_name_snapshot: Option<String>,
    /// 规格快照；物流费用行为空。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量；物流费用行为空。
    pub quantity: Option<Quantity>,
    /// 单位代码；物流费用行为空。
    pub base_unit_code: Option<String>,
    /// 含税采购单价；物流费用行为空。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 预计交期。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<SalesOrderRevisionLineId>,
    /// 商品行对应的历史销售提交行；仅保留旧流程追溯。
    pub sales_order_submission_line_id: Option<SalesOrderSubmissionLineId>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<Quantity>,
}

impl PurchaseLineDataRef for PurchaseOrderSubmissionLineData {
    fn line_type(&self) -> PurchaseLineType {
        self.line_type
    }

    fn procurement_confirmation_line_id(&self) -> &Option<ProcurementConfirmationLineId> {
        &self.procurement_confirmation_line_id
    }

    fn sku_id(&self) -> &Option<SkuId> {
        &self.sku_id
    }

    fn product_name_snapshot(&self) -> &Option<String> {
        &self.product_name_snapshot
    }

    fn specification_snapshot(&self) -> &Option<String> {
        &self.specification_snapshot
    }

    fn quantity(&self) -> Option<Quantity> {
        self.quantity
    }

    fn base_unit_code(&self) -> &Option<String> {
        &self.base_unit_code
    }

    fn unit_cost_gross(&self) -> Option<UnitPrice> {
        self.unit_cost_gross
    }

    fn gross_amount(&self) -> Amount {
        self.gross_amount
    }

    fn net_amount(&self) -> Amount {
        self.net_amount
    }

    fn tax_amount(&self) -> Amount {
        self.tax_amount
    }

    fn input_tax_rate(&self) -> Option<Rate> {
        self.input_tax_rate
    }

    fn ensure_allocation(&self) -> Result<()> {
        match self.line_type {
            PurchaseLineType::ItemService => {
                if self.sales_order_line_id.is_none() || self.sales_order_revision_line_id.is_none() {
                    return Err(Error::from("商品/服务行必须引用销售稳定行与当前版本行"));
                }
                let quantity = self.allocated_quantity.ok_or("商品/服务行必须填写分配数量")?;
                if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
                    return Err(Error::from("商品/服务行分配数量必须为正"));
                }
            }
            PurchaseLineType::LogisticsFee => {
                if self.sales_order_line_id.is_some()
                    || self.sales_order_revision_line_id.is_some()
                    || self.sales_order_submission_line_id.is_some()
                    || self.allocated_quantity.is_some()
                {
                    return Err(Error::from("物流费用行不得携带销售分配"));
                }
            }
        }
        Ok(())
    }
}

/// 采购提交行实体（数据模型 §6.6）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属提交。
    pub purchase_order_submission_id: PurchaseOrderSubmissionId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 商品名称快照。
    pub product_name_snapshot: Option<String>,
    /// 规格快照。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量。
    pub quantity: Option<Quantity>,
    /// 单位代码。
    pub base_unit_code: Option<String>,
    /// 含税采购单价。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 预计交期。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<SalesOrderRevisionLineId>,
    /// 商品行对应的历史销售提交行；仅保留旧流程追溯。
    pub sales_order_submission_line_id: Option<SalesOrderSubmissionLineId>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<Quantity>,
}

impl PurchaseOrderSubmissionLine {
    /// 创建采购提交行。
    ///
    /// 完成快照文本的规范化，并按行类型强制字段归属与金额三元组守恒（§6.6）；
    /// 商品行必须携带销售提交行引用与分配数量。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseOrderSubmissionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交行实体。
    ///
    /// # 错误
    /// 行号为零、字段归属与行类型不符、快照超长、数量/单价/税率越界或
    /// 金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseOrderSubmissionLineId, data: PurchaseOrderSubmissionLineData) -> Result<Self> {
        ensure_line_no(data.line_no)?;
        let (product_name, specification, base_unit_code) = normalize_and_validate_line(&data)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_order_submission_id: data.purchase_order_submission_id,
            line_no: data.line_no,
            line_type: data.line_type,
            procurement_confirmation_line_id: data.procurement_confirmation_line_id,
            sku_id: data.sku_id.clone(),
            sku_revision_id: data.sku_revision_id,
            product_name_snapshot: product_name,
            specification_snapshot: specification,
            quantity: data.quantity,
            base_unit_code,
            unit_cost_gross: data.unit_cost_gross,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            input_tax_rate: data.input_tax_rate,
            expected_delivery_date: data.expected_delivery_date,
            sales_order_line_id: data.sales_order_line_id,
            sales_order_revision_line_id: data.sales_order_revision_line_id,
            sales_order_submission_line_id: data.sales_order_submission_line_id,
            allocated_quantity: data.allocated_quantity,
        })
    }

    /// 把草稿行复制到新的冻结提交。
    ///
    /// # 参数
    /// * `id` - 新提交行稳定身份
    /// * `submission_id` - 新正式提交稳定身份
    /// * `draft_line` - 当前草稿行
    ///
    /// # 返回
    /// 返回业务内容与草稿行一致、重新挂接到正式提交的新行。
    ///
    /// # 错误
    /// 草稿行本身不满足当前采购行不变式时返回领域错误。
    pub fn freeze_from_draft(
        id: PurchaseOrderSubmissionLineId,
        submission_id: PurchaseOrderSubmissionId,
        draft_line: &Self,
    ) -> Result<Self> {
        Self::new(
            id,
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: submission_id,
                line_no: draft_line.line_no,
                line_type: draft_line.line_type,
                procurement_confirmation_line_id: draft_line.procurement_confirmation_line_id.clone(),
                sku_id: draft_line.sku_id.clone(),
                sku_revision_id: draft_line.sku_revision_id.clone(),
                product_name_snapshot: draft_line.product_name_snapshot.clone(),
                specification_snapshot: draft_line.specification_snapshot.clone(),
                quantity: draft_line.quantity,
                base_unit_code: draft_line.base_unit_code.clone(),
                unit_cost_gross: draft_line.unit_cost_gross,
                gross_amount: draft_line.gross_amount,
                net_amount: draft_line.net_amount,
                tax_amount: draft_line.tax_amount,
                input_tax_rate: draft_line.input_tax_rate,
                expected_delivery_date: draft_line.expected_delivery_date,
                sales_order_line_id: draft_line.sales_order_line_id.clone(),
                sales_order_revision_line_id: draft_line.sales_order_revision_line_id.clone(),
                sales_order_submission_line_id: draft_line.sales_order_submission_line_id.clone(),
                allocated_quantity: draft_line.allocated_quantity,
            },
        )
    }
}

/// 解析带固定前缀的十进制序号。
///
/// # 参数
/// * `value` - 完整编号
/// * `prefix` - 固定编号前缀
///
/// # 返回
/// 编号匹配前缀且后缀可解析为 `u32` 时返回序号，否则返回 `None`。
fn parse_sequence(value: &str, prefix: &str) -> Option<u32> {
    value.strip_prefix(prefix)?.parse().ok()
}

/// 校验行号从 1 开始。
///
/// # 参数
/// * `line_no` - 行号
///
/// # 错误
/// 行号为零时返回错误。
fn ensure_line_no(line_no: u32) -> Result<()> {
    if line_no == 0 {
        return Err(Error::from("行号必须从 1 开始"));
    }
    Ok(())
}

/// 校验表头金额三元组守恒。
///
/// # 参数
/// * `gross_amount` / `net_amount` / `tax_amount` - 表头汇总
/// * `context` - 错误提示中的上下文（如提交序号）
///
/// # 错误
/// `gross ≠ net + tax` 或任一分量为负时返回错误。
fn ensure_header_triple(
    gross_amount: Amount,
    net_amount: Amount,
    tax_amount: Amount,
    context: &str,
) -> Result<()> {
    if gross_amount.to_decimal() != net_amount.to_decimal() + tax_amount.to_decimal()
        || gross_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || net_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || tax_amount.to_decimal() < rust_decimal::Decimal::ZERO
    {
        return Err(Error::from(format!("提交表头金额三元组不守恒（{context}）")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PurchaseOrderReviewDecision, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
        PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, PurchaseOrderSubmissionUpdate,
        SubmissionStatus,
    };
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{
        ProcurementConfirmationLineId, PurchaseOrderId, PurchaseOrderSubmissionId,
        PurchaseOrderSubmissionLineId, SalesOrderLineId, SalesOrderRevisionLineId,
        SalesOrderSubmissionLineId, SkuId, SupplierAccountId, SupplierCommercialProfileRevisionId,
    };
    use crate::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
    use crate::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
    use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseLineType, PurchaseType};
    use std::str::FromStr;

    fn snapshot() -> SupplierSnapshot {
        SupplierSnapshot::new("北京华联供应商".to_string()).unwrap()
    }

    fn payment_term() -> PaymentTermSnapshot {
        PaymentTermSnapshot::new("NET-30".to_string(), false, None, None).unwrap()
    }

    fn submission_data() -> PurchaseOrderSubmissionData {
        PurchaseOrderSubmissionData {
            purchase_order_id: PurchaseOrderId::new("po-1"),
            submission_no: " SUB-01 ".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
            supplier_snapshot: snapshot(),
            payment_term_snapshot: payment_term(),
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
        }
    }

    fn goods_line_data() -> PurchaseOrderSubmissionLineData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: PurchaseOrderSubmissionId::new("sub-1"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(crate::ids::SkuRevisionId::new("skur-1")),
            product_name_snapshot: Some(" 慰问礼包 ".to_string()),
            specification_snapshot: Some(" 500g×2 ".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some(" 箱 ".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("ssl-1")),
            allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
        }
    }

    #[test]
    fn submission_new_trims_and_validates_header_triple() {
        let submission =
            PurchaseOrderSubmission::new(PurchaseOrderSubmissionId::new("sub-1"), submission_data()).unwrap();
        assert_eq!(submission.submission_no, "SUB-01");
        assert_eq!(submission.status, SubmissionStatus::Draft);
        assert!(submission.submitted_at.is_none());

        let inconsistent = PurchaseOrderSubmissionData {
            gross_amount: Amount::from_str("29.98").unwrap(),
            ..submission_data()
        };
        assert!(PurchaseOrderSubmission::new(PurchaseOrderSubmissionId::new("sub-2"), inconsistent).is_err());
    }

    #[test]
    fn submission_derives_sequence_freezes_draft_and_checks_review_invariants() {
        let draft =
            PurchaseOrderSubmission::new(PurchaseOrderSubmissionId::new("draft-1"), submission_data())
                .unwrap();
        let mut numbered = draft.clone();
        numbered.submission_no = "SUB-000009".to_string();
        assert_eq!(
            PurchaseOrderSubmission::next_submission_no(std::slice::from_ref(&numbered)).unwrap(),
            "SUB-000010"
        );
        let formal = PurchaseOrderSubmission::freeze_from_draft(
            PurchaseOrderSubmissionId::new("sub-10"),
            "SUB-000010".to_string(),
            &draft,
            Instant::from_unix_secs(1_700_000_000),
            "buyer-1",
        )
        .unwrap();
        assert_eq!(formal.status, SubmissionStatus::Pending);
        assert_eq!(formal.content_source(), "SUBMISSION");
        assert!(!formal.reviewer_is_separated("buyer-1"));
        assert!(formal.reviewer_is_separated("finance-1"));
        formal.ensure_pending().unwrap();
    }

    #[test]
    fn submission_line_freeze_and_header_totals_are_checked_together() {
        let draft =
            PurchaseOrderSubmission::new(PurchaseOrderSubmissionId::new("draft-1"), submission_data())
                .unwrap();
        let source =
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("line-1"), goods_line_data())
                .unwrap();
        let frozen = PurchaseOrderSubmissionLine::freeze_from_draft(
            PurchaseOrderSubmissionLineId::new("line-2"),
            PurchaseOrderSubmissionId::new("draft-1"),
            &source,
        )
        .unwrap();
        draft.ensure_line_totals(&[frozen]).unwrap();

        let foreign = PurchaseOrderSubmissionLine::freeze_from_draft(
            PurchaseOrderSubmissionLineId::new("line-3"),
            PurchaseOrderSubmissionId::new("other"),
            &source,
        )
        .unwrap();
        assert!(draft.ensure_line_totals(&[foreign]).is_err());
    }

    #[test]
    fn submission_submit_review_and_supersede_lifecycle() {
        let mut submission =
            PurchaseOrderSubmission::new(PurchaseOrderSubmissionId::new("sub-1"), submission_data()).unwrap();
        submission
            .submit(Instant::from_unix_secs(1_700_000_000), " buyer-1 ")
            .unwrap();
        assert_eq!(submission.status, SubmissionStatus::Pending);
        assert_eq!(submission.submitted_by.as_deref(), Some("buyer-1"));

        assert!(
            submission
                .update(PurchaseOrderSubmissionUpdate::default())
                .is_err(),
            "待审核提交冻结"
        );

        submission
            .record_review(
                PurchaseOrderReviewDecision::Approved {
                    comment: Some(" 金额核对无误 ".to_string()),
                },
                Instant::from_unix_secs(1_700_000_100),
                "finance-1",
            )
            .unwrap();
        assert_eq!(submission.status, SubmissionStatus::Approved);
        assert_eq!(submission.reviewed_by.as_deref(), Some("finance-1"));
        assert_eq!(submission.review_comment.as_deref(), Some("金额核对无误"));
        assert!(submission.review_reason_code.is_none());
        assert!(submission.mark_superseded().is_err(), "已通过提交不得失效");

        let mut rejected =
            PurchaseOrderSubmission::new(PurchaseOrderSubmissionId::new("sub-2"), submission_data()).unwrap();
        rejected
            .submit(Instant::from_unix_secs(1_700_000_000), "buyer-1")
            .unwrap();
        rejected
            .record_review(
                PurchaseOrderReviewDecision::Rejected {
                    reason_code: " COST_OR_TAX ".to_string(),
                    comment: Some(" 税率需复核 ".to_string()),
                },
                Instant::from_unix_secs(1_700_000_100),
                "finance-2",
            )
            .unwrap();
        assert_eq!(rejected.status, SubmissionStatus::Rejected);
        assert_eq!(rejected.review_reason_code.as_deref(), Some("COST_OR_TAX"));
        assert_eq!(rejected.review_comment.as_deref(), Some("税率需复核"));
        rejected.mark_superseded().unwrap();
        assert_eq!(rejected.status, SubmissionStatus::Superseded);
    }

    #[test]
    fn rejected_review_requires_structured_reason_and_terminal_review_cannot_repeat() {
        let mut submission =
            PurchaseOrderSubmission::new(PurchaseOrderSubmissionId::new("sub-1"), submission_data()).unwrap();
        submission
            .submit(Instant::from_unix_secs(1_700_000_000), "buyer-1")
            .unwrap();

        assert!(submission
            .record_review(
                PurchaseOrderReviewDecision::Rejected {
                    reason_code: "   ".to_string(),
                    comment: None,
                },
                Instant::from_unix_secs(1_700_000_100),
                "finance-1",
            )
            .is_err());
        assert_eq!(submission.status, SubmissionStatus::Pending);

        submission
            .record_review(
                PurchaseOrderReviewDecision::Rejected {
                    reason_code: "OTHER".to_string(),
                    comment: None,
                },
                Instant::from_unix_secs(1_700_000_100),
                "finance-1",
            )
            .unwrap();
        assert!(submission
            .record_review(
                PurchaseOrderReviewDecision::Approved { comment: None },
                Instant::from_unix_secs(1_700_000_200),
                "finance-1",
            )
            .is_err());
    }

    #[test]
    fn submission_line_goods_happy_path() {
        let line =
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-1"), goods_line_data())
                .unwrap();
        assert_eq!(line.product_name_snapshot.as_deref(), Some("慰问礼包"));
        assert_eq!(line.base_unit_code.as_deref(), Some("箱"));
        assert_eq!(line.line_type, PurchaseLineType::ItemService);
    }

    #[test]
    fn submission_line_logistics_fee_amounts_consistent() {
        let gross = Amount::from_str("50.00").unwrap();
        let tax = Amount::from_str("6.50").unwrap();
        let net = Amount::from_str("43.50").unwrap();
        let data = PurchaseOrderSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
            ..goods_line_data()
        };
        let line =
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-2"), data).unwrap();
        assert_eq!(line.quantity, None);
        assert_eq!(line.gross_amount, gross);
    }

    #[test]
    fn submission_line_rejects_failures() {
        // 商品行缺少销售当前版本行
        let no_allocation = PurchaseOrderSubmissionLineData {
            sales_order_revision_line_id: None,
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-3"), no_allocation,)
                .is_err()
        );

        // 物流费用行携带 SKU
        let fee_with_sku = PurchaseOrderSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            sku_id: Some(SkuId::new("sku-1")),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-4"), fee_with_sku)
                .is_err()
        );

        // 商品行金额三元组不守恒
        let bad_amounts = PurchaseOrderSubmissionLineData {
            gross_amount: Amount::from_str("30.00").unwrap(),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-5"), bad_amounts)
                .is_err()
        );

        // 行号为零
        let zero_line = PurchaseOrderSubmissionLineData {
            line_no: 0,
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-6"), zero_line).is_err()
        );

        // 超长规格快照
        let overlong = PurchaseOrderSubmissionLineData {
            specification_snapshot: Some("s".repeat(513)),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-7"), overlong).is_err()
        );
    }
}
