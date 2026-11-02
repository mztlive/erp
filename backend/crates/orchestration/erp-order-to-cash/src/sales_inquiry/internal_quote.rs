//! 采购内部资料按当前责任与精确需求版本保存，销售回执不包含内部字段。

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    BusinessDocumentId, DocumentAttachmentId, FileAssetId, SkuId, SupplierAccountId, SupplierOfferingId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use erp_procurement::entity::purchase_order::PurchaseInquiryAddress;
use erp_sales::entity::sales_inquiry::{InquiryRequirementLine, SalesInquiry};
use erp_supplier_catalog::supply_governance::offering::MongoOfferingQualification;
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

/// 绑定当前冻结需求行和已转换商业条件的报价输入，不参与持久化或请求序列化。
pub(super) struct ResolvedInquiryQuote<'a> {
    input: &'a InquiryQuoteInput,
    line: &'a InquiryRequirementLine,
    terms: QuoteTerms,
}

impl<'a> ResolvedInquiryQuote<'a> {
    /// 将报价输入绑定到已由当前责任范围定位的冻结需求行。
    ///
    /// # 参数
    /// `input` 为原始报价；`line` 为已定位的需求行。
    /// # 返回
    /// 返回复用同一份配送范围与商业条件的内部输入。
    /// # 错误
    /// 可供数量、单位精度、配送范围或商业条件非法时返回原错误。
    pub(super) fn new(input: &'a InquiryQuoteInput, line: &'a InquiryRequirementLine) -> Result<Self> {
        Ok(Self { input, line, terms: input.terms.for_requirement(line)? })
    }

    fn scope(&self, inquiry: &SalesInquiry) -> InquiryQuoteScope {
        InquiryQuoteScope {
            inquiry_id: inquiry.base.id.clone(),
            requirement_version: inquiry.requirement_version,
            line_id: self.line.line_id.clone(),
        }
    }

    /// 核对是否已经保存相同作用域、供应身份与商业条件的回复。
    ///
    /// # 参数
    /// `prior` 为既有报价；`inquiry` 与 `round` 指定当前需求版本和反馈轮次。
    /// # 返回
    /// 存在完全相同且未删除的回复时为真。
    /// # 错误
    /// 不返回错误。
    pub(super) fn matches_prior_reply(
        &self,
        prior: &[InquiryQuote],
        inquiry: &SalesInquiry,
        round: u32,
    ) -> bool {
        let scope = self.scope(inquiry);
        prior.iter().any(|old| {
            old.matches_reply(
                &scope,
                round,
                &self.input.supplier_id,
                &self.input.supplier_offering_id,
                &self.terms,
                &self.input.internal_note,
            )
        })
    }

    fn into_quote(self, inquiry: &SalesInquiry, recorded_by: &str) -> Result<InquiryQuote> {
        Ok(InquiryQuote::new(
            next_id(),
            InquiryQuoteData {
                quote_id: self.input.quote_id.clone().unwrap_or_else(next_id),
                revision_no: self.input.revision_no,
                feedback_round: next_round(inquiry)?,
                scope: self.scope(inquiry),
                supplier_id: self.input.supplier_id.clone(),
                supplier_offering_id: self.input.supplier_offering_id.clone(),
                sku_id: SkuId::new(self.line.sku_id.clone()),
                terms: self.terms,
                internal_note: self.input.internal_note.clone(),
                recorded_by: recorded_by.to_string(),
            },
        )?)
    }
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
        if !self.unavailable {
            let allocation_ids =
                line.allocations.iter().map(|row| row.allocation_id.clone()).collect::<Vec<_>>();
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

    /// 保存已授权任务的一条内部报价，单条录入与批量导入共用相同校验和写入。
    ///
    /// # 参数
    /// `actor` 为采购，`inquiry` 与 `task` 为已授权事实，`expected_version` 为请求版本，
    /// `input` 为当前报价，`executor` 沿用调用方事务。
    /// # 返回
    /// 报价已通过供应资格校验并追加保存。
    /// # 错误
    /// 版本、责任范围、供应商权限、报价条件或持久化失败时返回对应错误。
    pub(super) async fn append_internal_quote(
        &self,
        actor: &AuditActor,
        inquiry: &SalesInquiry,
        task: &WorkItem,
        expected_version: u64,
        input: &InquiryQuoteInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        inquiry.ensure_processing(expected_version)?;
        let line = inquiry.responsibility_line(task.responsibility_scope_ids(), &input.line_id)?;
        self.services.suppliers.require_with(actor, "detail", input.supplier_id.as_ref(), executor).await?;
        let quote = ResolvedInquiryQuote::new(input, line)?;
        self.append_resolved_quote(actor, inquiry, quote, executor).await
    }

    /// 对已完成供应商授权和条款转换的报价执行最终构造与资格写入。
    ///
    /// # 参数
    /// `actor` 为已授权采购；`inquiry` 为当前询价；`input` 为已绑定需求的报价；
    /// `executor` 沿用调用方事务。
    /// # 返回
    /// 报价通过最终领域与供应资格校验并追加保存。
    /// # 错误
    /// 轮次溢出、最终报价不合法、供应资格失效或写入失败时返回原错误。
    pub(super) async fn append_resolved_quote(
        &self,
        actor: &AuditActor,
        inquiry: &SalesInquiry,
        input: ResolvedInquiryQuote<'_>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let quote = input.into_quote(inquiry, actor.id())?;
        InquirySupplyService::new(self.db.clone())
            .append_quote(&quote, &MongoOfferingQualification::new(self.db.clone()), executor)
            .await
            .map_err(Error::from)
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
        self.services
            .suppliers
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
    use std::slice::from_ref;

    use erp_core::region::RegionCode;
    use erp_sales::entity::sales_inquiry::{
        InquiryCreateData, InquiryDeliveryAllocation, InquiryFulfillmentKind, InquiryRequirementData,
    };

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
            expected_delivery_date: "2026-10-25".parse().unwrap(),
            valid_to: None,
        }
    }

    fn input() -> InquiryQuoteInput {
        InquiryQuoteInput {
            quote_id: Some("quote".into()),
            revision_no: 1,
            line_id: "line".into(),
            supplier_id: SupplierAccountId::new("supplier"),
            supplier_offering_id: SupplierOfferingId::new("offering"),
            terms: terms(),
            internal_note: String::new(),
        }
    }

    fn inquiry() -> SalesInquiry {
        let mut inquiry = SalesInquiry::create(
            "inquiry".into(),
            "IQ1".into(),
            InquiryCreateData {
                customer_id: "customer".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                deadline_at: Instant::from_unix_secs(5000),
                copied_from_inquiry_id: None,
                draft: InquiryRequirementData { lines: vec![line()], note: String::new() },
            },
        )
        .unwrap();
        inquiry.requirement_version = 2;
        inquiry.feedback_round = 3;
        inquiry
    }

    #[test]
    fn resolved_quote_keeps_frozen_identity_and_terms_for_matching_and_persistence() {
        let inquiry = inquiry();
        let line = line();
        let mut input = input();
        input.terms.available_quantity = "5".parse().unwrap();
        let quote = ResolvedInquiryQuote::new(&input, &line).unwrap().into_quote(&inquiry, "buyer").unwrap();
        assert_eq!(quote.data.scope.inquiry_id, "inquiry");
        assert_eq!(quote.data.scope.requirement_version, 2);
        assert_eq!(quote.data.scope.line_id, "line");
        assert_eq!(quote.data.sku_id.as_ref(), "sku");
        assert_eq!(quote.data.feedback_round, 4);
        assert_eq!(quote.data.recorded_by, "buyer");
        assert_eq!(quote.data.terms.available_quantity.to_string(), "5");
        assert_eq!(quote.data.terms.address_limits[0].quantity.to_string(), "10");
        assert_eq!(
            quote.data.terms.address_limits[0].address_fingerprint,
            PurchaseInquiryAddress::fingerprint("收件人", "00138", "310115", "测试路")
        );

        input.quote_id = Some("another-client-id".into());
        input.revision_no = 8;
        let resolved = ResolvedInquiryQuote::new(&input, &line).unwrap();
        assert!(resolved.matches_prior_reply(from_ref(&quote), &inquiry, 4));
        assert!(!resolved.matches_prior_reply(from_ref(&quote), &inquiry, 5));
        let mut changed = quote.clone();
        changed.data.scope.requirement_version += 1;
        assert!(!resolved.matches_prior_reply(from_ref(&changed), &inquiry, 4));
        changed = quote.clone();
        changed.data.internal_note = "变更说明".into();
        assert!(!resolved.matches_prior_reply(from_ref(&changed), &inquiry, 4));
        changed = quote.clone();
        changed.data.terms.freight_gross = Some("0".parse().unwrap());
        assert!(!resolved.matches_prior_reply(from_ref(&changed), &inquiry, 4));
        changed = quote;
        changed.data.supplier_id = SupplierAccountId::new("another-supplier");
        assert!(!resolved.matches_prior_reply(from_ref(&changed), &inquiry, 4));
    }

    #[test]
    fn resolved_quote_rejects_invalid_terms_and_retains_final_domain_validation() {
        let inquiry = inquiry();
        let mut line = line();
        let mut input = input();
        input.terms.available_quantity = "11".parse().unwrap();
        assert!(matches!(
            ResolvedInquiryQuote::new(&input, &line),
            Err(Error::ValidationError(message)) if message == "专项报价可供量不得超过本行询价需求"
        ));
        input.terms.available_quantity = "10".parse().unwrap();
        line.allocations.clear();
        assert!(matches!(
            ResolvedInquiryQuote::new(&input, &line),
            Err(Error::ValidationError(message)) if message == "实物专项报价必须明确配送地址范围"
        ));
        input.terms.unavailable = true;
        input.terms.unit_cost_gross = "0".parse().unwrap();
        input.terms.available_quantity = "0".parse().unwrap();
        let resolved = ResolvedInquiryQuote::new(&input, &line).unwrap();
        assert!(matches!(
            resolved.into_quote(&inquiry, "buyer"),
            Err(Error::ValidationError(message)) if message == "无法供货须填写原因"
        ));
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
