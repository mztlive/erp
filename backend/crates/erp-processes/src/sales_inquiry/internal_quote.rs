//! 采购内部资料按当前责任与精确需求版本保存，销售回执不包含内部字段。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{
    BusinessDocumentId, DocumentAttachmentId, FileAssetId, SkuId, SupplierAccountId, SupplierOfferingId,
};
use erp_procurement::entity::purchase_order::PurchaseInquiryAddress;
use erp_sales::entity::sales_inquiry::{InquiryRequirementLine, SalesInquiry};
use erp_supply::inquiry::{
    InquiryFeeBasis, InquiryFeeBasisData, InquiryQuote, InquiryQuoteData, InquiryQuoteScope,
    InquirySupplyService, QuoteTerms,
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
    pub terms: QuoteTerms,
    pub internal_note: String,
    pub attachment_ids: Vec<String>,
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
    /// 任务、SKU、地址、附件、供应身份或商业条件无效时拒绝。
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
        let mut terms = req.quote.terms.clone();
        normalize_addresses(line, &mut terms)?;
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
                attachment_ids: req.quote.attachment_ids.clone(),
                recorded_by: actor.id().to_string(),
            },
        )?;
        self.attach_internal_files(actor, &id, &req.quote.attachment_ids, executor).await?;
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

fn normalize_addresses(line: &InquiryRequirementLine, terms: &mut QuoteTerms) -> Result<()> {
    let allocation_ids =
        terms.address_limits.iter().map(|limit| limit.allocation_id.clone()).collect::<Vec<_>>();
    line.validate_quote_scope(terms.available_quantity, &allocation_ids)?;
    line.validate_supply_quantity(terms.minimum_order_quantity)?;
    for limit in &mut terms.address_limits {
        line.validate_supply_quantity(limit.quantity)?;
        let allocation = line
            .allocations
            .iter()
            .find(|allocation| allocation.allocation_id == limit.allocation_id)
            .ok_or_else(|| Error::ValidationError("专项报价地址不属于当前需求行".into()))?;
        if limit.quantity > allocation.quantity {
            return Err(Error::ValidationError("专项报价地址数量超出需求".into()));
        }
        limit.address_fingerprint = PurchaseInquiryAddress::fingerprint(
            &allocation.recipient,
            &allocation.phone,
            allocation.region_code.as_str(),
            &allocation.address,
        );
    }
    Ok(())
}

fn next_round(inquiry: &SalesInquiry) -> Result<u32> {
    inquiry.feedback_round.checked_add(1).ok_or_else(|| Error::Internal("询价反馈轮次超出范围".into()))
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::region::RegionCode;
    use erp_sales::entity::sales_inquiry::{InquiryDeliveryAllocation, InquiryFulfillmentKind};
    use erp_supply::inquiry::QuoteAddressLimit;

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

    fn terms() -> QuoteTerms {
        QuoteTerms {
            unit_cost_gross: "1.2".parse().unwrap(),
            input_tax_rate: "0.13".parse().unwrap(),
            minimum_order_quantity: "1".parse().unwrap(),
            available_quantity: "10".parse().unwrap(),
            address_limits: vec![QuoteAddressLimit {
                allocation_id: "address".into(),
                address_fingerprint: "forged".into(),
                quantity: "5".parse().unwrap(),
            }],
            expected_delivery_date: BusinessDate::today(),
            valid_from: BusinessDate::today(),
            valid_to: "2030-01-01".parse().unwrap(),
            payment_term_code: "net30".into(),
            allow_cumulative_batches: false,
        }
    }

    #[test]
    fn exact_address_is_server_fingerprinted_and_foreign_or_excess_scope_rejected() {
        let line = line();
        let mut terms = terms();
        normalize_addresses(&line, &mut terms).unwrap();
        assert_eq!(
            terms.address_limits[0].address_fingerprint,
            PurchaseInquiryAddress::fingerprint("收件人", "00138", "310115", "测试路")
        );
        terms.address_limits[0].quantity = "11".parse().unwrap();
        assert!(matches!(normalize_addresses(&line, &mut terms), Err(Error::ValidationError(_))));
        terms.address_limits[0].quantity = "1".parse().unwrap();
        terms.address_limits[0].allocation_id = "foreign".into();
        assert!(matches!(normalize_addresses(&line, &mut terms), Err(Error::ValidationError(_))));
    }
}
