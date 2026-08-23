//! 采购创建依据与选源建单（erp-phase-1.md §7.4）。
//!
//! 旧采购确认批次（procurement_confirmation_*）已随硬切换删除。创建依据改为从
//! **已生效销售单**的未覆盖商品/服务明细推导：
//! - `creation_basis_list`：为每个（销售单 × 有合格供给的供应商）组合生成一条依据；
//! - `create_from_basis`：采购从该供应商的供给修订取成本/进项税率，按履约责任拆单
//!   创建采购草稿（同一销售单内供应商、付款条件、履约责任一致的明细合并为一张采购单）。
//!
//! 供给合格判定（§4.4/§7.4）：供给状态 ACTIVE、当前条款修订在业务时点有效
//! （valid_from ≤ 今天 ≤ valid_to）、可供状态 AVAILABLE 且可供数量（如有）≥ 剩余数量。

use std::collections::HashMap;

use chrono::{Datelike, FixedOffset};
use database::{
    AccessControlExt, DocumentRegistryExt, Executor, NoTransaction, PartyExt, PurchaseOrderExt,
    SalesOrderExt, SupplierExt, SupplierOfferingExt, Transactional,
};
use entities::common::time::{BusinessDate, Instant};
use entities::document_registry::DocumentType;
use entities::ids::{
    PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId,
    SalesOrderSubmissionLineId, SupplierAccountId,
};
use entities::money::{line_amounts, Amount, Quantity, UnitPrice};
use entities::purchase_order::{
    FulfillmentResponsibility, PurchaseLineType, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus,
    PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
    PurchaseOrderSubmissionLineData, PurchaseType, SupplierSnapshot,
};
use entities::sales_order::types::FulfillmentMode;
use entities::sales_order::{SalesOrder, SalesOrderSubmission, SalesOrderSubmissionLine, SubmissionStatus};
use entities::supplier_offering::{
    AvailabilityStatus, OfferingStatus, SupplierOffering, SupplierOfferingAvailability,
    SupplierOfferingRevision,
};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::ClientSession;
use validator::Validate;

use super::adapter::{purchase_order_object_readable, purchase_order_responsible_org_id};
use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreationBasisLineView, CreationBasisView,
};
use super::shared::zero_amount;
use super::PurchaseOrderService;
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::audit::AuditActor;
use crate::document_registry::new_registered_document;
use crate::errors::{Error, Result};
use crate::iam::{shared_rbac_service, SharedRbacService};

/// 一条可建单的销售明细及其剩余数量。
#[derive(Clone)]
struct BasisLine {
    /// 已通过提交行（不可变快照）。
    submission_line: SalesOrderSubmissionLine,
    /// 剩余数量（销售数量 − 已分配采购数量；已建单销售单不进入依据，恒为全量）。
    remaining_quantity: Quantity,
}

/// 单条销售明细的合格供给（用于选源与成本取值）。
struct LineSupply {
    offering: SupplierOffering,
    revision: SupplierOfferingRevision,
    availability: Option<SupplierOfferingAvailability>,
}

impl PurchaseOrderService {
    /// 查询采购创建依据（已生效销售单 × 合格供给供应商，W08 建单入口）。
    ///
    /// # 返回
    /// 返回全部可建单依据。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn creation_basis_list(&self) -> Result<Vec<CreationBasisView>> {
        let candidates = self.basis_candidates().await?;
        let owner_ids = candidates
            .iter()
            .map(|candidate| candidate.submission.submitted_by.clone())
            .collect::<Vec<_>>();
        let owner_names = self.resolve_account_names(&owner_ids).await?;
        let mut views = Vec::new();
        for candidate in candidates {
            let sales_owner_name = owner_names.get(&candidate.submission.submitted_by).cloned();
            let supplier_lines = self.eligible_supplier_lines(&candidate).await?;
            for (supplier_id, lines) in supplier_lines {
                if lines.is_empty() {
                    continue;
                }
                views.push(
                    self.build_basis_view(&candidate, &supplier_id, &lines, sales_owner_name.clone())
                        .await?,
                );
            }
        }
        Ok(views)
    }

    /// 依据创建采购单（幂等：同销售单 + 供应商 + 履约责任已存在草稿/待审单时复用）。
    ///
    /// 在单事务内写入 `purchase_order`、草稿 `purchase_order_submission` 与其明细，
    /// 并为每张采购单绑定已发布定义；成本/进项税率取所选供应商供给修订，
    /// 预计交期取销售明细履约期限，商品快照取自销售提交行。
    ///
    /// # 参数
    /// * `req` - 创建请求（basis_id 形如 `{sales_order_id}:{supplier_id}`）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或复用）采购单结果。
    ///
    /// # 错误
    /// * `NotFound` - 创建依据不存在或销售单不是已生效状态
    /// * `ValidationError` - 所选供应商无合格供给、明细已全部覆盖或请求体非法
    pub async fn create_from_basis(
        &self,
        req: CreatePurchaseOrderFromBasisRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        req.validate()?;
        let (sales_order_id, supplier_id) = parse_basis_id(&req.basis_id)?;
        let candidate = self.load_basis_candidate(&sales_order_id).await?;
        let supplier_lines = self.eligible_supplier_lines(&candidate).await?;
        let lines = supplier_lines
            .get(&supplier_id)
            .cloned()
            .ok_or_else(|| Error::ValidationError("所选供应商没有覆盖该销售单的合格供给".to_string()))?;
        if lines.is_empty() {
            return Err(Error::ValidationError("创建依据没有可拆入的分行".to_string()));
        }

        // 同一拆单维度已存在草稿/待审单时幂等复用
        if let Some(existing) = self.find_existing_draft(&sales_order_id, &supplier_id).await? {
            return Ok(existing);
        }

        let groups = group_by_fulfillment_responsibility(&lines);
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let rbac = shared_rbac_service(db.clone());
        let drafts = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut created = Vec::with_capacity(groups.len());
                    for group in groups {
                        let draft = persist_split_group(
                            &db,
                            &rbac,
                            &sales_order_id,
                            &supplier_id,
                            &group,
                            &actor,
                            session,
                        )
                        .await?;
                        created.push(draft);
                    }
                    Ok::<Vec<CreatedPurchaseDraft>, Error>(created)
                })
            })
            .await?;
        let draft = drafts
            .first()
            .ok_or_else(|| Error::BusinessLogicError("未能按创建依据拆出采购草稿".to_string()))?;
        Ok(CreatePurchaseOrderResult {
            purchase_order_id: draft.purchase_order_id.clone(),
            purchase_no: draft.purchase_no.clone(),
            lock_version: draft.lock_version,
            replayed: draft.replayed,
            reference: draft.purchase_order_id.clone(),
        })
    }

    /// 已生效且尚未建单的销售单候选及其未覆盖商品/服务明细。
    async fn basis_candidates(&self) -> Result<Vec<BasisCandidate>> {
        let orders = self
            .db
            .sales_orders()
            .find_many(doc! { "commercial_status": "EFFECTIVE" }, &mut NoTransaction)
            .await?;
        let mut candidates = Vec::new();
        for order in orders {
            let Some(submission) = self.latest_approved_submission(&order).await? else {
                continue;
            };
            if self
                .has_purchase_order(&SalesOrderId::new(order.base.id.clone()))
                .await?
            {
                continue;
            }
            let lines = self.basis_lines_of_submission(&submission).await?;
            if lines.is_empty() {
                continue;
            }
            candidates.push(BasisCandidate {
                order,
                submission,
                lines,
            });
        }
        Ok(candidates)
    }

    /// 加载单个候选（create_from_basis 复用同一推导）。
    async fn load_basis_candidate(&self, sales_order_id: &SalesOrderId) -> Result<BasisCandidate> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.commercial_status != entities::sales_order::CommercialStatus::Effective {
            return Err(Error::NotFound("销售单未生效，不能作为采购创建依据".to_string()));
        }
        if self
            .has_purchase_order(&SalesOrderId::new(order.base.id.clone()))
            .await?
        {
            return Err(Error::NotFound("该销售单已创建采购单".to_string()));
        }
        let submission = self
            .latest_approved_submission(&order)
            .await?
            .ok_or_else(|| Error::NotFound("销售单缺少已通过提交，不能作为采购创建依据".to_string()))?;
        let lines = self.basis_lines_of_submission(&submission).await?;
        if lines.is_empty() {
            return Err(Error::NotFound("销售单没有可采购的商品/服务明细".to_string()));
        }
        Ok(BasisCandidate {
            order,
            submission,
            lines,
        })
    }

    /// 取提交的 GOODS_SERVICE 明细（数量非空且为正）。
    async fn basis_lines_of_submission(&self, submission: &SalesOrderSubmission) -> Result<Vec<BasisLine>> {
        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .find_many(
                doc! {
                    "submission_id": submission.base.id.clone(),
                    "line_type": "GOODS_SERVICE",
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(submission_lines
            .into_iter()
            .filter_map(|line| {
                let quantity = line.quantity?;
                if quantity.to_decimal().is_zero() {
                    return None;
                }
                Some(BasisLine {
                    submission_line: line,
                    remaining_quantity: quantity,
                })
            })
            .collect())
    }

    /// 取销售单最近一次已通过提交（同一销售单可能存在多轮提交/变更）。
    async fn latest_approved_submission(&self, order: &SalesOrder) -> Result<Option<SalesOrderSubmission>> {
        let submissions = self
            .db
            .sales_order_submissions()
            .find_many(
                doc! {
                    "sales_order_id": order.base.id.clone(),
                    "status": SubmissionStatus::Approved.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(submissions
            .into_iter()
            .max_by_key(|submission| submission.base.created_at))
    }

    /// 销售单是否已存在未作废采购单（草稿/审批中/生效/部分执行/已完成）。
    async fn has_purchase_order(&self, sales_order_id: &SalesOrderId) -> Result<bool> {
        let count = self
            .db
            .purchase_orders()
            .count_active_by_sales_order(sales_order_id, &mut NoTransaction)
            .await?;
        Ok(count > 0)
    }

    /// 按合格供给供应商分组候选明细（供应商可覆盖至少一条明细才进入依据）。
    async fn eligible_supplier_lines(
        &self,
        candidate: &BasisCandidate,
    ) -> Result<HashMap<SupplierAccountId, Vec<BasisLine>>> {
        let mut result: HashMap<SupplierAccountId, Vec<BasisLine>> = HashMap::new();
        for line in &candidate.lines {
            let supplies = self.qualified_supplies(line).await?;
            for supply in supplies {
                result
                    .entry(supply.offering.supplier_id.clone())
                    .or_default()
                    .push(line.clone());
            }
        }
        Ok(result)
    }

    /// 查询某销售明细的合格供给（ACTIVE 供给 + 时点有效条款 + 当前可供）。
    async fn qualified_supplies(&self, line: &BasisLine) -> Result<Vec<LineSupply>> {
        let Some(sku_id) = line.submission_line.sku_id.clone() else {
            return Ok(Vec::new());
        };
        let offerings = self
            .db
            .supplier_offerings()
            .find_many(
                doc! {
                    "sku_id": sku_id.to_string(),
                    "status": OfferingStatus::Active.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let today = BusinessDate::today();
        let mut supplies = Vec::new();
        for offering in offerings {
            let Some(revision_id) = offering.stable.current_revision_id.clone() else {
                continue;
            };
            let revision = self
                .db
                .supplier_offering_revisions()
                .find_by_id(&revision_id, &mut NoTransaction)
                .await?;
            let Some(revision) = revision else {
                continue;
            };
            if revision.valid_from > today || revision.valid_to.is_some_and(|valid_to| valid_to < today) {
                continue;
            }
            let availability = self
                .db
                .supplier_offering_availabilities()
                .find_one(
                    doc! { "supplier_offering_id": offering.base.id.clone() },
                    &mut NoTransaction,
                )
                .await?;
            let Some(availability) = availability else {
                continue;
            };
            if availability.availability_status != AvailabilityStatus::Available {
                continue;
            }
            if availability
                .available_quantity
                .is_some_and(|quantity| quantity < line.remaining_quantity)
            {
                continue;
            }
            supplies.push(LineSupply {
                offering,
                revision,
                availability: Some(availability),
            });
        }
        Ok(supplies)
    }

    /// 构造创建依据视图（金额逐行舍入）。
    async fn build_basis_view(
        &self,
        candidate: &BasisCandidate,
        supplier_id: &SupplierAccountId,
        lines: &[BasisLine],
        sales_owner_name: Option<String>,
    ) -> Result<CreationBasisView> {
        let supplier_name = self
            .resolve_supplier_name(supplier_id)
            .await?
            .unwrap_or_else(|| supplier_id.to_string());
        let payment_term_code = self.resolve_payment_term_code(supplier_id).await?;
        let first_mode = lines[0]
            .submission_line
            .fulfillment_mode
            .unwrap_or(FulfillmentMode::CompanyWarehouse);
        let mut line_views = Vec::with_capacity(lines.len());
        let mut estimated = zero_amount();
        for line in lines {
            let supply = self.supply_for_line(supplier_id, line).await?;
            let cost = supply_cost(&supply.revision, first_mode);
            let due_at = line
                .submission_line
                .fulfillment_due_at
                .ok_or_else(|| Error::Internal("销售提交行缺少履约期限".to_string()))?;
            let (gross, _, _) = line_amounts(cost, line.remaining_quantity, supply.revision.input_tax_rate);
            estimated = estimated.checked_add(gross);
            line_views.push(CreationBasisLineView {
                procurement_confirmation_line_id: line.submission_line.sales_order_line_id.to_string(),
                sales_order_submission_line_id: line.submission_line.base.id.clone(),
                sales_line_no: line.submission_line.line_no,
                supplier_id: supplier_id.to_string(),
                confirmed_quantity: line.remaining_quantity.to_string(),
                latest_cost_gross: cost.to_string(),
                input_tax_rate: supply.revision.input_tax_rate.to_string(),
                expected_delivery_date: business_date_of(due_at)?.to_string(),
                product_name: Some(line.submission_line.item_name_snapshot.clone()),
                specification: line.submission_line.spec_snapshot.clone(),
                unit: line
                    .submission_line
                    .unit_snapshot
                    .clone()
                    .or_else(|| line.submission_line.base_unit_code.clone()),
                gross_amount: gross.to_string(),
            });
        }
        Ok(CreationBasisView {
            basis_id: format!("{}:{}", candidate.order.base.id, supplier_id),
            sales_order_id: candidate.order.base.id.clone(),
            sales_order_no: candidate.order.order_no.clone(),
            customer_name: candidate.submission.customer_snapshot.customer_name.clone(),
            contract_no: candidate
                .submission
                .contract_snapshot
                .as_ref()
                .map(|snapshot| snapshot.contract_no.clone()),
            sales_owner_name,
            submission_id: candidate.submission.base.id.clone(),
            supplier_id: supplier_id.to_string(),
            supplier_name,
            purchase_type: purchase_type_from_mode(first_mode).as_str().to_string(),
            fulfillment_responsibility: fulfillment_from_mode(first_mode).as_str().to_string(),
            payment_term_code,
            lines: line_views,
            estimated_gross: estimated.to_string(),
        })
    }

    /// 取某供应商对该明细的合格供给（用于成本/税率；视作与依据推导一致）。
    async fn supply_for_line(&self, supplier_id: &SupplierAccountId, line: &BasisLine) -> Result<LineSupply> {
        let supplies = self.qualified_supplies(line).await?;
        supplies
            .into_iter()
            .find(|supply| &supply.offering.supplier_id == supplier_id)
            .ok_or_else(|| Error::ValidationError("所选供应商对该明细无合格供给".to_string()))
    }

    /// 查找同一拆单维度上已存在的草稿或待审采购单。
    async fn find_existing_draft(
        &self,
        sales_order_id: &SalesOrderId,
        supplier_id: &SupplierAccountId,
    ) -> Result<Option<CreatePurchaseOrderResult>> {
        let existing = self
            .db
            .purchase_orders()
            .find_one(
                doc! {
                    "sales_order_id": sales_order_id.to_string(),
                    "supplier_id": supplier_id.to_string(),
                    "status": { "$in": [
                        PurchaseOrderStatus::Draft.as_str(),
                        PurchaseOrderStatus::InApproval.as_str(),
                    ]},
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(existing.map(|order| CreatePurchaseOrderResult {
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            lock_version: order.base.version,
            replayed: true,
            reference: order.base.id.clone(),
        }))
    }

    /// 解析供应商付款条件代码（D09 商务结算版本快照，缺省 `NET-30`）。
    async fn resolve_payment_term_code(&self, supplier_id: &SupplierAccountId) -> Result<String> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?;
        let Some(supplier) = supplier else {
            return Ok("NET-30".to_string());
        };
        let Some(revision_id) = supplier.current_commercial_profile_revision_id.clone() else {
            return Ok("NET-30".to_string());
        };
        let revision = self
            .db
            .supplier_commercial_profile_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?;
        Ok(revision
            .map(|revision| revision.payment_term_snapshot)
            .unwrap_or_else(|| "NET-30".to_string()))
    }
}

/// 建单候选聚合。
struct BasisCandidate {
    /// 已生效销售单。
    order: SalesOrder,
    /// 最近一次已通过提交。
    submission: SalesOrderSubmission,
    /// 未覆盖商品/服务明细。
    lines: Vec<BasisLine>,
}

/// 已落库（或幂等复用）的采购草稿身份。
struct CreatedPurchaseDraft {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 乐观锁版本。
    pub lock_version: u64,
    /// 是否幂等回放。
    pub replayed: bool,
}

/// 解析依据 ID（`{sales_order_id}:{supplier_id}`）。
fn parse_basis_id(basis_id: &str) -> Result<(SalesOrderId, SupplierAccountId)> {
    let mut parts = basis_id.split(':');
    let sales_order_id = parts.next().filter(|part| !part.is_empty());
    let supplier_id = parts.next().filter(|part| !part.is_empty());
    match (sales_order_id, supplier_id, parts.next()) {
        (Some(sales), Some(supplier), None) => Ok((
            SalesOrderId::new(sales.to_string()),
            SupplierAccountId::new(supplier.to_string()),
        )),
        _ => Err(Error::NotFound("采购创建依据不存在".to_string())),
    }
}

/// 按履约责任分组明细（同一销售单内供应商、付款条件、履约责任一致才合并）。
fn group_by_fulfillment_responsibility(lines: &[BasisLine]) -> Vec<Vec<BasisLine>> {
    let mut groups: Vec<(FulfillmentResponsibility, Vec<BasisLine>)> = Vec::new();
    for line in lines {
        let mode = line
            .submission_line
            .fulfillment_mode
            .unwrap_or(FulfillmentMode::CompanyWarehouse);
        let responsibility = fulfillment_from_mode(mode);
        match groups.iter_mut().find(|(key, _)| *key == responsibility) {
            Some((_, bucket)) => bucket.push(line.clone()),
            None => groups.push((responsibility, vec![line.clone()])),
        }
    }
    groups.into_iter().map(|(_, bucket)| bucket).collect()
}

/// 把一个拆单分组写成采购草稿，或复用已有非终态单据。
async fn persist_split_group(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    sales_order_id: &SalesOrderId,
    supplier_id: &SupplierAccountId,
    lines: &[BasisLine],
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<CreatedPurchaseDraft> {
    let first = lines
        .first()
        .ok_or_else(|| Error::BusinessLogicError("拆单分组不能为空".to_string()))?;
    let mode = first
        .submission_line
        .fulfillment_mode
        .unwrap_or(FulfillmentMode::CompanyWarehouse);
    let fulfillment = fulfillment_from_mode(mode);
    let purchase_type = purchase_type_from_mode(mode);
    let supplier_name = resolve_supplier_name(db, supplier_id, session)
        .await?
        .unwrap_or_else(|| supplier_id.to_string());
    let payment_term_code = resolve_payment_term_code(db, supplier_id, session).await?;
    let order_id = PurchaseOrderId::new(next_id());
    let mut order = PurchaseOrder::new(
        order_id.clone(),
        PurchaseOrderData {
            purchase_no: String::new(),
            sales_order_id: sales_order_id.clone(),
            supplier_id: supplier_id.clone(),
            purchase_type,
            payment_term_code: payment_term_code.clone(),
            fulfillment_responsibility: fulfillment,
        },
        actor.id(),
    )?;

    // 行金额按所选供给成本与剩余数量逐行舍入
    let mut computed_lines = Vec::with_capacity(lines.len());
    let mut gross_sum = zero_amount();
    let mut net_sum = zero_amount();
    let mut tax_sum = zero_amount();
    for line in lines.iter() {
        let supply = qualified_supply_for_line(db, supplier_id, line, session).await?;
        let cost = supply_cost(&supply.revision, mode);
        let (gross, net, tax) = line_amounts(cost, line.remaining_quantity, supply.revision.input_tax_rate);
        gross_sum = gross_sum.checked_add(gross);
        net_sum = net_sum.checked_add(net);
        tax_sum = tax_sum.checked_add(tax);
        computed_lines.push((line, cost, gross, net, tax, supply.revision.input_tax_rate));
    }

    let submission = build_draft_submission(
        db,
        &order_id,
        supplier_id,
        purchase_type,
        fulfillment,
        &supplier_name,
        &payment_term_code,
        gross_sum,
        net_sum,
        tax_sum,
        session,
    )
    .await?;
    // 明细必须挂在提交头上（purchase_order_submission_id = 提交 id），
    // 否则详情按提交查不到行，保存会以空行覆盖草稿。
    let mut submission_lines = Vec::with_capacity(computed_lines.len());
    for (index, (line, cost, gross, net, tax, input_tax_rate)) in computed_lines.iter().enumerate() {
        submission_lines.push(build_submission_line(
            &PurchaseOrderSubmissionId::new(submission.base.id.clone()),
            (index + 1) as u32,
            line,
            *cost,
            *gross,
            *net,
            *tax,
            *input_tax_rate,
        )?);
    }
    order.current_submission_id = Some(submission.base.id.clone());
    write_prepared_draft(db, rbac, &order, &submission, &submission_lines, actor, session).await?;
    Ok(CreatedPurchaseDraft {
        purchase_order_id: order.base.id.clone(),
        purchase_no: order.purchase_no.clone(),
        lock_version: order.base.version,
        replayed: false,
    })
}

/// 构造采购草稿提交头（供应商快照 + 付款条件快照）。
async fn build_draft_submission(
    db: &mongodb::Database,
    order_id: &PurchaseOrderId,
    supplier_id: &SupplierAccountId,
    purchase_type: PurchaseType,
    fulfillment: FulfillmentResponsibility,
    supplier_name: &str,
    payment_term_code: &str,
    gross: Amount,
    net: Amount,
    tax: Amount,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrderSubmission> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let revision_id = supplier
        .current_commercial_profile_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("供应商缺少商务结算版本".to_string()))?;
    let prepay_gate = payment_term_code.trim().to_uppercase().starts_with("PREPAY");
    PurchaseOrderSubmission::new(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmissionData {
            purchase_order_id: order_id.clone(),
            submission_no: format!("DRAFT-{}", &next_id()[..8]),
            supplier_id: supplier_id.clone(),
            purchase_type,
            fulfillment_responsibility: fulfillment,
            supplier_revision_id: revision_id,
            supplier_snapshot: SupplierSnapshot::new(supplier_name.to_string())?,
            payment_term_snapshot: entities::purchase_order::PaymentTermSnapshot::new(
                payment_term_code.to_string(),
                prepay_gate,
                None,
                None,
            )?,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
        },
    )
    .map_err(Into::into)
}

/// 构造单条采购草稿明细（商品快照取自销售提交行）。
fn build_submission_line(
    submission_id: &PurchaseOrderSubmissionId,
    line_no: u32,
    line: &BasisLine,
    cost: UnitPrice,
    gross: Amount,
    net: Amount,
    tax: Amount,
    input_tax_rate: entities::money::Rate,
) -> Result<PurchaseOrderSubmissionLine> {
    let sku_id = line
        .submission_line
        .sku_id
        .clone()
        .ok_or_else(|| Error::Internal("销售提交行缺少 SKU".to_string()))?;
    let sku_revision_id = line
        .submission_line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::Internal("销售提交行缺少 SKU 修订".to_string()))?;
    let base_unit_code = line
        .submission_line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::Internal("销售提交行缺少单位".to_string()))?;
    let due_at = line
        .submission_line
        .fulfillment_due_at
        .ok_or_else(|| Error::Internal("销售提交行缺少履约期限".to_string()))?;
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(next_id()),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: submission_id.clone(),
            line_no,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(entities::ids::ProcurementConfirmationLineId::new(
                line.submission_line.sales_order_line_id.to_string(),
            )),
            sku_id: Some(sku_id),
            sku_revision_id: Some(sku_revision_id),
            product_name_snapshot: Some(line.submission_line.item_name_snapshot.clone()),
            specification_snapshot: line.submission_line.spec_snapshot.clone(),
            quantity: Some(line.remaining_quantity),
            base_unit_code: Some(base_unit_code),
            unit_cost_gross: Some(cost),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(input_tax_rate),
            expected_delivery_date: Some(business_date_of(due_at)?),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new(
                line.submission_line.base.id.clone(),
            )),
            allocated_quantity: Some(line.remaining_quantity),
        },
    )
    .map_err(Into::into)
}

/// 把已构造的采购草稿、提交和明细写入当前事务，并绑定已发布定义。
async fn write_prepared_draft(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    order: &PurchaseOrder,
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let audit =
        actor
            .clone()
            .resource_log("purchase_order.create", "purchase_order", order.base.id.clone())?;
    let sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    let organization_id = purchase_order_responsible_org_id(&sales_order)?;
    let _ = purchase_order_object_readable(&organization_id, actor.id())?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::PurchaseOrder,
        business_object_id: order.base.id.clone(),
        business_object_version: order.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let binding =
        bind_published_definition_on_document_create(db, rbac, &bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("采购单必须绑定已发布定义".to_string()))?;
    let mut document = new_registered_document(&order.base.id, DocumentType::PurchaseOrder, "")?;
    attach_published_binding(&mut document, binding)?;
    db.purchase_orders().create(order, session).await?;
    db.business_documents().create(&document, session).await?;
    db.purchase_order_submissions()
        .create(submission, session)
        .await?;
    for line in lines {
        db.purchase_order_submission_lines().create(line, session).await?;
    }
    db.audit_logs().create(&audit, session).await?;
    Ok(())
}

/// 查询某供应商对该明细的合格供给（create 时与依据推导保持一致）。
async fn qualified_supply_for_line(
    db: &mongodb::Database,
    supplier_id: &SupplierAccountId,
    line: &BasisLine,
    executor: &mut dyn Executor,
) -> Result<LineSupply> {
    let Some(sku_id) = line.submission_line.sku_id.clone() else {
        return Err(Error::ValidationError("销售明细缺少 SKU，无法选源".to_string()));
    };
    let offerings = db
        .supplier_offerings()
        .find_many(
            doc! {
                "sku_id": sku_id.to_string(),
                "supplier_id": supplier_id.to_string(),
                "status": OfferingStatus::Active.as_str(),
            },
            executor,
        )
        .await?;
    let today = BusinessDate::today();
    for offering in offerings {
        let Some(revision_id) = offering.stable.current_revision_id.clone() else {
            continue;
        };
        let revision = db
            .supplier_offering_revisions()
            .find_by_id(&revision_id, executor)
            .await?;
        let Some(revision) = revision else {
            continue;
        };
        if revision.valid_from > today || revision.valid_to.is_some_and(|valid_to| valid_to < today) {
            continue;
        }
        let availability = db
            .supplier_offering_availabilities()
            .find_one(
                doc! { "supplier_offering_id": offering.base.id.clone() },
                executor,
            )
            .await?;
        let Some(availability) = availability else {
            continue;
        };
        if availability.availability_status != AvailabilityStatus::Available {
            continue;
        }
        if availability
            .available_quantity
            .is_some_and(|quantity| quantity < line.remaining_quantity)
        {
            continue;
        }
        return Ok(LineSupply {
            offering,
            revision,
            availability: Some(availability),
        });
    }
    Err(Error::ValidationError("所选供应商对该明细无合格供给".to_string()))
}

/// 取供给成本：入仓用集采价，直发/电子交付/线下服务用一件代发价（§7.4）。
fn supply_cost(revision: &SupplierOfferingRevision, mode: FulfillmentMode) -> UnitPrice {
    match mode {
        FulfillmentMode::CompanyWarehouse => revision.bulk_supply_price_gross,
        FulfillmentMode::SupplierDirect
        | FulfillmentMode::ElectronicDelivery
        | FulfillmentMode::OfflineService => revision.dropship_supply_price_gross,
    }
}

/// 由履约方式推断采购类型。
fn purchase_type_from_mode(mode: FulfillmentMode) -> PurchaseType {
    match mode {
        FulfillmentMode::CompanyWarehouse | FulfillmentMode::SupplierDirect => PurchaseType::Physical,
        FulfillmentMode::ElectronicDelivery => PurchaseType::Virtual,
        FulfillmentMode::OfflineService => PurchaseType::Service,
    }
}

/// 由履约方式推断履约责任。
fn fulfillment_from_mode(mode: FulfillmentMode) -> FulfillmentResponsibility {
    match mode {
        FulfillmentMode::CompanyWarehouse => FulfillmentResponsibility::Warehouse,
        FulfillmentMode::SupplierDirect => FulfillmentResponsibility::SupplierDirect,
        FulfillmentMode::ElectronicDelivery => FulfillmentResponsibility::Electronic,
        FulfillmentMode::OfflineService => FulfillmentResponsibility::Service,
    }
}

/// 销售明细履约期限（Instant）转业务日期。
fn business_date_of(instant: Instant) -> Result<BusinessDate> {
    let business_tz = FixedOffset::east_opt(8 * 60 * 60)
        .ok_or_else(|| Error::Internal("无法形成 Asia/Shanghai 时区".to_string()))?;
    let naive = instant.as_utc().with_timezone(&business_tz).date_naive();
    BusinessDate::from_ymd(naive.year(), naive.month(), naive.day())
        .ok_or_else(|| Error::Internal("履约期限日期非法".to_string()))
}

/// 读取供应商主体法定名称。
async fn resolve_supplier_name(
    db: &mongodb::Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(supplier) = db.supplier_accounts().find_by_id(supplier_id, executor).await? else {
        return Ok(None);
    };
    let Some(party) = db.parties().find_by_id(&supplier.party_id, executor).await? else {
        return Ok(None);
    };
    let Some(revision_id) = party.stable.current_revision_id.clone() else {
        return Ok(None);
    };
    let revision = db.party_revisions().find_by_id(&revision_id, executor).await?;
    Ok(revision.map(|revision| revision.legal_name))
}

/// 读取供应商当前商务结算版本上的付款条件，缺省 `NET-30`。
async fn resolve_payment_term_code(
    db: &mongodb::Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<String> {
    let Some(supplier) = db.supplier_accounts().find_by_id(supplier_id, executor).await? else {
        return Ok("NET-30".to_string());
    };
    let Some(revision_id) = supplier.current_commercial_profile_revision_id.clone() else {
        return Ok("NET-30".to_string());
    };
    let revision = db
        .supplier_commercial_profile_revisions()
        .find_by_id(&revision_id, executor)
        .await?;
    Ok(revision
        .map(|revision| revision.payment_term_snapshot)
        .unwrap_or_else(|| "NET-30".to_string()))
}

#[cfg(test)]
mod tests {
    use super::business_date_of;
    use entities::common::time::Instant;

    /// 上海零点对应前一日 UTC 时，仍须还原为输入的业务自然日。
    #[test]
    fn business_date_uses_shanghai_timezone() {
        let unix_secs = chrono::DateTime::parse_from_rfc3339("2026-08-23T00:00:00+08:00")
            .expect("测试时间合法")
            .timestamp();

        let date = business_date_of(Instant::from_unix_secs(unix_secs)).expect("业务日期合法");

        assert_eq!(date.to_string(), "2026-08-23");
    }
}
