//! 采购变更发起、提交、生效与查询。

use database::{AccessControlExt, CostExt, NoTransaction, PayableExt, PurchaseOrderExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{PurchaseChangeOrderId, PurchaseChangeSubmissionId};
use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus, PurchaseChangeOrderUpdate,
    PurchaseChangeSubmission, PurchaseChangeSubmissionData, PurchaseOrder, PurchaseOrderRevision,
    PurchaseOrderStatus, SubmissionStatus,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{
    EffectPurchaseChangeRequest, PageView, PurchaseChangeEffectResult, PurchaseChangeOrderListParams,
    PurchaseChangeOrderView, PurchaseChangeSubmitResult, SavePurchaseOrderLine, StartPurchaseChangeRequest,
    StartPurchaseChangeResult, SubmitPurchaseChangeRequest,
};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use entities::document_registry::DocumentType;

impl PurchaseOrderService {
    /// 发起采购变更（基于当前生效版本创建变更单）。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 发起请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单结果。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `ConflictError` - 版本不一致或已存在进行中变更
    /// * `BusinessLogicError` - 采购单未生效
    pub async fn start_change(
        &self,
        id: &str,
        req: StartPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<StartPurchaseChangeResult> {
        req.validate()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::Effective
            && order.stable.status != PurchaseOrderStatus::PartiallyExecuted
        {
            return Err(Error::BusinessLogicError(
                "只有已生效的采购单可以发起变更".to_string(),
            ));
        }
        let base_revision_id = order
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("采购单没有生效版本，不能发起变更".to_string()))?;
        let has_in_progress = self
            .db
            .purchase_change_orders()
            .exists(
                mongodb::bson::doc! {
                    "purchase_order_id": id,
                    "status": { "$in": [
                        PurchaseChangeOrderStatus::Draft.as_str(),
                        PurchaseChangeOrderStatus::PendingWarehouseImpact.as_str(),
                        PurchaseChangeOrderStatus::PendingFinanceReview.as_str(),
                    ]},
                },
                &mut NoTransaction,
            )
            .await?;
        if has_in_progress {
            return Err(Error::ConflictError(
                "存在进行中的采购变更，不能重复发起".to_string(),
            ));
        }
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;

        let change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new(next_id()),
            PurchaseChangeOrderData {
                purchase_order_id: order.base.id.clone().into(),
                base_revision_id: entities::ids::PurchaseOrderRevisionId::new(base_revision.base.id.clone()),
                reason: req.reason.clone(),
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.create",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let document = new_registered_document(&change.base.id, DocumentType::PurchaseChangeOrder, "")?;
        let db = self.db.clone();
        let client = db.client().clone();
        let change_for_tx = change.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_change_orders()
                        .create(&change_for_tx, session)
                        .await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(StartPurchaseChangeResult {
            change_id: change.base.id.clone(),
            base_revision_id: base_revision.base.id.clone(),
            base_revision_no: base_revision.revision.revision_no,
            lock_version: order.base.version,
            reference: format!("CHANGE-V{}", base_revision.revision.revision_no),
        })
    }

    /// 提交采购变更目标内容（形成不可变变更提交）。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 提交请求（目标完整头、行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更提交结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 版本不一致或重复提交
    pub async fn submit_change(
        &self,
        change_id: &str,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeSubmitResult> {
        req.validate()?;
        let mut change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        self.ensure_version(&change, req.expected_lock_version)?;
        if change.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::ConflictError("变更单已提交，请勿重复提交".to_string()));
        }
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;

        let supplier_name = self
            .resolve_supplier_name(&order.supplier_id)
            .await?
            .unwrap_or_else(|| order.supplier_id.to_string());
        let submission = self
            .build_change_submission(&change, &order, &base_revision, &supplier_name, &req)
            .await?;
        let lines = self
            .build_change_submission_lines(&submission.base.id.clone(), &req.lines)
            .await?;
        let mut submission_mut = submission.clone();
        submission_mut.submit(Instant::now(), actor.id())?;

        let change_update = PurchaseChangeOrderUpdate {
            current_submission_id: Some(submission.base.id.clone().into()),
            target_content_hash: Some(content_fingerprint(&req.lines)),
            status: Some(PurchaseChangeOrderStatus::PendingWarehouseImpact),
            ..Default::default()
        };
        change.update(change_update, actor.id())?;

        let audit = actor.clone().resource_log(
            "purchase_change_order.submit",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let change_for_tx = change.clone();
        let submission_for_tx = submission_mut.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_change_submission(
                            &mut change_for_tx.clone(),
                            &submission_for_tx,
                            &lines,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseChangeSubmitResult {
            change_id: change.base.id.clone(),
            submission_id: submission.base.id.clone(),
            submission_no: submission.submission_no.clone(),
            status: change.stable.status.as_str().to_string(),
            lock_version: change.base.version,
            reference: format!("CS-{}", submission.submission_no),
        })
    }

    /// 采购变更生效（§8.1.3 采购部分）。
    ///
    /// 单事务：基准版本仍为当前版本 → 复制目标提交为新采购版本与版本行 →
    /// 形成销售分配 → 追加应付与成本差额 → 推进采购当前版本指针 →
    /// 变更单置为生效。不修改已发生事实。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 生效请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单/提交不存在
    /// * `ConflictError` - 版本不一致或重复生效
    /// * `BusinessLogicError` - 基准版本已不是当前版本
    pub async fn effect_change(
        &self,
        change_id: &str,
        req: EffectPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeEffectResult> {
        req.validate()?;
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        self.ensure_version(&change, req.expected_lock_version)?;
        if change.stable.status == PurchaseChangeOrderStatus::Effective {
            return Err(Error::ConflictError("变更单已生效，请勿重复操作".to_string()));
        }
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        if change.base_revision_id.to_string()
            != order.stable.current_revision_id.as_deref().unwrap_or_default()
        {
            return Err(Error::BusinessLogicError(
                "基准版本已不是当前版本，变更不能生效".to_string(),
            ));
        }
        let submission = self
            .db
            .purchase_change_submissions()
            .find_by_id(&req.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError("变更提交已处理，请勿重复生效".to_string()));
        }
        let lines = self
            .db
            .purchase_change_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_change_submission_id": &req.submission_id },
                &mut NoTransaction,
            )
            .await?;

        let new_revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_change_revision(&order, &submission, &lines, new_revision_no)
            .await?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        let delta = self
            .build_change_deltas(&order, &base_revision, &revision)
            .await?;

        let audit = actor.clone().resource_log(
            "purchase_change_order.effect",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let actor_id = actor.id().to_string();
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let change_for_tx = change.clone();
        let submission_for_tx = submission.clone();
        let revision_for_tx = revision.clone();
        let payable_delta_id = delta.0.as_ref().map(|(_, entry)| entry.base.id.clone());
        let cost_deltas = delta.1.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_effective_revision(&revision_for_tx, &revision_lines, session)
                        .await?;
                    let mut order_mut = order_for_tx.clone();
                    order_mut.stable.current_revision_id = Some(revision_for_tx.base.id.clone());
                    db.purchase_orders().update(&mut order_mut, session).await?;
                    if let Some((account, entry)) = &delta.0 {
                        db.payable()
                            .create_payable_with_entry(account, entry, session)
                            .await?;
                    }
                    for entry in &cost_deltas {
                        db.cost()
                            .create_cost_entry_with_allocations(entry, Vec::new(), session)
                            .await?;
                    }
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.status = SubmissionStatus::Approved;
                    db.purchase_change_submissions()
                        .update(&mut submission_mut, session)
                        .await?;
                    let mut change_mut = change_for_tx.clone();
                    change_mut.update(
                        PurchaseChangeOrderUpdate {
                            effective_revision_id: Some(revision_for_tx.base.id.clone().into()),
                            status: Some(PurchaseChangeOrderStatus::Effective),
                            ..Default::default()
                        },
                        &actor_id,
                    )?;
                    db.purchase_change_orders()
                        .update(&mut change_mut, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseChangeEffectResult {
            change_id: change.base.id.clone(),
            revision_id: revision.base.id.clone(),
            revision_no: new_revision_no,
            payable_delta_entry_id: payable_delta_id,
            purchase_order_lock_version: order.base.version,
            reference: format!("EFFECT-V{new_revision_no}"),
        })
    }

    /// 分页查询采购变更单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法
    pub async fn change_order_list(
        &self,
        params: &PurchaseChangeOrderListParams,
    ) -> Result<PageView<PurchaseChangeOrderView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            super::dto::normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
        let mut filter = mongodb::bson::doc! {};
        if let Some(purchase_order_id) = params
            .purchase_order_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            filter.insert("purchase_order_id", purchase_order_id);
        }
        if let Some(status) = params.status.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            filter.insert("status", status);
        }
        let sort_doc = mongodb::bson::doc! { sort_by: if matches!(sort_dir, super::dto::SortDir::Asc) { 1i32 } else { -1i32 } };
        let items = self
            .db
            .purchase_change_orders()
            .find_many_sorted(filter.clone(), sort_doc, &mut NoTransaction)
            .await?;
        let total = items.len() as i64;
        let start = ((page - 1) * u64::from(page_size)) as usize;
        let views = items
            .into_iter()
            .skip(start)
            .take(page_size as usize)
            .map(|change| PurchaseChangeOrderView {
                id: change.base.id.clone(),
                purchase_order_id: change.purchase_order_id.to_string(),
                base_revision_id: change.base_revision_id.to_string(),
                reason: change.reason.clone(),
                status: change.stable.status.as_str().to_string(),
                current_submission_id: change.current_submission_id.as_ref().map(ToString::to_string),
                effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
                version: change.base.version,
                created_at: change.base.created_at,
            })
            .collect();
        Ok(PageView {
            items: views,
            total,
            page,
            page_size,
        })
    }

    /// 查询采购变更单详情。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    ///
    /// # 返回
    /// 返回变更单视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    pub async fn change_order_detail(&self, id: &str) -> Result<PurchaseChangeOrderView> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        Ok(PurchaseChangeOrderView {
            id: change.base.id.clone(),
            purchase_order_id: change.purchase_order_id.to_string(),
            base_revision_id: change.base_revision_id.to_string(),
            reason: change.reason.clone(),
            status: change.stable.status.as_str().to_string(),
            current_submission_id: change.current_submission_id.as_ref().map(ToString::to_string),
            effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
            version: change.base.version,
            created_at: change.base.created_at,
        })
    }

    /// 构建变更提交（表头取自目标内容，提交动作由调用方冻结审计人）。
    async fn build_change_submission(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        _supplier_name: &str,
        req: &SubmitPurchaseChangeRequest,
    ) -> Result<PurchaseChangeSubmission> {
        let (gross, net, tax) = self.compute_request_totals(&req.lines).await?;
        let payment_term_code = req
            .payment_term_code
            .clone()
            .unwrap_or_else(|| base_revision.payment_term_snapshot.payment_term_code.clone());
        let payment_term_snapshot = self.payment_term_snapshot(&payment_term_code).await?;
        let next_no = self.next_change_submission_no(change).await?;
        PurchaseChangeSubmission::new(
            PurchaseChangeSubmissionId::new(next_id()),
            PurchaseChangeSubmissionData {
                purchase_change_order_id: change.base.id.clone().into(),
                submission_no: next_no,
                base_revision_id: change.base_revision_id.clone(),
                supplier_id: order.supplier_id.clone(),
                purchase_type: order.purchase_type,
                fulfillment_responsibility: order.fulfillment_responsibility,
                supplier_revision_id: base_revision.supplier_revision_id.clone(),
                supplier_snapshot: base_revision.supplier_snapshot.clone(),
                payment_term_snapshot,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )
        .map_err(Into::into)
    }

    /// 计算下一个变更提交序号。
    async fn next_change_submission_no(&self, change: &PurchaseChangeOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_change_submissions()
            .find_many(
                mongodb::bson::doc! { "purchase_change_order_id": change.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        let max_no = existing
            .iter()
            .filter_map(|submission| {
                submission
                    .submission_no
                    .strip_prefix("CS-")
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(format!("CS-{:06}", max_no + 1))
    }
}

/// 内容指纹（Debug 形态 SipHash 十六进制；同二进制内稳定，用于变更目标内容比对）。
fn content_fingerprint(lines: &[SavePurchaseOrderLine]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{:?}", lines).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
