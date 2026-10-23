//! 采购内部资料按当前责任与精确需求版本保存，销售回执不包含内部字段。

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    BusinessDocumentId, DocumentAttachmentId, FileAssetId, SkuId, SupplierAccountId, SupplierOfferingId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use erp_procurement::entity::purchase_order::PurchaseInquiryAddress;
use erp_sales::entity::sales_inquiry::{InquiryRequirementLine, SalesInquiry};
use erp_supply::inquiry::{
    InquiryFeeBasis, InquiryFeeBasisData, InquiryQuote, InquiryQuoteData, InquiryQuoteScope,
    InquirySupplyService, QuoteAddressLimit, QuoteTerms,
};
use erp_support::entity::file_asset::{AttachmentUsage, DocumentAttachment, DocumentAttachmentData};
use erp_support::repository::{DocumentAttachmentRepositoryExt, FileAssetExt};
use erp_workflow::entity::work_item::WorkItem;
use id_generator::next_id;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::SalesInquiryProcess;
use super::dto::InquiryMutationView;
use super::execution::InquiryCommand;
use crate::adapters::supplier_access;
use crate::supply_governance::offering::MongoOfferingQualification;
use crate::{Error, Result};

/// 客户端只表达采购内部商业条件，不可伪造需求版本、登记人或SKU。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryQuoteInput {
    pub quote_id: Option<String>,
    pub revision_no: u32,
    pub line_id: String,
    pub supplier_id: SupplierAccountId,
    pub supplier_offering_id: SupplierOfferingId,
    pub terms: InquiryQuoteTermsInput,
    pub internal_note: String,
}

/// 采购可录入的报价条件；配送范围由需求生成，客户端不得指定。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryQuoteTermsInput {
    #[serde(default)]
    pub unavailable: bool,
    #[serde(default)]
    pub freight_gross: Option<Amount>,
    pub unit_cost_gross: UnitPrice,
    pub input_tax_rate: Rate,
    pub available_quantity: Quantity,
    pub expected_delivery_date: BusinessDate,
    #[serde(default, skip_serializing)]
    pub valid_to: Option<BusinessDate>,
}

impl InquiryQuoteTermsInput {
    pub(super) fn for_requirement(&self, line: &InquiryRequirementLine) -> Result<QuoteTerms> {
        let allocation_ids = line.allocations.iter().map(|row| row.allocation_id.clone()).collect::<Vec<_>>();
        if !self.unavailable {
            line.validate_quote_scope(self.available_quantity, &allocation_ids)?;
        }
        let terms = QuoteTerms {
            unavailable: self.unavailable,
            freight_gross: self.freight_gross,
            unit_cost_gross: self.unit_cost_gross,
            input_tax_rate: self.input_tax_rate,
            available_quantity: self.available_quantity,
            expected_delivery_date: self.expected_delivery_date,
            valid_to: None,
            address_limits: line
                .allocations
                .iter()
                .map(|row| QuoteAddressLimit {
                    allocation_id: row.allocation_id.clone(),
                    address_fingerprint: PurchaseInquiryAddress::fingerprint(
                        &row.recipient,
                        &row.phone,
                        row.region_code.as_str(),
                        &row.address,
                    ),
                    quantity: row.quantity,
                })
                .collect(),
        };
        Ok(terms.validated()?)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInquiryQuoteCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub task_id: String,
    pub task_version: u64,
    pub quote: InquiryQuoteInput,
}

/// 费用公开金额单独保存，本输入只补充采购内部付款依据。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryFeeBasisInput {
    pub fee_id: String,
    pub payee_supplier_id: SupplierAccountId,
    pub payment_term_code: String,
    pub internal_note: String,
    pub attachment_ids: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInquiryFeeBasisCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub task_id: String,
    pub task_version: u64,
    pub basis: InquiryFeeBasisInput,
}

impl SalesInquiryProcess {
    /// 为当前责任需求追加不可变供应商报价版本。
    ///
    /// # 参数
    /// `actor` 为当前采购，`id` 为询价，`req` 为版本、任务及内部条件。
    /// # 返回
    /// 同事务写入询价CAS、报价、任务活动、审计和安全回执后的结果。
    /// # 错误
    /// 任务、SKU、地址、供应身份或商业条件无效时拒绝。
    pub async fn save_internal_quote(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SaveInquiryQuoteCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::InternalQuote(Box::new(req))).await
    }

    /// 为当前责任公开费用保存独立采购内部收款依据。
    ///
    /// # 参数
    /// `actor` 为当前采购，`id` 为询价，`req` 为任务及内部费用依据。
    /// # 返回
    /// 不产生采购、应付或付款授权的安全命令结果。
    /// # 错误
    /// 费用不属于当前工作、供应商不可见、付款条件或附件无效时拒绝。
    pub async fn save_fee_basis(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SaveInquiryFeeBasisCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::FeeBasis(req)).await
    }

    pub(super) async fn append_internal_quote(
        &self,
        actor: &AuditActor,
        inquiry: &SalesInquiry,
        task: &WorkItem,
        req: &SaveInquiryQuoteCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        inquiry.ensure_processing(req.expected_version)?;
        let line = inquiry.responsibility_line(task.responsibility_scope_ids(), &req.quote.line_id)?;
        supplier_access(self.db.clone(), self.rbac.clone())
            .require_with(actor, "detail", req.quote.supplier_id.as_ref(), executor)
            .await?;
        let terms = req.quote.terms.for_requirement(line)?;
        let id = next_id();
        let quote = InquiryQuote::new(
            id.clone(),
            InquiryQuoteData {
                quote_id: req.quote.quote_id.clone().unwrap_or_else(next_id),
                revision_no: req.quote.revision_no,
                feedback_round: next_round(inquiry)?,
                scope: InquiryQuoteScope {
                    inquiry_id: inquiry.base.id.clone(),
                    requirement_version: inquiry.requirement_version,
                    line_id: line.line_id.clone(),
                },
                supplier_id: req.quote.supplier_id.clone(),
                supplier_offering_id: req.quote.supplier_offering_id.clone(),
                sku_id: SkuId::new(line.sku_id.clone()),
                terms,
                internal_note: req.quote.internal_note.clone(),
                recorded_by: actor.id().to_string(),
            },
        )?;
        InquirySupplyService::new(self.db.clone())
            .append_quote(&quote, &MongoOfferingQualification::new(self.db.clone()), executor)
            .await
    }

    pub(super) async fn append_fee_basis(
        &self,
        actor: &AuditActor,
        inquiry: &SalesInquiry,
        task: &WorkItem,
        req: &SaveInquiryFeeBasisCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        inquiry.ensure_processing(req.expected_version)?;
        inquiry.responsibility_fee(actor.id(), task.responsibility_scope_ids(), &req.basis.fee_id)?;
        supplier_access(self.db.clone(), self.rbac.clone())
            .require_with(actor, "detail", req.basis.payee_supplier_id.as_ref(), executor)
            .await?;
        let service = InquirySupplyService::new(self.db.clone());
        let round = next_round(inquiry)?;
        let prior = service
            .fee_basis(&inquiry.base.id, inquiry.requirement_version, round, &req.basis.fee_id, executor)
            .await?;
        let id = prior.as_ref().map(|basis| basis.base.id.clone()).unwrap_or_else(next_id);
        let mut basis = InquiryFeeBasis::new(
            id.clone(),
            InquiryFeeBasisData {
                inquiry_id: inquiry.base.id.clone(),
                requirement_version: inquiry.requirement_version,
                feedback_round: round,
                fee_id: req.basis.fee_id.clone(),
                payee_supplier_id: req.basis.payee_supplier_id.clone(),
                payment_term_code: req.basis.payment_term_code.clone(),
                internal_note: req.basis.internal_note.clone(),
                attachment_ids: req.basis.attachment_ids.clone(),
                recorded_by: actor.id().to_string(),
            },
        )?;
        if let Some(prior) = prior {
            basis.base = prior.base;
        }
        self.attach_internal_files(actor, &id, &req.basis.attachment_ids, executor).await?;
        Ok(service.save_fee_basis(&basis, executor).await?)
    }

    async fn attach_internal_files(
        &self,
        actor: &AuditActor,
        owner_id: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let links = self
            .db
            .document_attachments()
            .list_by_document(&BusinessDocumentId::new(owner_id), executor)
            .await?;
        for id in ids {
            let linked = links.iter().any(|link| link.file_asset_id.as_ref() == id);
            let asset = self
                .db
                .file_assets()
                .find_by_id(id, executor)
                .await?
                .filter(|asset| {
                    (asset.created_by == actor.id() || linked)
                        && asset.destroyed_at.is_none()
                        && asset.expires_at.is_none_or(|at| at > Instant::now())
                })
                .ok_or_else(|| Error::Forbidden("内部附件不存在或不属于当前上传人".into()))?;
            if linked {
                continue;
            }
            let attachment = DocumentAttachment::new(
                DocumentAttachmentId::new(next_id()),
                DocumentAttachmentData {
                    document_id: BusinessDocumentId::new(owner_id),
                    file_asset_id: FileAssetId::new(asset.base.id),
                    usage: AttachmentUsage::Attachment,
                    created_by: actor.id().to_string(),
                },
            )?;
            self.db.document_attachments().create(&attachment, executor).await?;
        }
        Ok(())
    }
}

fn next_round(inquiry: &SalesInquiry) -> Result<u32> {
    inquiry.feedback_round.checked_add(1).ok_or_else(|| Error::Internal("询价反馈轮次超出范围".into()))
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::region::RegionCode;
    use erp_sales::entity::sales_inquiry::{InquiryDeliveryAllocation, InquiryFulfillmentKind};

    use super::*;

    fn line() -> InquiryRequirementLine {
        InquiryRequirementLine {
            line_id: "line".into(),
            sku_id: "sku".into(),
            sku_revision_id: "revision".into(),
            sku_code: "0001".into(),
            product_name: "商品".into(),
            specification: "盒".into(),
            unit_code: "box".into(),
            quantity_scale: 0,
            quantity: "10".parse().unwrap(),
            fulfillment_kind: InquiryFulfillmentKind::Physical,
            delivery_due_at: Instant::from_unix_secs(1000),
            delivery_terms: String::new(),
            sales_reference_price_gross: None,
            allocations: vec![InquiryDeliveryAllocation {
                allocation_id: "address".into(),
                recipient: "收件人".into(),
                phone: "00138".into(),
                region_code: RegionCode::parse("310115").unwrap(),
                region_name: "浦东新区".into(),
                address: "测试路".into(),
                quantity: "10".parse().unwrap(),
            }],
        }
    }

    fn terms() -> InquiryQuoteTermsInput {
        InquiryQuoteTermsInput {
            unavailable: false,
            freight_gross: None,
            unit_cost_gross: "1.2".parse().unwrap(),
            input_tax_rate: "0.13".parse().unwrap(),
            available_quantity: "10".parse().unwrap(),
            expected_delivery_date: BusinessDate::today(),
            valid_to: None,
        }
    }

    #[test]
    fn quote_scope_is_all_demand_addresses_even_for_partial_supply() {
        let mut line = line();
        let mut second = line.allocations[0].clone();
        second.allocation_id = "second".into();
        second.address = "另一地址".into();
        line.allocations.push(second);
        let mut input = terms();
        input.available_quantity = "5".parse().unwrap();
        let quote = input.for_requirement(&line).unwrap();
        assert_eq!(quote.available_quantity.to_string(), "5");
        assert_eq!(quote.address_limits.len(), 2);
        assert_eq!(quote.address_limits[0].quantity.to_string(), "10");
        assert_eq!(quote.address_limits[1].quantity.to_string(), "10");
        assert_eq!(
            quote.address_limits[0].address_fingerprint,
            PurchaseInquiryAddress::fingerprint("收件人", "00138", "310115", "测试路")
        );
    }

    #[test]
    fn quote_rejects_excess_supply_and_wrong_quantity_precision() {
        let line = line();
        let mut input = terms();
        for quantity in ["11", "0", "1.5"] {
            input.available_quantity = quantity.parse().unwrap();
            assert!(input.for_requirement(&line).is_err());
        }
    }
    #[test]
    fn unavailable_reply_does_not_require_adoptable_delivery_scope() {
        let mut line = line();
        line.allocations.clear();
        let mut input = terms();
        input.unavailable = true;
        input.unit_cost_gross = "0".parse().unwrap();
        input.available_quantity = "0".parse().unwrap();
        assert!(input.for_requirement(&line).unwrap().unavailable);
        input.available_quantity = "1".parse().unwrap();
        assert!(input.for_requirement(&line).is_err());
    }
}
