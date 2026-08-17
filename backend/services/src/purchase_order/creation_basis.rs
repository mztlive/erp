//! 采购创建依据查询、依据建单及草稿构造。

use std::collections::HashMap;

use database::{NoTransaction, PurchaseOrderExt, SalesOrderExt, SalesReviewExt, SupplierExt, Transactional};
use entities::ids::ProcurementConfirmationId;
use entities::money::line_amounts;
use entities::sales_review::ProcurementConfirmationStatus;
use validator::Validate;

use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreationBasisLineView, CreationBasisView,
};
use super::shared::zero_amount;
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
        let sales_order_id = confirmation.sales_order_id.clone();
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let drafts = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    super::create_drafts_from_confirmation_lines(
                        &db,
                        &sales_order_id,
                        &lines,
                        &actor,
                        session,
                    )
                    .await
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
            reference: format!("PO-{}", draft.purchase_no),
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
}
