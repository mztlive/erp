//! 销售单命令用例：建单、保存草稿、提交、作废。

use std::collections::HashSet;

use database::{
    AccessControlExt, CatalogExt, ContractExt, CustomerExt, Executor, NoTransaction, SalesOrderExt,
    SalesReviewExt, Transactional, WorkItemExt,
};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    ProcurementConfirmationId, SalesOrderId, SalesOrderReviewId, SalesOrderSubmissionId,
    SalesOrderWorkingCopyId, WorkItemId,
};
use entities::sales_order::{
    BusinessType, LineType, SalesOrder, SalesOrderData, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine,
    SalesOrderWorkingCopyUpdate, WorkingPurpose,
};
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationData, SalesOrderReview, SalesOrderReviewData,
    SalesReviewStage,
};
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use validator::Validate;

use super::dto::{
    CreateSalesOrderRequest, SalesOrderCreateIntent, SalesOrderDetailView, SalesOrderDraftLineRequest,
    SaveWorkingCopyRequest, SubmissionView, SubmitSalesOrderRequest, VoidSalesOrderRequest, WorkingCopyView,
};
use super::mapper::{
    build_stable_lines, build_submission, build_submission_lines, build_working_copy,
    build_working_copy_lines, draft_hash, header_snapshot, submission_view,
};
use super::pricing::line_totals;
use super::SalesOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SalesOrderService {
    /// 创建销售单（订单 + 稳定明细 + 首次提交工作副本原子形成；`intent=SUBMIT`
    /// 时随后立即提交）。
    ///
    /// 表头金额三元组由服务端按 §4.2 铁律 2 汇总**已舍入**的行金额，客户端不可
    /// 指定；跨域校验客户（D08）与合同（D12）存在性。`order_no` 唯一性由唯一
    /// 索引兜底（重复幂等键提交 → 409）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回销售单详情视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败或行字段组缺失
    /// * `NotFound` - 客户/合同不存在
    /// * `BusinessLogicError` - 客户已停用
    /// * `ConflictError` - order_no 重复
    pub async fn create_sales_order(
        &self,
        req: CreateSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        self.ensure_sellable_draft_lines(&req.draft.lines).await?;
        let customer = self
            .db
            .customer_accounts()
            .find_by_id(&req.customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError(
                "客户已停用，禁止创建新销售单".to_string(),
            ));
        }
        if let Some(contract_id) = &req.contract_id {
            let contract = self
                .db
                .contracts()
                .find_by_id(contract_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
            if contract.customer_id != req.customer_id {
                return Err(Error::ValidationError(
                    "销售单引用的合同不属于所选客户".to_string(),
                ));
            }
        }

        let order = SalesOrder::new(
            SalesOrderId::new(next_id()),
            SalesOrderData {
                order_no: req.order_no,
                business_type: req.business_type,
                origin_system: entities::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: req.customer_id,
                contract_id: req.contract_id,
                settlement_party_id: req.settlement_party_id,
                source_status_code: None,
            },
            actor.id(),
        )?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let stable_lines = build_stable_lines(&order_id, &req.draft.lines)?;
        let (working_copy, working_copy_lines) =
            build_working_copy(&order, &stable_lines, &req.draft, 1, actor)?;

        let audit = actor
            .clone()
            .resource_log("sales_order.create", "sales_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let lines_for_tx = stable_lines.clone();
        let working_copy_for_tx = working_copy.clone();
        let working_copy_lines_for_tx = working_copy_lines.clone();
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&working_copy_lines)?;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    db.sales_orders().create(&order_for_tx, session).await?;
                    for line in &lines_for_tx {
                        db.sales_order_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .create(&working_copy_for_tx, session)
                        .await?;
                    for line in &working_copy_lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        if req.intent == SalesOrderCreateIntent::Submit {
            self.submit_sales_order(
                &order.base.id,
                SubmitSalesOrderRequest {
                    version: 1,
                    idempotency_key: req.idempotency_key,
                },
                actor,
            )
            .await?;
        }
        self.sales_order_detail(&order.base.id).await
    }

    /// 保存草稿（整表头覆盖 + 明细整批替换，乐观锁语义）。
    ///
    /// 采购/销售驳回后订单回到草稿，但首次提交工作副本已是 `Submitted` 终态时，
    /// 会新开一份 `Editing` 副本，而不是返回「有效工作副本不存在」。
    /// 已有有效副本时，`req.version` 必须与当前工作副本版本一致；新开副本不校验
    /// 该版本（前端在无副本时会把销售单版本误当成草稿版本）。
    /// 行替换在事务内「软删旧行 + 写入新行」原子完成。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 保存请求（含期望版本与草稿内容）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回保存后的工作副本视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 非草稿，或已有副本但期望版本不一致
    pub async fn save_working_copy(
        &self,
        id: &str,
        req: SaveWorkingCopyRequest,
        actor: &AuditActor,
    ) -> Result<WorkingCopyView> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        self.ensure_sellable_draft_lines(&req.draft.lines).await?;
        let (mut working_copy, stable, opened_new) = self
            .load_or_reopen_first_submission_working_copy(&order, req.version, &req.draft, actor)
            .await?;
        if opened_new {
            return self.working_copy_view(&working_copy).await;
        }

        let snapshot = header_snapshot(&req.draft)?;
        let created_stable_lines = stable.created;
        let lines = build_working_copy_lines(
            &order_id,
            &working_copy.base.id.clone().into(),
            &stable.all,
            &req.draft.lines,
        )?;
        let (gross, net, tax) = line_totals(&lines);
        let next_version = working_copy.draft_version + 1;
        working_copy.update(
            SalesOrderWorkingCopyUpdate {
                content_hash: Some(draft_hash(&working_copy.base.id, next_version)),
                customer_id: Some(order.customer_id.clone()),
                contract_id: order.contract_id.clone(),
                settlement_party_id: Some(order.settlement_party_id.clone()),
                snapshot: Some(snapshot),
                project_name: req.draft.project_name.clone(),
                business_remark: req.draft.business_remark.clone(),
                voucher_category_sku_id: req.draft.voucher_category_sku_id.clone(),
                voucher_expiry_at: req
                    .draft
                    .voucher_expiry_at
                    .map(|secs| Instant::from_unix_secs(secs as i64)),
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
            },
            actor.id(),
        )?;
        working_copy.save_draft(
            draft_hash(&working_copy.base.id, next_version),
            req.draft.editor_user_id.clone(),
        )?;

        let old_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&working_copy.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let audit = actor
            .clone()
            .resource_log("sales_order.save_draft", "sales_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let lines_for_tx = lines.clone();
        let created_stable_for_tx = created_stable_lines;
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&lines)?;
        let working_copy = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    for line in &created_stable_for_tx {
                        db.sales_order_lines().create(line, session).await?;
                    }
                    for mut old in old_lines {
                        db.sales_order_working_copy_lines()
                            .soft_delete(&mut old, session)
                            .await?;
                    }
                    for line in &lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .update(&mut working_copy, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderWorkingCopy, crate::errors::Error>(working_copy)
                })
            })
            .await?;

        self.working_copy_view(&working_copy).await
    }

    /// 提交销售单（冻结提交快照并推进审核轨；重复提交幂等返回既有提交）。
    ///
    /// 跨集合事务内写入 `sales_order_submission(+_line)`、CAS 锁定工作副本、
    /// 推进销售单审核轨，并按业务性质派发：
    /// - 实物及服务 → 创建采购二次确认批次（`procurement_confirmation`，
    ///   待处理）+ `PROCUREMENT_CONFIRMATION` 待办（W07 队列）；
    /// - 卡券 → 创建 `SALES_LEADER` 阶段审批记录 + `CARD_SALES_MANAGER_APPROVAL`
    ///   待办。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 提交请求（含期望草稿版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交快照视图；已提交过的草稿幂等返回既有提交。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或有效工作副本不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn submit_sales_order(
        &self,
        id: &str,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmissionView> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("有效工作副本不存在".to_string()))?;
        if working_copy.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if let Some(existing) = self
            .db
            .sales_order_submissions()
            .find_one_by_field(
                "working_copy_id",
                working_copy.base.id.as_str(),
                &mut NoTransaction,
            )
            .await?
        {
            let existing_id = SalesOrderSubmissionId::new(existing.base.id.clone());
            let existing_lines = self
                .db
                .sales_order_submission_lines()
                .list_lines_by_submissions(&[existing_id], &mut NoTransaction)
                .await?;
            return Ok(submission_view(existing, existing_lines));
        }

        let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
        let copy_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
            .await?;
        self.ensure_sellable_working_copy_lines(&copy_lines).await?;
        let existing_submissions = self
            .db
            .sales_order_submissions()
            .find_many(mongodb::bson::doc! { "sales_order_id": id }, &mut NoTransaction)
            .await?;
        let submission_no = existing_submissions
            .iter()
            .map(|submission| submission.submission_no)
            .max()
            .unwrap_or(0)
            + 1;
        let submission = build_submission(&working_copy, &copy_lines, submission_no, actor)?;
        let submission_lines = build_submission_lines(&submission, &copy_lines)?;
        let mut order_mut = order;
        order_mut.submit_for_review(actor.id())?;
        working_copy.submit()?;

        let (review, confirmation, work_item) = match order_mut.business_type {
            BusinessType::Voucher => {
                let review = SalesOrderReview::new(
                    SalesOrderReviewId::new(next_id()),
                    SalesOrderReviewData {
                        sales_order_id: order_id.clone(),
                        submission_id: submission.base.id.clone().into(),
                        review_stage: SalesReviewStage::SalesLeader,
                    },
                    actor.id(),
                )?;
                let item = WorkItem::new(
                    WorkItemId::new(next_id()),
                    WorkItemData {
                        work_item_type: WorkItemType::CardSalesManagerApproval,
                        business_object_type: "sales_order_review".to_string(),
                        business_object_id: review.base.id.clone(),
                        subject_version: Some(submission.base.id.clone()),
                        owner_role: Some("sales_leader".to_string()),
                        owner_user_id: None,
                        priority: WorkItemPriority::High,
                        due_at: None,
                        reason_code: Some("review_dispatched".to_string()),
                        impact_summary: Some("卡券销售提交待销售领导审批".to_string()),
                        completion_action: "DECIDE_REVIEW".to_string(),
                    },
                )?;
                (Some(review), None, Some(item))
            }
            BusinessType::GoodsService => {
                let confirmation = ProcurementConfirmation::new(
                    ProcurementConfirmationId::new(next_id()),
                    ProcurementConfirmationData {
                        sales_order_id: order_id.clone(),
                        submission_id: submission.base.id.clone().into(),
                        reject_reason_code: None,
                        comment: None,
                    },
                    actor.id(),
                )?;
                let item = WorkItem::new(
                    WorkItemId::new(next_id()),
                    WorkItemData {
                        work_item_type: WorkItemType::ProcurementConfirmation,
                        business_object_type: "procurement_confirmation".to_string(),
                        business_object_id: confirmation.base.id.clone(),
                        subject_version: Some(submission.base.id.clone()),
                        owner_role: Some("procurement".to_string()),
                        owner_user_id: None,
                        priority: WorkItemPriority::High,
                        due_at: None,
                        reason_code: Some("procurement_confirmation_dispatched".to_string()),
                        impact_summary: Some(format!("采购二次确认：销售提交 {}", submission.submission_no)),
                        completion_action: "DECIDE_CONFIRMATION".to_string(),
                    },
                )?;
                (None, Some(confirmation), Some(item))
            }
        };
        let audit = actor
            .clone()
            .resource_log("sales_order.submit", "sales_order", order_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut wc_for_tx = working_copy.clone();
        let submission_for_tx = submission.clone();
        let lines_for_tx = submission_lines.clone();
        let confirmation_for_tx = confirmation.clone();
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&copy_lines)?;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    db.sales_order()
                        .submit_working_copy(&mut wc_for_tx, &submission_for_tx, &lines_for_tx, session)
                        .await?;
                    db.sales_orders().update(&mut order_mut, session).await?;
                    if let Some(review) = &review {
                        db.sales_order_reviews().create(review, session).await?;
                    }
                    if let Some(item) = &work_item {
                        db.work_items().create(item, session).await?;
                    }
                    if let Some(confirmation) = &confirmation_for_tx {
                        db.sales_review()
                            .create_procurement_confirmation_with_lines(confirmation, &[], session)
                            .await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(submission_view(submission, submission_lines))
    }

    /// 校验请求草稿中的实物及服务行仍引用公司商品池内的精确 SKU 修订。
    ///
    /// # 参数
    /// * `lines` - 草稿行请求
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一 `sku_id + sku_revision_id` 不再可销售时返回校验错误。
    async fn ensure_sellable_draft_lines(&self, lines: &[SalesOrderDraftLineRequest]) -> Result<()> {
        let refs = lines
            .iter()
            .filter_map(|line| line.goods.as_ref())
            .map(|goods| (goods.sku_id.to_string(), goods.sku_revision_id.to_string()))
            .collect::<Vec<_>>();
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 提交前重新校验已保存工作副本的精确 SKU 修订资格。
    ///
    /// # 参数
    /// * `lines` - 已保存工作副本行
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 缺 SKU/修订或引用失效时返回校验错误。
    async fn ensure_sellable_working_copy_lines(&self, lines: &[SalesOrderWorkingCopyLine]) -> Result<()> {
        let refs = Self::sellable_working_copy_refs(lines)?;
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 从工作副本行提取必须成对存在的销售 SKU 与修订引用。
    ///
    /// # 参数
    /// * `lines` - 工作副本行
    ///
    /// # 返回
    /// 返回 `(sku_id, sku_revision_id)` 列表。
    ///
    /// # 错误
    /// 实物行缺少 SKU 或修订身份时返回校验错误。
    pub(super) fn sellable_working_copy_refs(
        lines: &[SalesOrderWorkingCopyLine],
    ) -> Result<Vec<(String, String)>> {
        let refs = lines
            .iter()
            .filter(|line| line.line_type == LineType::GoodsService)
            .map(|line| {
                let sku_id = line
                    .sku_id
                    .as_ref()
                    .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU", line.line_no)))?;
                let revision_id = line
                    .sku_revision_id
                    .as_ref()
                    .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
                Ok((sku_id.to_string(), revision_id.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(refs)
    }

    /// 批量执行公司商品池资格校验并对缺失引用 fail-closed。
    ///
    /// # 参数
    /// * `refs` - `(sku_id, sku_revision_id)` 列表
    /// * `executor` - 事务会话或 `NoTransaction`
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一引用不在当日可销售集合中时返回校验错误。
    pub(super) async fn ensure_sellable_refs(
        &self,
        refs: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if refs.is_empty() {
            return Ok(());
        }
        let expected = refs.iter().cloned().collect::<HashSet<_>>();
        let qualified = self
            .db
            .catalog()
            .find_sellable_sku_refs(refs, BusinessDate::today(), executor)
            .await?
            .into_iter()
            .map(|row| (row.sku_id, row.sku_revision_id))
            .collect::<HashSet<_>>();
        let mut invalid = expected
            .difference(&qualified)
            .map(|(sku_id, _)| sku_id.clone())
            .collect::<Vec<_>>();
        invalid.sort();
        if invalid.is_empty() {
            Ok(())
        } else {
            Err(crate::catalog::sellable_sku_invalid_error(&invalid))
        }
    }

    /// 作废销售单草稿（主状态 `DRAFT → VOIDED`；放弃有效工作副本）。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后的销售单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn void_sales_order(
        &self,
        id: &str,
        req: VoidSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        order.void(actor.id())?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?;
        if let Some(copy) = &mut working_copy {
            copy.abandon()?;
        }
        let audit = actor
            .clone()
            .resource_log("sales_order.void", "sales_order", id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_orders().update(&mut order, session).await?;
                    if let Some(copy) = &mut working_copy {
                        db.sales_order_working_copies().update(copy, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_order_detail(id).await
    }
}
