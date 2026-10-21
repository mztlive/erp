//! 询价任务完成后，费用采购按现有销售来源资格取得最小办理上下文。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::money::{Amount, Rate};
use erp_identity::{Permission, SharedRbacService};
use erp_read_models::purchase_center::expense_source::ExpenseSourceReadService;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::Result as SalesResult;
use erp_sales::entity::sales_inquiry::{
    InquiryFee, InquiryFeeAllocation, InquiryFeeConfirmation, SalesInquiry,
};
use erp_sales::repository::sales_order::inquiry_source::inquiry_sources;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_supply::inquiry::{InquiryFeeBasis, InquirySupplyService};
use mongodb::Database;
use persistence_core::NoTransaction;
use serde::Serialize;

use super::access::SalesInquiryAccess;
use crate::sales_inquiry_expense::category_code;
use crate::{Error, Result};

/// 已授权采购为一张生效销售选择询价费用时的最小候选。
#[derive(Debug, Serialize)]
pub struct InquiryExpenseCandidate {
    pub inquiry_id: String,
    pub inquiry_no: String,
    pub feedback_round: u32,
    pub fee_id: String,
    pub allocation_id: String,
    pub name: String,
    pub category: &'static str,
    pub tax_rate: Rate,
    pub available_amount: Amount,
    pub valid_until: Instant,
    pub valid_conditions: String,
    pub unavailable_reason: Option<String>,
    pub payee_supplier_id: Option<String>,
    pub payment_term_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InquiryExpenseCandidates {
    pub items: Vec<InquiryExpenseCandidate>,
}

/// 复用费用采购来源授权，不恢复已经完成的询价任务。
pub struct InquiryExpenseReadService {
    db: Database,
    rbac: SharedRbacService,
}

impl InquiryExpenseReadService {
    /// 绑定现有采购来源与当前权限服务。
    /// # 参数
    /// `db` 为业务库，`rbac` 为当前权限源。
    /// # 返回
    /// 不缓存任务或销售范围的读取器。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 获取当前销售明确分摊的费用、余额与必要付款依据。
    /// # 参数
    /// `sales_id` 为来源销售，`actor` 为具备采购创建及费用转换资格的当前账号。
    /// # 返回
    /// 仅含本销售的询价费用；无来源或未明确分摊时返回空集合。
    /// # 错误
    /// 当前账号、费用采购来源范围、询价链或期间版本失效时拒绝。
    pub async fn candidates(&self, sales_id: &str, actor: &AuditActor) -> Result<InquiryExpenseCandidates> {
        let access = SalesInquiryAccess::new(self.db.clone(), self.rbac.clone());
        access.ensure_procurement_permission(actor, "convert_expenses", &mut NoTransaction).await?;
        let sources = ExpenseSourceReadService::new(self.db.clone(), self.rbac.clone());
        let order = sources.require_order(sales_id, actor, &mut NoTransaction).await?;
        let Some(source) = inquiry_sources(&self.db).find_by_id(sales_id, &mut NoTransaction).await? else {
            return Ok(InquiryExpenseCandidates { items: vec![] });
        };
        let inquiry =
            SalesInquiryService::new(self.db.clone()).require(&source.inquiry_id, &mut NoTransaction).await?;
        if !source.consumed
            || source.released
            || order.customer_id.as_ref() != inquiry.customer_id
            || source.customer_id != order.customer_id
        {
            return Err(Error::NotFound("询价费用来源不存在或不适用".into()));
        }
        let Some(round) = inquiry.acceptances.last().map(|acceptance| acceptance.feedback_round) else {
            return Ok(InquiryExpenseCandidates { items: vec![] });
        };
        let feedback = inquiry
            .feedbacks
            .iter()
            .find(|feedback| feedback.round == round)
            .ok_or_else(|| Error::Internal("询价接受结果缺少费用快照".into()))?;
        let bases = InquirySupplyService::new(self.db.clone())
            .fee_bases(&inquiry.base.id, feedback.requirement_version, round, &mut NoTransaction)
            .await?;
        let items = build_candidates(
            &inquiry,
            sales_id,
            round,
            &bases,
            Instant::now(),
            FeeHandlingMode::Procurement,
        )?;
        sources.require_order(sales_id, actor, &mut NoTransaction).await?;
        access.ensure_procurement_permission(actor, "convert_expenses", &mut NoTransaction).await?;
        let latest =
            SalesInquiryService::new(self.db.clone()).require(&source.inquiry_id, &mut NoTransaction).await?;
        if latest.base.version != inquiry.base.version {
            return Err(Error::ConflictError("询价费用或可用余额已变化，请重新查询".into()));
        }
        Ok(InquiryExpenseCandidates { items })
    }
    /// 按财务附加费用登记资格取得本销售的 ERP 外费用来源。
    /// # 参数
    /// `sales_id` 为生效来源销售，`actor` 为具有费用登记和既有销售范围的账号。
    /// # 返回
    /// 仅含公开费用身份、金额、税率和办理原因，不含供应商及付款资料。
    /// # 错误
    /// 当前账号、费用登记权限、销售范围或来源版本失效时拒绝。
    pub async fn financial_candidates(
        &self,
        sales_id: &str,
        actor: &AuditActor,
    ) -> Result<FinancialInquiryExpenseCandidates> {
        let access = SalesAccess::new(self.db.clone(), self.rbac.clone());
        let permissions = [Permission::parse("cost_entry:create")?];
        let order =
            access.require_object(actor, "detail", sales_id, &permissions, &mut NoTransaction).await?;
        order.ensure_extra_cost_source()?;
        let Some(source) = inquiry_sources(&self.db).find_by_id(sales_id, &mut NoTransaction).await? else {
            return Ok(FinancialInquiryExpenseCandidates { items: vec![] });
        };
        let inquiry =
            SalesInquiryService::new(self.db.clone()).require(&source.inquiry_id, &mut NoTransaction).await?;
        if !source.consumed
            || source.released
            || source.customer_id != order.customer_id
            || inquiry.customer_id != order.customer_id.as_ref()
        {
            return Err(Error::NotFound("询价费用来源不存在或不适用".into()));
        }
        let Some(round) = inquiry.acceptances.last().map(|acceptance| acceptance.feedback_round) else {
            return Ok(FinancialInquiryExpenseCandidates { items: vec![] });
        };
        let items =
            build_candidates(&inquiry, sales_id, round, &[], Instant::now(), FeeHandlingMode::Finance)?
                .into_iter()
                .map(FinancialInquiryExpenseCandidate::from)
                .collect();
        access.require_object(actor, "detail", sales_id, &permissions, &mut NoTransaction).await?;
        let latest =
            SalesInquiryService::new(self.db.clone()).require(&source.inquiry_id, &mut NoTransaction).await?;
        if latest.base.version != inquiry.base.version {
            return Err(Error::ConflictError("询价费用或可用余额已变化，请重新查询".into()));
        }
        Ok(FinancialInquiryExpenseCandidates { items })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FeeHandlingMode {
    Procurement,
    Finance,
}

fn build_candidates(
    inquiry: &SalesInquiry,
    sales_id: &str,
    round: u32,
    bases: &[InquiryFeeBasis],
    now: Instant,
    mode: FeeHandlingMode,
) -> Result<Vec<InquiryExpenseCandidate>> {
    let feedback = inquiry
        .feedbacks
        .iter()
        .find(|feedback| feedback.round == round)
        .ok_or_else(|| Error::Internal("询价费用快照缺失".into()))?;
    let mut items = Vec::new();
    for allocation in
        inquiry.fee_allocations.iter().filter(|allocation| allocation.sales_order_id == sales_id)
    {
        let Some(fee) = feedback.data.fees.iter().find(|fee| fee.fee_id == allocation.fee_id) else {
            continue;
        };
        let basis = bases.iter().find(|basis| basis.data.fee_id == fee.fee_id);
        items.push(fee_candidate(inquiry, allocation, fee, round, basis, now, mode));
    }
    Ok(items)
}

fn fee_candidate(
    inquiry: &SalesInquiry,
    allocation: &InquiryFeeAllocation,
    fee: &InquiryFee,
    round: u32,
    basis: Option<&InquiryFeeBasis>,
    now: Instant,
    mode: FeeHandlingMode,
) -> InquiryExpenseCandidate {
    let balance = inquiry.fee_available_amount(&fee.fee_id, &allocation.allocation_id, round, now);
    let reason = candidate_reason(fee, &balance, mode);
    InquiryExpenseCandidate {
        inquiry_id: inquiry.base.id.clone(),
        inquiry_no: inquiry.inquiry_no.clone(),
        feedback_round: round,
        fee_id: fee.fee_id.clone(),
        allocation_id: allocation.allocation_id.clone(),
        name: fee.name.clone(),
        category: category_code(fee),
        tax_rate: fee.tax_rate,
        available_amount: balance.unwrap_or_else(|_| Amount::zero()),
        valid_until: fee.valid_until,
        valid_conditions: fee.valid_conditions.clone(),
        unavailable_reason: reason,
        payee_supplier_id: basis.map(|basis| basis.data.payee_supplier_id.to_string()),
        payment_term_code: basis.map(|basis| basis.data.payment_term_code.clone()),
    }
}

fn candidate_reason(
    fee: &InquiryFee,
    balance: &SalesResult<Amount>,
    mode: FeeHandlingMode,
) -> Option<String> {
    if fee.confirmation != InquiryFeeConfirmation::Confirmed {
        return Some("费用仍为暂估，请先确认费用并由销售接受".into());
    }
    if fee.included_in_goods_price {
        return Some("本费用已含商品供给价，不能重复办理".into());
    }
    if mode == FeeHandlingMode::Procurement && !fee.requires_erp_payment {
        return Some("本费用应按财务凭证登记，不生成费用采购".into());
    }
    if mode == FeeHandlingMode::Finance && fee.requires_erp_payment {
        return Some("本费用须通过 ERP 费用采购办理，不能直接登记".into());
    }
    match balance {
        Err(error) => Some(error.to_string()),
        Ok(amount) if *amount <= Amount::zero() => Some("本费用分摊已无可转额度".into()),
        Ok(_) => None,
    }
}

/// 财务登记费用来源的独立公开字段，不含采购收款对象及付款条款。
#[derive(Debug, Serialize)]
pub struct FinancialInquiryExpenseCandidate {
    pub inquiry_id: String,
    pub inquiry_no: String,
    pub feedback_round: u32,
    pub fee_id: String,
    pub allocation_id: String,
    pub name: String,
    pub category: &'static str,
    pub tax_rate: Rate,
    pub available_amount: Amount,
    pub valid_until: Instant,
    pub valid_conditions: String,
    pub unavailable_reason: Option<String>,
}
impl From<InquiryExpenseCandidate> for FinancialInquiryExpenseCandidate {
    fn from(value: InquiryExpenseCandidate) -> Self {
        Self {
            inquiry_id: value.inquiry_id,
            inquiry_no: value.inquiry_no,
            feedback_round: value.feedback_round,
            fee_id: value.fee_id,
            allocation_id: value.allocation_id,
            name: value.name,
            category: value.category,
            tax_rate: value.tax_rate,
            available_amount: value.available_amount,
            valid_until: value.valid_until,
            valid_conditions: value.valid_conditions,
            unavailable_reason: value.unavailable_reason,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct FinancialInquiryExpenseCandidates {
    pub items: Vec<FinancialInquiryExpenseCandidate>,
}

#[cfg(test)]
mod tests {
    use erp_sales::entity::sales_inquiry::InquiryFeeCategory;

    use super::*;
    fn fee() -> InquiryFee {
        InquiryFee {
            fee_id: "fee".into(),
            category: InquiryFeeCategory::Logistics,
            name: "运费".into(),
            amount_gross: "10.00".parse().unwrap(),
            tax_rate: "0.06".parse().unwrap(),
            line_ids: vec![],
            allocation_ids: vec![],
            included_in_goods_price: false,
            confirmation: InquiryFeeConfirmation::Confirmed,
            requires_erp_payment: true,
            valid_until: Instant::from_unix_secs(100),
            valid_conditions: String::new(),
            public_note: String::new(),
        }
    }
    #[test]
    fn candidate_channels_keep_procurement_and_finance_handling_exclusive() {
        let mut fee = fee();
        let balance = Ok("10.00".parse().unwrap());
        assert!(candidate_reason(&fee, &balance, FeeHandlingMode::Procurement).is_none());
        assert!(candidate_reason(&fee, &balance, FeeHandlingMode::Finance).is_some());
        fee.requires_erp_payment = false;
        assert!(candidate_reason(&fee, &balance, FeeHandlingMode::Finance).is_none());
        assert!(candidate_reason(&fee, &balance, FeeHandlingMode::Procurement).is_some());
        fee.confirmation = InquiryFeeConfirmation::Estimated;
        assert!(candidate_reason(&fee, &balance, FeeHandlingMode::Finance).is_some());
    }
    #[test]
    fn financial_response_does_not_serialize_internal_payment_basis() {
        let value = InquiryExpenseCandidate {
            inquiry_id: "i".into(),
            inquiry_no: "IQ01".into(),
            feedback_round: 1,
            fee_id: "f".into(),
            allocation_id: "a".into(),
            name: "运费".into(),
            category: "logistics",
            tax_rate: "0.06".parse().unwrap(),
            available_amount: "10.00".parse().unwrap(),
            valid_until: Instant::from_unix_secs(100),
            valid_conditions: String::new(),
            unavailable_reason: None,
            payee_supplier_id: Some("internal-supplier".into()),
            payment_term_code: Some("internal-payment".into()),
        };
        let response = serde_json::to_value(FinancialInquiryExpenseCandidate::from(value)).unwrap();
        assert!(response.get("payee_supplier_id").is_none());
        assert!(response.get("payment_term_code").is_none());
        assert_eq!(response["available_amount"], "10.00");
    }
}
