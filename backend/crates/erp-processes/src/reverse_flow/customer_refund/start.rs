//! 客户退款审批启动、精确授权回放、8次fresh恢复与撤回编排。
use super::*;
impl ReturnsProcess {
    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    pub(super) async fn dispatch_customer_refund_start(
        &self,
        id: &str,
        refund: CustomerRefund,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::super::adapter::CustomerRefundAdapter,
    ) -> Result<CustomerRefundView> {
        let subject = customer_refund_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(services::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let organization_id = self.customer_responsible_org_id(&refund.customer_id).await?;
        let snapshot = build_customer_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let start =
            customer_refund_start_command(id, refund.approval_subject_version, actor.id(), &idempotency_key);
        let _ = start_approval_command_kind(&start);
        let _ = customer_refund_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &subject,
            refund.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_customer_refund_start_input(CustomerRefundStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: refund.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        let recovery_subject_version = refund.approval_subject_version;
        let persisted = persist_customer_refund_start(
            &self.db,
            CustomerRefundStartPersistInput {
                refund,
                actor: actor.clone(),
                id: id.to_string(),
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
            },
        )
        .await;
        if let Err(error) = persisted {
            if !error.command_may_have_committed() {
                return Err(error);
            }
            self.recover_customer_refund_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        self.reads().customer_refund_detail(id).await
    }
    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_customer_refund_start(
        &self,
        refund_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let refund_id = refund_id.to_string();
            let idempotency_key = idempotency_key.to_string();
            let actor = actor.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        ensure_return_start_actor_active(&db, &rbac, &actor, session).await?;
                        let refund = erp_returns::service::ReturnsService::new(db.clone())
                            .load_customer_refund(&refund_id, session)
                            .await?;
                        let customer = db
                            .customer_accounts()
                            .find_by_id(&refund.customer_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
                        let organization_id = customer_refund_responsible_org_id(customer.party_id.as_ref())?;
                        ensure_return_start_replay_authorized(
                            &db,
                            &rbac,
                            &actor,
                            DocumentType::CustomerRefund,
                            "customer_refund:submit",
                            &organization_id,
                            session,
                        )
                        .await?;
                        let binding = find_approval_binding(&db, &refund_id, session)
                            .await
                            .map_err(services::Error::from)?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = customer_refund_subject_ref(&refund_id)?;
                        replay_return_start_with_executor(
                            &db,
                            ReplayReturnStartInput {
                                document_type: DocumentType::CustomerRefund,
                                subject: &subject,
                                subject_version,
                                idempotency_key: &idempotency_key,
                                binding,
                                actor_id: actor.id(),
                            },
                            session,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(instance_id)) => return Ok(instance_id),
                Ok(None) => {}
                Err(error) if error.command_may_have_committed() => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
    /// 在任何当前状态/版本门禁前，以当前或下一主题版本精确回放既有启动。
    pub(super) async fn replay_customer_refund_start(
        &self,
        refund_id: &str,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<Option<String>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let refund_id = refund_id.to_string();
        let idempotency_key = idempotency_key.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    ensure_return_start_actor_active(&db, &rbac, &actor, session).await?;
                    let refund = erp_returns::service::ReturnsService::new(db.clone())
                        .load_customer_refund(&refund_id, session)
                        .await?;
                    let customer = db
                        .customer_accounts()
                        .find_by_id(&refund.customer_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
                    let organization_id = customer_refund_responsible_org_id(customer.party_id.as_ref())?;
                    ensure_return_start_replay_authorized(
                        &db,
                        &rbac,
                        &actor,
                        DocumentType::CustomerRefund,
                        "customer_refund:submit",
                        &organization_id,
                        session,
                    )
                    .await?;
                    let binding = find_approval_binding(&db, &refund_id, session)
                        .await
                        .map_err(services::Error::from)?;
                    let binding = require_frozen_binding(binding.as_ref())?;
                    let subject = customer_refund_subject_ref(&refund_id)?;
                    for subject_version in replay_subject_versions(refund.approval_subject_version)? {
                        if let Some(instance_id) = replay_return_start_with_executor(
                            &db,
                            ReplayReturnStartInput {
                                document_type: DocumentType::CustomerRefund,
                                subject: &subject,
                                subject_version,
                                idempotency_key: &idempotency_key,
                                binding,
                                actor_id: actor.id(),
                            },
                            session,
                        )
                        .await?
                        {
                            return Ok(Some(instance_id));
                        }
                    }
                    Ok(None)
                })
            })
            .await
    }
    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    pub(super) async fn persist_cancelled_customer_refund(
        &self,
        id: &str,
        refund: &mut CustomerRefund,
        req: &CancelCustomerRefundApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = customer_refund_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(services::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = customer_refund_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, refund.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_customer_refund_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_customer_refund_domain_action(refund, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "customer_refund.cancel_approval",
            "customer_refund",
            id.to_string(),
        )?;
        persist_customer_refund_cancel(
            &self.db,
            CustomerRefundCancelPersistInput {
                refund: refund.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await
    }
    /// 读取客户往来主体作为责任组织。
    ///
    /// # 错误
    /// 客户不存在或往来主体为空时返回错误。
    async fn customer_responsible_org_id(&self, customer_id: &CustomerAccountId) -> Result<String> {
        load_customer_responsible_org_id(&self.db, customer_id).await
    }
}
