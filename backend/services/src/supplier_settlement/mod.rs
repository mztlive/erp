//! 域 D33 `supplier_settlement` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建结算草稿：结算单 + 全部明细同事务（`create_statement_with_items` 要求事务
//!   执行器，§6.20）；
//! - 结算确认（§8.4 第 6 条）：锁定结算单并重验差异处理结果、形成结算单应付
//!   （D19 `PayableRepository::create_payable_with_entry`，来源类型
//!   `SupplierSettlement`）并更新结算状态，同一事务完成；最终成本差额（D20
//!   `cost_entry`）不在本域声明依赖内，见 PR「未实现且已知的缺口」。
//!
//! 跨域协作只经 DatabaseExt 调对方域 Repository（P3 §2）：D32 `supplier_fulfillment`
//! （履约订单与明细存在性）、D19 `payable`（应付账户与原始分录）。
//!
//! 资金/状态机入口一律幂等：创建键为 `statement_no`，确认/提交复核/作废重复提交
//! 返回原结算单当前视图（不重复形成应付、不重复推进状态）；差异处理以版本 CAS
//! 防并发覆盖。

use database::{
    AccessControlExt, NoTransaction, PayableExt, SupplierFulfillmentExt, SupplierSettlementExt, Transactional,
};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{PayableAccountId, PayableEntryId};
use entities::money::Amount;
use entities::payable::{
    EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData, PayableEntryType,
    PayableSourceType,
};
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementStatus, SupplierSettlementDifference,
    SupplierSettlementDifferenceUpdate, SupplierSettlementItem, SupplierSettlementItemData,
    SupplierSettlementItemId, SupplierSettlementStatement, SupplierSettlementStatementData,
    SupplierSettlementStatementId, SupplierSettlementStatementUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::supplier_fulfillment::dto::SortDir;

mod dto;

use self::dto::StatementListQuery;
pub use self::dto::{
    ConfirmSettlementRequest, CreateSettlementStatementRequest, ResolveDifferenceRequest, SettlementPageView,
    SubmitSettlementReviewRequest, SupplierSettlementDifferenceListParams, SupplierSettlementDifferenceView,
    SupplierSettlementItemListParams, SupplierSettlementItemView, SupplierSettlementStatementDetailView,
    SupplierSettlementStatementListParams, SupplierSettlementStatementView, VoidSettlementRequest,
};

/// 结算单列表筛选条件类型（经 `SupplierSettlementExt` 关联类型跨 crate 可达）。
type StatementFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementStatementFilter;
/// 结算明细列表筛选条件类型。
type ItemFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementItemFilter;
/// 结算差异列表筛选条件类型。
type DifferenceFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementDifferenceFilter;

/// 供应商结算服务。
///
/// 提供供应商周期结算单的创建、查询、复核/确认/作废与差异处理编排。
pub struct SupplierSettlementService {
    db: Database,
}

impl SupplierSettlementService {
    /// 创建供应商结算服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询供应商结算单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_no`/`supplier_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_statement_list(
        &self,
        params: &SupplierSettlementStatementListParams,
    ) -> Result<SettlementPageView<SupplierSettlementStatementView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = statement_filter(&query);
        let page = self
            .db
            .supplier_settlement_statements()
            .search_supplier_settlement_statements(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementStatementView {
                id: row.id,
                statement_no: row.statement_no,
                supplier_id: row.supplier_id.to_string(),
                period_start: row.period_start.to_string(),
                period_end: row.period_end.to_string(),
                external_bill_no: row.external_bill_no,
                external_bill_version: row.external_bill_version,
                erp_amount: row.erp_amount,
                supplier_amount: row.supplier_amount,
                difference_amount: row.difference_amount,
                status: row.status,
                prepared_by: row.prepared_by,
                reviewed_by: row.reviewed_by,
                confirmed_at: row.confirmed_at.map(|t| t.unix_secs()),
                payable_account_id: row.payable_account_id.map(|id| id.to_string()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(SettlementPageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询供应商结算单详情（结算单 + 全部明细 + 全部差异）。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_statement_detail(
        &self,
        id: &str,
    ) -> Result<SupplierSettlementStatementDetailView> {
        let statement = self.load_statement(id).await?;
        let items = self
            .db
            .supplier_settlement_items()
            .find_many_sorted(
                mongodb::bson::doc! { "statement_id": id },
                mongodb::bson::doc! { "created_at": 1 },
                &mut NoTransaction,
            )
            .await?;
        let item_id_list: Vec<String> = items.iter().map(|item| item.base.id.clone()).collect();
        let differences = self
            .db
            .supplier_settlement_differences()
            .find_many_sorted(
                mongodb::bson::doc! { "statement_item_id": { "$in": item_id_list } },
                mongodb::bson::doc! { "created_at": 1 },
                &mut NoTransaction,
            )
            .await?;

        Ok(SupplierSettlementStatementDetailView {
            statement: statement.into(),
            items: items.into_iter().map(settlement_item_view).collect(),
            differences: differences.into_iter().map(settlement_difference_view).collect(),
        })
    }

    /// 创建供应商结算草稿（幂等键：`statement_no`，§6.20）。
    ///
    /// 表头金额由明细派生（ERP 金额 = 明细 ERP 计算金额合计，供应商金额 = 账单金额
    /// 合计），明细构成恒等由实体校验（§6.20）；同事务写入结算单与全部明细。
    /// 重复提交（同一结算单号）返回原结算单当前视图。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人（经办人）
    ///
    /// # 返回
    /// 返回新建结算单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 履约订单/履约明细不存在
    /// * `ConflictError` - 结算单号重复（唯一索引透出）
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn create_statement(
        &self,
        req: CreateSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementView> {
        req.validate()?;
        if let Some(existing) = self
            .db
            .supplier_settlement_statements()
            .find_by_statement_no(&req.statement_no, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), statement_no = %req.statement_no, "创建结算单幂等命中");
            return Ok(existing.into());
        }
        self.ensure_settlement_items(&req).await?;
        let (statement, items) = self.build_statement(&req, actor)?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.create",
            "supplier_settlement_statement",
            statement.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let statement_for_tx = statement.clone();
        let items_for_tx = items.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement()
                        .create_statement_with_items(&statement_for_tx, &items_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(statement.into())
    }

    /// 提交结算复核（幂等：已在待复核返回当前视图）。
    ///
    /// 前置条件：存在未解决（`Pending`）差异时拒绝提交（§6.20/w27：全部差异必须
    /// 已有完整处理结论）；版本 CAS 冲突返回 409。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 提交请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后结算单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `ConflictError` - 版本冲突
    /// * `BusinessLogicError` - 存在未解决差异或当前状态不可提交
    pub async fn submit_review(
        &self,
        id: &str,
        req: SubmitSettlementReviewRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementView> {
        req.validate()?;
        let mut statement = self.load_statement(id).await?;
        if statement.status == SettlementStatus::PendingReview {
            return Ok(statement.into());
        }
        self.ensure_version(&statement, req.version)?;
        if self.has_open_difference(id).await? {
            return Err(Error::BusinessLogicError(
                "存在未解决差异，无法提交复核".to_string(),
            ));
        }
        statement.update(SupplierSettlementStatementUpdate {
            status: Some(SettlementStatus::PendingReview),
            ..Default::default()
        })?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.submit_review",
            "supplier_settlement_statement",
            id.to_string(),
        )?;
        self.update_statement_with_audit(&mut statement, &audit).await?;
        Ok(statement.into())
    }

    /// 确认结算（幂等：已在已确认返回当前视图；§8.4 第 6 条）。
    ///
    /// 同事务完成：锁定结算单并重验版本、确认状态更新（复核人/确认时间/应付账户）、
    /// 形成结算单应付（D19：应付账户 + 原始应付分录）、审计。
    /// 重复确认只形成一条应付事实（唯一索引 `(source_type, source_document_id)` 兜底）。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 确认请求（含期望版本与复核人）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后结算单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `ConflictError` - 版本冲突
    /// * `BusinessLogicError` - 状态不在待复核或存在未解决差异
    pub async fn confirm(
        &self,
        id: &str,
        req: ConfirmSettlementRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementView> {
        req.validate()?;
        let mut statement = self.load_statement(id).await?;
        if statement.status == SettlementStatus::Confirmed {
            return Ok(statement.into());
        }
        self.ensure_version(&statement, req.version)?;
        if statement.status != SettlementStatus::PendingReview {
            return Err(Error::BusinessLogicError("仅待复核结算单可确认".to_string()));
        }
        if self.has_open_difference(id).await? {
            return Err(Error::BusinessLogicError(
                "存在未解决差异，无法确认结算".to_string(),
            ));
        }
        let payable_account_id = PayableAccountId::new(next_id());
        statement.update(SupplierSettlementStatementUpdate {
            status: Some(SettlementStatus::Confirmed),
            reviewed_by: Some(req.reviewed_by),
            payable_account_id: Some(payable_account_id.clone()),
            ..Default::default()
        })?;
        let account = PayableAccount::new(
            payable_account_id,
            PayableAccountData {
                source_document_id: statement.statement_no.clone(),
                supplier_id: statement.supplier_id.clone(),
                source_type: PayableSourceType::SupplierSettlement,
                gross_total: statement.erp_amount,
                settled_total: Amount::from_str("0.00").expect("零是合法金额"),
                invoiceable_total: statement.erp_amount,
                invoiced_total: Amount::from_str("0.00").expect("零是合法金额"),
            },
            actor.id(),
        )?;
        let entry = PayableEntry::new(
            PayableEntryId::new(next_id()),
            PayableEntryData {
                payable_account_id: account.base.id.clone().into(),
                entry_type: PayableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: statement.erp_amount,
                due_date: statement.period_end,
                source_fact_type: "supplier_settlement".to_string(),
                source_document_id: statement.statement_no.clone(),
                source_revision_id: statement.base.id.clone(),
                source_sequence: 1,
                posted_at: Instant::now(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.confirm",
            "supplier_settlement_statement",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut statement_for_tx = statement.clone();
        let account_for_tx = account.clone();
        let entry_for_tx = entry.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_statements()
                        .update(&mut statement_for_tx, session)
                        .await?;
                    db.payable()
                        .create_payable_with_entry(&account_for_tx, &entry_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(statement.into())
    }

    /// 作废结算单（乐观锁；已确认可作废，已作废终态由实体守卫）。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 作废请求（含期望版本与原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后结算单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `ConflictError` - 版本冲突
    pub async fn void_statement(
        &self,
        id: &str,
        req: VoidSettlementRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementView> {
        req.validate()?;
        let mut statement = self.load_statement(id).await?;
        if statement.status == SettlementStatus::Voided {
            return Ok(statement.into());
        }
        self.ensure_version(&statement, req.version)?;
        statement.update(SupplierSettlementStatementUpdate {
            status: Some(SettlementStatus::Voided),
            ..Default::default()
        })?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.void",
            "supplier_settlement_statement",
            id.to_string(),
        )?;
        self.update_statement_with_audit(&mut statement, &audit).await?;
        Ok(statement.into())
    }

    /// 分页查询供应商结算明细列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_item_list(
        &self,
        params: &SupplierSettlementItemListParams,
    ) -> Result<SettlementPageView<SupplierSettlementItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ItemFilter {
            statement_id: query.statement_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_settlement_items()
            .search_supplier_settlement_items(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementItemView {
                id: row.id,
                statement_id: row.statement_id.to_string(),
                supplier_fulfillment_order_id: row.supplier_fulfillment_order_id.to_string(),
                supplier_fulfillment_item_id: row.supplier_fulfillment_item_id.to_string(),
                order_amount: row.order_amount,
                freight_amount: row.freight_amount,
                service_fee_amount: row.service_fee_amount,
                refund_amount: row.refund_amount,
                erp_calculated_amount: row.erp_calculated_amount,
                supplier_billed_amount: row.supplier_billed_amount,
                created_at: row.created_at,
            })
            .collect();

        Ok(SettlementPageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询供应商结算差异列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_item_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_difference_list(
        &self,
        params: &SupplierSettlementDifferenceListParams,
    ) -> Result<SettlementPageView<SupplierSettlementDifferenceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DifferenceFilter {
            statement_item_id: query.statement_item_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_settlement_differences()
            .search_supplier_settlement_differences(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementDifferenceView {
                id: row.id,
                statement_item_id: row.statement_item_id.to_string(),
                difference_type: row.difference_type,
                difference_amount: row.difference_amount,
                status: row.status,
                resolution: row.resolution,
                resolved_by: row.resolved_by,
                resolved_at: row.resolved_at.map(|t| t.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(SettlementPageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 登记结算差异处理结论（乐观锁；实体校验处理结果三元组成组约束）。
    ///
    /// # 参数
    /// * `id` - 结算差异 ID
    /// * `req` - 处理请求（含期望版本与结论状态）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回处理后差异的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    /// * `ConflictError` - 版本冲突
    pub async fn resolve_difference(
        &self,
        id: &str,
        req: ResolveDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementDifferenceView> {
        req.validate()?;
        let mut difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异不存在".to_string()))?;
        if difference.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        difference.update(SupplierSettlementDifferenceUpdate {
            status: Some(req.status),
            resolution: req.resolution,
            resolved_by: req.resolved_by,
            resolved_at: req.resolved_at.map(Instant::from_unix_secs),
        })?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.resolve_difference",
            "supplier_settlement_difference",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut difference_for_tx = difference.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_differences()
                        .update(&mut difference_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(settlement_difference_view(difference))
    }
}

impl SupplierSettlementService {
    /// 校验结算明细引用的履约订单与明细全部存在且归属一致（D32 跨域读取）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    ///
    /// # 错误
    /// * `NotFound` - 履约订单/履约明细不存在
    /// * `BusinessLogicError` - 履约明细不属于对应订单
    async fn ensure_settlement_items(&self, req: &CreateSettlementStatementRequest) -> Result<()> {
        let order_ids: Vec<String> = req
            .items
            .iter()
            .map(|item| item.supplier_fulfillment_order_id.to_string())
            .collect();
        let item_ids: Vec<String> = req
            .items
            .iter()
            .map(|item| item.supplier_fulfillment_item_id.to_string())
            .collect();
        let orders = self
            .db
            .supplier_fulfillment_orders()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": order_ids.clone() } },
                &mut NoTransaction,
            )
            .await?;
        if orders.len() != req.items.len() {
            return Err(Error::NotFound("供应商履约订单不存在".to_string()));
        }
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": item_ids } },
                &mut NoTransaction,
            )
            .await?;
        if items.len() != req.items.len() {
            return Err(Error::NotFound("供应商履约明细不存在".to_string()));
        }
        let order_id_set: std::collections::HashSet<String> = order_ids.into_iter().collect();
        if items
            .iter()
            .any(|item| !order_id_set.contains(item.supplier_fulfillment_order_id.as_ref()))
        {
            return Err(Error::BusinessLogicError(
                "履约明细不属于对应供应商子订单".to_string(),
            ));
        }
        Ok(())
    }

    /// 构建结算草稿事实（结算单 + 全部明细，表头金额由明细派生）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 审计操作人（经办人）
    ///
    /// # 返回
    /// 返回 `(结算单, 明细)` 二元组。
    ///
    /// # 错误
    /// 期间非法/金额恒等失败时返回 `LogicError`。
    fn build_statement(
        &self,
        req: &CreateSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<(SupplierSettlementStatement, Vec<SupplierSettlementItem>)> {
        let statement_id = SupplierSettlementStatementId::new(next_id());
        let items = req
            .items
            .iter()
            .map(|item| {
                let erp_calculated = item
                    .order_amount
                    .checked_add(item.freight_amount)
                    .checked_add(item.service_fee_amount)
                    .checked_sub(item.refund_amount);
                SupplierSettlementItem::new(
                    SupplierSettlementItemId::new(next_id()),
                    SupplierSettlementItemData {
                        statement_id: statement_id.clone(),
                        supplier_fulfillment_order_id: item.supplier_fulfillment_order_id.clone(),
                        supplier_fulfillment_item_id: item.supplier_fulfillment_item_id.clone(),
                        order_amount: item.order_amount,
                        freight_amount: item.freight_amount,
                        service_fee_amount: item.service_fee_amount,
                        refund_amount: item.refund_amount,
                        erp_calculated_amount: erp_calculated,
                        supplier_billed_amount: item.supplier_billed_amount,
                    },
                )
            })
            .collect::<std::result::Result<Vec<_>, entities::Error>>()
            .map_err(crate::errors::Error::from)?;
        let erp_amount = items.iter().fold(zero_amount(), |acc, item| {
            acc.checked_add(item.erp_calculated_amount)
        });
        let supplier_amount = items.iter().fold(zero_amount(), |acc, item| {
            acc.checked_add(item.supplier_billed_amount)
        });
        let statement = SupplierSettlementStatement::new(
            statement_id,
            SupplierSettlementStatementData {
                statement_no: req.statement_no.clone(),
                supplier_id: req.supplier_id.clone(),
                period_start: BusinessDate::from_str(&req.period_start)
                    .map_err(|_| Error::ValidationError("结算期间开始日期非法".to_string()))?,
                period_end: BusinessDate::from_str(&req.period_end)
                    .map_err(|_| Error::ValidationError("结算期间结束日期非法".to_string()))?,
                external_bill_no: req.external_bill_no.clone(),
                external_bill_version: req.external_bill_version.clone(),
                erp_amount,
                supplier_amount,
                status: SettlementStatus::Draft,
                prepared_by: actor.id().to_string(),
                reviewed_by: None,
                confirmed_at: None,
                payable_account_id: None,
            },
        )?;
        Ok((statement, items))
    }

    /// 判断结算单是否存在未解决（`Pending`）差异。
    ///
    /// # 参数
    /// * `statement_id` - 结算单 ID
    ///
    /// # 返回
    /// 存在未解决差异时返回 `true`。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn has_open_difference(&self, statement_id: &str) -> Result<bool> {
        let items = self
            .db
            .supplier_settlement_items()
            .find_many(
                mongodb::bson::doc! { "statement_id": statement_id },
                &mut NoTransaction,
            )
            .await?;
        let item_ids: Vec<String> = items.iter().map(|item| item.base.id.clone()).collect();
        if item_ids.is_empty() {
            return Ok(false);
        }
        let open = self
            .db
            .supplier_settlement_differences()
            .find_many(
                mongodb::bson::doc! {
                    "statement_item_id": { "$in": item_ids },
                    "status": SettlementDifferenceStatus::Pending.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(!open.is_empty())
    }

    /// 校验期望版本与当前版本一致（乐观锁前置校验）。
    ///
    /// # 参数
    /// * `statement` - 结算单实体
    /// * `expected` - 期望版本
    ///
    /// # 错误
    /// 版本不一致时返回 `ConflictError`。
    fn ensure_version(&self, statement: &SupplierSettlementStatement, expected: u64) -> Result<()> {
        if statement.base.version != expected {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    /// 在同一事务更新结算单并写审计。
    ///
    /// # 参数
    /// * `statement` - 结算单实体（就地更新）
    /// * `audit` - 审计日志
    ///
    /// # 错误
    /// 乐观锁冲突透出 `ConflictError`，提交结果未知透出 `OutcomeUnknown`。
    async fn update_statement_with_audit(
        &self,
        statement: &mut SupplierSettlementStatement,
        audit: &entities::AuditLog,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut statement_for_tx = statement.clone();
        let audit_for_tx = audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_statements()
                        .update(&mut statement_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<SupplierSettlementStatement, crate::errors::Error>(statement_for_tx)
                })
            })
            .await?;
        *statement = updated;
        Ok(())
    }

    /// 按 ID 加载未删除结算单。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 返回结算单实体。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    async fn load_statement(&self, id: &str) -> Result<SupplierSettlementStatement> {
        self.db
            .supplier_settlement_statements()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))
    }
}

/// 返回零金额（表头金额累加起点）。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 构建结算单列表筛选条件。
///
/// # 参数
/// * `query` - 归一化查询参数
///
/// # 返回
/// 返回仓储筛选条件。
fn statement_filter(query: &StatementListQuery) -> StatementFilter {
    StatementFilter {
        statement_no: query.statement_no.clone(),
        supplier_id: query.supplier_id.clone(),
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 从结算明细实体构造响应视图。
///
/// # 参数
/// * `item` - 结算明细实体
///
/// # 返回
/// 返回响应视图。
fn settlement_item_view(item: SupplierSettlementItem) -> SupplierSettlementItemView {
    SupplierSettlementItemView {
        id: item.base.id,
        statement_id: item.statement_id.to_string(),
        supplier_fulfillment_order_id: item.supplier_fulfillment_order_id.to_string(),
        supplier_fulfillment_item_id: item.supplier_fulfillment_item_id.to_string(),
        order_amount: item.order_amount,
        freight_amount: item.freight_amount,
        service_fee_amount: item.service_fee_amount,
        refund_amount: item.refund_amount,
        erp_calculated_amount: item.erp_calculated_amount,
        supplier_billed_amount: item.supplier_billed_amount,
        created_at: item.base.created_at,
    }
}

/// 从结算差异实体构造响应视图。
///
/// # 参数
/// * `difference` - 结算差异实体
///
/// # 返回
/// 返回响应视图。
fn settlement_difference_view(difference: SupplierSettlementDifference) -> SupplierSettlementDifferenceView {
    SupplierSettlementDifferenceView {
        id: difference.base.id,
        statement_item_id: difference.statement_item_id.to_string(),
        difference_type: difference.difference_type,
        difference_amount: difference.difference_amount,
        status: difference.status,
        resolution: difference.resolution,
        resolved_by: difference.resolved_by,
        resolved_at: difference.resolved_at.map(|t| t.unix_secs()),
        version: difference.base.version,
        created_at: difference.base.created_at,
    }
}
