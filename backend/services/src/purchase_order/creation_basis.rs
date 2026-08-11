//! 采购创建依据查询、依据建单及草稿构造。

use std::collections::HashMap;

use database::{
    AccessControlExt, NoTransaction, PurchaseOrderExt, SalesOrderExt, SalesReviewExt, SupplierExt,
    Transactional,
};
use entities::ids::{
    ProcurementConfirmationId, PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
};
use entities::money::line_amounts;
use entities::purchase_order::{
    FulfillmentResponsibility, PurchaseLineType, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus,
    PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
    PurchaseOrderSubmissionLineData, PurchaseType, SupplierSnapshot,
};
use entities::sales_review::ProcurementConfirmationStatus;
use id_generator::next_id;
use validator::Validate;

use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreationBasisLineView, CreationBasisView,
};
use super::shared::{fulfillment_from_mode, today_stamp, zero_amount};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 采购确认筛选条件类型（经 `SalesReviewExt` 关联类型跨 crate 可达）。
type ProcurementConfirmationFilter = <mongodb::Database as SalesReviewExt>::ProcurementConfirmationFilter;

impl PurchaseOrderService {
    /// 查询采购创建依据（已通过的采购确认及其分行，页面 W08 建单入口）。
    ///
    /// # 返回
    /// 返回全部已通过且未完全消费的确认批次（按创建时间倒序）。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn creation_basis_list(&self) -> Result<Vec<CreationBasisView>> {
        let filter = ProcurementConfirmationFilter {
            submission_id: None,
            status: Some(ProcurementConfirmationStatus::Approved),
            page: 1,
            page_size: 100,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let page = self
            .db
            .procurement_confirmations()
            .search_procurement_confirmations(&filter, &mut NoTransaction)
            .await?;

        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let already_created = self
                .db
                .purchase_orders()
                .find_one(
                    mongodb::bson::doc! {
                        "sales_order_id": row.sales_order_id.clone(),
                    },
                    &mut NoTransaction,
                )
                .await?;
            if already_created.is_some() {
                continue;
            }
            let confirmation_id = ProcurementConfirmationId::new(row.id.clone());
            let lines = self
                .db
                .procurement_confirmation_lines()
                .list_lines_by_confirmation(&confirmation_id, &mut NoTransaction)
                .await?;
            if lines.is_empty() {
                continue;
            }
            let supplier_id = lines[0].supplier_id.clone();
            let supplier_name = self
                .resolve_supplier_name(&supplier_id)
                .await?
                .unwrap_or_else(|| supplier_id.to_string());
            let payment_term_code = self.resolve_payment_term_code(&supplier_id).await?;

            let (line_views, estimated) = self.build_basis_lines(&lines).await?;
            views.push(CreationBasisView {
                basis_id: row.id,
                sales_order_id: row.sales_order_id,
                submission_id: row.submission_id,
                supplier_id: supplier_id.to_string(),
                supplier_name,
                payment_term_code,
                lines: line_views,
                estimated_gross: estimated,
            });
        }
        Ok(views)
    }

    /// 依据创建采购单（幂等：同拆单维度草稿复用，不重复创建）。
    ///
    /// 在单事务内写入 `purchase_order`、草稿 `purchase_order_submission` 与其明细；
    /// 商品行快照取自销售提交行（D13 读），金额逐行按确认成本×数量舍入。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或复用）采购单结果。
    ///
    /// # 错误
    /// * `NotFound` - 创建依据不存在或未通过
    /// * `ConflictError` - 同拆单维度已存在非终态采购单（幂等复用则返回成功）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_from_basis(
        &self,
        req: CreatePurchaseOrderFromBasisRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        req.validate()?;
        let confirmation_id = ProcurementConfirmationId::new(req.basis_id.clone());
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(&confirmation_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购创建依据不存在".to_string()))?;
        if confirmation.stable.status != ProcurementConfirmationStatus::Approved {
            return Err(Error::BusinessLogicError(
                "创建依据未通过采购确认，不能建单".to_string(),
            ));
        }
        let lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation_id, &mut NoTransaction)
            .await?;
        if lines.is_empty() {
            return Err(Error::BusinessLogicError("创建依据没有可拆入的分行".to_string()));
        }
        let split_dimensions = lines
            .iter()
            .map(|line| (line.supplier_id.to_string(), line.fulfillment_mode.as_str()))
            .collect::<std::collections::HashSet<_>>();
        if split_dimensions.len() != 1 {
            return Err(Error::ValidationError(
                "该采购确认包含多个供应商或履约方式，必须由审批动作自动拆分采购单草稿".to_string(),
            ));
        }
        let supplier_id = lines[0].supplier_id.clone();
        for line in &lines {
            crate::supplier::eligibility::ensure_capability_qualified(
                &self.db,
                &line.supplier_id,
                &line.supplier_capability_revision_id,
                entities::common::time::BusinessDate::today(),
            )
            .await
            .map_err(|error| {
                Error::BusinessLogicError(format!("采购创建依据第 {} 行资质校验失败：{error}", line.line_no))
            })?;
        }
        let fulfillment = fulfillment_from_mode(lines[0].fulfillment_mode);
        let sales_order_id = confirmation.sales_order_id.clone();

        // 幂等去重：同一销售单 + 供应商 + 拆单维度已存在非终态采购单则复用。
        let existing = self
            .db
            .purchase_orders()
            .find_one(
                mongodb::bson::doc! {
                    "sales_order_id": sales_order_id.to_string(),
                    "supplier_id": supplier_id.to_string(),
                    "status": { "$in": [
                        PurchaseOrderStatus::Draft.as_str(),
                        PurchaseOrderStatus::PendingFinanceReview.as_str(),
                    ]},
                },
                &mut NoTransaction,
            )
            .await?;
        if let Some(order) = existing {
            return Ok(CreatePurchaseOrderResult {
                purchase_order_id: order.base.id.clone(),
                purchase_no: order.purchase_no.clone(),
                lock_version: order.base.version,
                replayed: true,
                reference: format!("PO-{}", order.purchase_no),
            });
        }

        let supplier_name = self
            .resolve_supplier_name(&supplier_id)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let order_id = PurchaseOrderId::new(next_id());
        let purchase_no = format!("PO-{}-{}", today_stamp(), &order_id.to_string()[..6]);
        let mut order = PurchaseOrder::new(
            order_id.clone(),
            PurchaseOrderData {
                purchase_no,
                sales_order_id,
                supplier_id: supplier_id.clone(),
                purchase_type: req.purchase_type,
                payment_term_code: req.payment_term_code.clone(),
                fulfillment_responsibility: fulfillment,
            },
            actor.id(),
        )?;

        let submission = self
            .build_draft_submission(
                &order_id,
                &supplier_id,
                req.purchase_type,
                fulfillment,
                &supplier_name,
                &req.payment_term_code,
                &lines,
            )
            .await?;
        order.current_submission_id = Some(submission.base.id.clone());
        let submission_lines = self
            .build_submission_lines_from_basis(&submission.base.id, &lines)
            .await?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.create", "purchase_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_orders().create(&order_for_tx, session).await?;
                    db.purchase_order_submissions()
                        .create(&submission_for_tx, session)
                        .await?;
                    for line in &submission_lines {
                        db.purchase_order_submission_lines().create(line, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(CreatePurchaseOrderResult {
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            lock_version: order.base.version,
            replayed: false,
            reference: format!("PO-{}", order.purchase_no),
        })
    }

    /// 解析供应商付款条件代码（D09 商务结算版本快照，缺省 `NET-30`）。
    async fn resolve_payment_term_code(
        &self,
        supplier_id: &entities::ids::SupplierAccountId,
    ) -> Result<String> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?;
        let Some(supplier) = supplier else {
            return Ok("NET-30".to_string());
        };
        let Some(revision_id) = supplier.current_commercial_profile_revision_id else {
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

    /// 构建创建依据行视图（金额逐行舍入）。
    async fn build_basis_lines(
        &self,
        lines: &[entities::sales_review::ProcurementConfirmationLine],
    ) -> Result<(Vec<CreationBasisLineView>, String)> {
        let mut views = Vec::with_capacity(lines.len());
        let mut estimated = zero_amount();
        let sales_line_ids: Vec<String> = lines
            .iter()
            .map(|line| line.sales_order_submission_line_id.to_string())
            .collect();
        let sales_lines = self
            .db
            .sales_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": sales_line_ids } },
                &mut NoTransaction,
            )
            .await?;
        let sales_by_id: HashMap<String, entities::sales_order::SalesOrderSubmissionLine> = sales_lines
            .into_iter()
            .map(|line| (line.base.id.clone(), line))
            .collect();
        for line in lines {
            let (gross, _, _) = line_amounts(
                line.latest_cost_gross,
                line.confirmed_quantity,
                line.input_tax_rate,
            );
            estimated = estimated.checked_add(gross);
            let sales_line = sales_by_id.get(&line.sales_order_submission_line_id.to_string());
            views.push(CreationBasisLineView {
                procurement_confirmation_line_id: line.base.id.clone(),
                sales_order_submission_line_id: line.sales_order_submission_line_id.to_string(),
                supplier_id: line.supplier_id.to_string(),
                confirmed_quantity: line.confirmed_quantity.to_string(),
                latest_cost_gross: line.latest_cost_gross.to_string(),
                input_tax_rate: line.input_tax_rate.to_string(),
                expected_delivery_date: line.expected_delivery_date.to_string(),
                product_name: sales_line.map(|sales| sales.item_name_snapshot.clone()),
                specification: sales_line.and_then(|sales| sales.spec_snapshot.clone()),
                gross_amount: gross.to_string(),
            });
        }
        Ok((views, estimated.to_string()))
    }

    /// 构建草稿提交（表头来自依据，供应商快照与付款条件门禁在提交时冻结）。
    #[allow(clippy::too_many_arguments)]
    async fn build_draft_submission(
        &self,
        order_id: &PurchaseOrderId,
        supplier_id: &entities::ids::SupplierAccountId,
        purchase_type: PurchaseType,
        fulfillment: FulfillmentResponsibility,
        supplier_name: &str,
        payment_term_code: &str,
        lines: &[entities::sales_review::ProcurementConfirmationLine],
    ) -> Result<PurchaseOrderSubmission> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let revision_id = supplier
            .current_commercial_profile_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("供应商缺少商务结算版本".to_string()))?;
        let gross = lines
            .iter()
            .map(|line| {
                let (gross, _, _) = line_amounts(
                    line.latest_cost_gross,
                    line.confirmed_quantity,
                    line.input_tax_rate,
                );
                gross
            })
            .fold(zero_amount(), |acc, amount| acc.checked_add(amount));
        let net = lines
            .iter()
            .map(|line| {
                let (_, net, _) = line_amounts(
                    line.latest_cost_gross,
                    line.confirmed_quantity,
                    line.input_tax_rate,
                );
                net
            })
            .fold(zero_amount(), |acc, amount| acc.checked_add(amount));
        let tax = lines
            .iter()
            .map(|line| {
                let (_, _, tax) = line_amounts(
                    line.latest_cost_gross,
                    line.confirmed_quantity,
                    line.input_tax_rate,
                );
                tax
            })
            .fold(zero_amount(), |acc, amount| acc.checked_add(amount));
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
                payment_term_snapshot: self.payment_term_snapshot(payment_term_code).await?,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )
        .map_err(Into::into)
    }

    /// 从依据分行构建草稿明细（商品行快照取自销售提交行 D13）。
    async fn build_submission_lines_from_basis(
        &self,
        submission_id: &str,
        lines: &[entities::sales_review::ProcurementConfirmationLine],
    ) -> Result<Vec<PurchaseOrderSubmissionLine>> {
        let mut result = Vec::with_capacity(lines.len());
        let sales_line_ids: Vec<String> = lines
            .iter()
            .map(|line| line.sales_order_submission_line_id.to_string())
            .collect();
        let sales_lines = self
            .db
            .sales_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": sales_line_ids } },
                &mut NoTransaction,
            )
            .await?;
        let sales_by_id: HashMap<String, entities::sales_order::SalesOrderSubmissionLine> = sales_lines
            .into_iter()
            .map(|line| (line.base.id.clone(), line))
            .collect();
        for (index, line) in lines.iter().enumerate() {
            let (gross, net, tax) = line_amounts(
                line.latest_cost_gross,
                line.confirmed_quantity,
                line.input_tax_rate,
            );
            let sales_line = sales_by_id.get(&line.sales_order_submission_line_id.to_string());
            let purchase_line = PurchaseOrderSubmissionLine::new(
                PurchaseOrderSubmissionLineId::new(next_id()),
                PurchaseOrderSubmissionLineData {
                    purchase_order_submission_id: entities::ids::PurchaseOrderSubmissionId::new(
                        submission_id.to_string(),
                    ),
                    line_no: (index + 1) as u32,
                    line_type: PurchaseLineType::ItemService,
                    procurement_confirmation_line_id: Some(line.base.id.clone().into()),
                    sku_id: sales_line.and_then(|sales| sales.sku_id.clone()),
                    sku_revision_id: sales_line.and_then(|sales| sales.sku_revision_id.clone()),
                    product_name_snapshot: sales_line.map(|sales| sales.item_name_snapshot.clone()),
                    specification_snapshot: sales_line.and_then(|sales| sales.spec_snapshot.clone()),
                    quantity: Some(line.confirmed_quantity),
                    base_unit_code: sales_line.and_then(|sales| sales.unit_snapshot.clone()),
                    unit_cost_gross: Some(line.latest_cost_gross),
                    gross_amount: gross,
                    net_amount: net,
                    tax_amount: tax,
                    input_tax_rate: Some(line.input_tax_rate),
                    expected_delivery_date: Some(line.expected_delivery_date),
                    sales_order_submission_line_id: Some(line.sales_order_submission_line_id.clone()),
                    allocated_quantity: Some(line.confirmed_quantity),
                },
            )?;
            result.push(purchase_line);
        }
        Ok(result)
    }
}
