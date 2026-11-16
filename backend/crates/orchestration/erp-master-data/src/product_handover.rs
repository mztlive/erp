//! 商品维护人交接：目标资格、组织启用、幂等收据。

use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::AuditAction;
use erp_audit::execution::{AuditedCommand, AuditedWrite, run_audited_event};
use erp_catalog::entity::handover_receipt::{
    HANDOVER_ACTION, HANDOVER_LABEL, HANDOVER_RESOURCE, ProductHandoverReceipt,
};
use erp_catalog::repository::handover_receipt::ProductHandoverReceiptExt;
use erp_catalog::{
    CatalogAccess, CatalogService, HandoverCandidateView, HandoverProductRequest, HandoverProductView,
};
use erp_identity::service::organization_access::ensure_org_enabled;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{catalog as catalog_permissions, catalog};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use crate::handover_common::{
    HANDOVER_AUDIT_FIELDS, HandoverEventFacts, handover_content, handover_context, qualified_candidates,
    target_qualified, trimmed_idempotency_key,
};
use crate::{Error, Result};

/// 应用装配的范围和写入合同；两个服务必须采用同一授权来源。
#[derive(Clone)]
pub struct CatalogHandoverPorts {
    access: CatalogAccess,
    service: CatalogService,
}
impl CatalogHandoverPorts {
    /// 绑定完成装配的领域服务，不执行读取或写入。
    /// # 参数
    /// * `access` - 当前范围服务。
    /// * `service` - 使用相同范围 Port 的写入服务。
    /// # 返回
    /// 返回当前命令的领域合同。
    /// # 错误
    /// 不返回错误。
    pub fn new(access: CatalogAccess, service: CatalogService) -> Self {
        Self { access, service }
    }
}

/// 显式交接商品维护人；目标须有效且具备 `product:update`。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `ports` - 应用注入且采用同一授权来源的范围与写入服务。
/// * `id` - 商品 ID
/// * `req` - 交接请求
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回交接后责任视图。
///
/// # 错误
/// 账号失效、无维护资格、版本冲突或同幂等键异载荷时拒绝。
pub async fn handover_product(
    db: Database,
    rbac: Arc<AuthorizationService>,
    ports: CatalogHandoverPorts,
    id: &str,
    req: HandoverProductRequest,
    actor: &AuditActor,
) -> Result<HandoverProductView> {
    req.validate()?;
    let key = trimmed_idempotency_key(&req.idempotency_key)?;
    let fingerprint = handover_fingerprint(actor.id(), id, &req, &key)?;
    let command = CommandReceipt::from_resource_parts(
        "product-handover-",
        actor.id(),
        HANDOVER_ACTION,
        HANDOVER_RESOURCE,
        id,
        &key,
        [fingerprint],
    )?;
    let context = handover_context(
        actor,
        &command,
        AuditAction {
            code: HANDOVER_ACTION,
            resource_type: HANDOVER_RESOURCE,
            label: HANDOVER_LABEL,
            version: 1,
            allowed_fields: HANDOVER_AUDIT_FIELDS,
        },
    )?;
    let operation = ProductHandoverCommand {
        db: db.clone(),
        rbac,
        ports,
        id: id.to_string(),
        req,
        actor: actor.clone(),
        command,
        event_id: context.event_id().to_string(),
    };
    run_audited_event(&db, context, operation).await
}

/// 查询当前商品可交接的合格目标候选。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `ports` - 应用注入且采用同一授权来源的范围与写入服务。
/// * `id` - 商品 ID
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回有效且具备 `product:update` 的账号。
///
/// # 错误
/// 对象不可见时拒绝。
pub async fn handover_candidates(
    db: Database,
    rbac: Arc<AuthorizationService>,
    ports: CatalogHandoverPorts,
    id: &str,
    actor: &AuditActor,
) -> Result<Vec<HandoverCandidateView>> {
    let client = db.client().clone();
    let actor = actor.clone();
    let id = id.to_string();
    client
        .with_transaction(move |executor| {
            Box::pin(async move {
                let product = ports
                    .access
                    .require_product(&actor, &catalog_permissions::PRODUCT_PRODUCT_UPDATE, &id, executor)
                    .await?;
                Ok(qualified_candidates(
                    &db,
                    &rbac,
                    &product.maintainer_user_id,
                    &catalog::PRODUCT_PRODUCT_UPDATE,
                    executor,
                )
                .await?
                .into_iter()
                .map(|candidate| HandoverCandidateView {
                    user_id: candidate.user_id,
                    display_name: candidate.display_name,
                    account: candidate.account,
                })
                .collect())
            })
        })
        .await
}

/// 在原执行器中匹配独立回执，并重验当前访问读取当前责任视图。
///
/// # 参数
/// * `db` - 当前数据库。
/// * `rbac` - 当前权限快照。
/// * `command` - 当前请求的结构化命令身份。
/// * `id` - 领域目标。
/// * `actor` - 认证操作人。
/// * `executor` - 当前查证或事务执行器。
/// # 返回
/// 无回执时返回 None，命中时返回当前责任视图。
/// # 错误
/// 指纹冲突、回执损坏、权限拒绝或持久化失败时返回错误。
async fn replay(
    db: &Database,
    access: &CatalogAccess,
    command: &CommandReceipt,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<HandoverProductView>> {
    let Some(receipt) =
        db.product_handover_receipts().find_by_id_including_deleted(command.id(), executor).await?
    else {
        return Ok(None);
    };
    receipt.ensure_matches(command)?;
    let product =
        access.require_product(actor, &catalog_permissions::PRODUCT_PRODUCT_UPDATE, id, executor).await?;
    Ok(Some(HandoverProductView {
        product_id: product.base.id,
        maintainer_user_id: product.maintainer_user_id,
        business_org_unit_id: product.business_org_unit_id,
        version: product.base.version,
    }))
}

/// 命名 Process 的执行边界，持有首次请求和安全审计关联。
struct ProductHandoverCommand {
    db: Database,
    rbac: Arc<AuthorizationService>,
    ports: CatalogHandoverPorts,
    id: String,
    req: HandoverProductRequest,
    actor: AuditActor,
    command: CommandReceipt,
    event_id: String,
}

#[async_trait]
impl AuditedCommand for ProductHandoverCommand {
    type Error = Error;
    type Output = HandoverProductView;

    /// 同 Executor 查证、资格校验、交接和回执；重放不追加成功事件。
    ///
    /// # 参数
    /// * `executor` - 查证、交接与回执共用的调用方执行器。
    ///
    /// # 返回
    /// 已有匹配回执时返回 `AuditedWrite::Replayed`，不追加成功事件。首次成功返回 `AuditedWrite::Fresh`，含交接事件和当前商品视图。
    ///
    /// # 错误
    /// 回执不匹配、目标账号或组织校验失败、商品不可更新、交接写入、回执构造或保存失败时返回对应错误。
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        self.rbac
            .require(&self.actor, &catalog::PRODUCT_PRODUCT_UPDATE, AccessTarget::record(&self.id)?, executor)
            .await?;
        if let Some(view) =
            replay(&self.db, &self.ports.access, &self.command, &self.id, &self.actor, executor).await?
        {
            return Ok(AuditedWrite::Replayed(view));
        }
        ensure_target_qualified(&self.db, &self.rbac, self.req.target_user_id.trim(), executor).await?;
        ensure_org_enabled(&self.db, self.req.target_org_unit_id.as_deref(), executor).await?;
        let (before, view) =
            self.ports.service.apply_product_handover(&self.id, &self.req, &self.actor, executor).await?;
        let receipt = ProductHandoverReceipt::new(&self.command, view.clone(), self.event_id.clone())?;
        self.db.product_handover_receipts().create(&receipt, executor).await?;
        Ok(AuditedWrite::Fresh {
            content: handover_content(HandoverEventFacts {
                target_id: view.product_id.clone(),
                target_number: Some(before.product_no),
                responsibility_changed: before.maintainer_user_id != view.maintainer_user_id,
                organization_changed: before.business_org_unit_id != view.business_org_unit_id,
            }),
            result: view,
        })
    }
}

/// 校验目标账号有效且具备商品维护资格。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 当前权限服务。
/// * `target` - 目标账号。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 账号资格满足时成功。
///
/// # 错误
/// 账号缺失、失效或缺少 `product:update` 时返回既有拒绝文案。
async fn ensure_target_qualified(
    db: &Database,
    rbac: &Arc<AuthorizationService>,
    target: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if target_qualified(db, rbac, target, &catalog::PRODUCT_PRODUCT_UPDATE, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("目标账号不存在、已失效或不具备商品维护资格".into()))
}

/// 计算交接命令指纹，供幂等回放比对。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `product_id` - 商品 ID
/// * `req` - 交接请求
/// * `key` - 幂等键
///
/// # 返回
/// 返回命令序列化指纹。
///
/// # 错误
/// 序列化失败时拒绝。
fn handover_fingerprint(
    actor_id: &str,
    product_id: &str,
    req: &HandoverProductRequest,
    key: &str,
) -> Result<String> {
    serde_json::to_string(&(
        actor_id,
        product_id,
        req.target_user_id.trim(),
        req.target_org_unit_id.as_deref().map(str::trim),
        req.reason.trim(),
        req.expected_version,
        key,
    ))
    .map_err(|error| Error::Internal(format!("商品交接命令序列化失败: {error}")))
}

#[cfg(test)]
mod tests {
    use erp_catalog::ports::{EmptyFileAssetFacts, FailClosedAuditPort};
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn handover_authorizes_source_before_receipt_target_account_and_organization_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("product_handover_authorization");
        let authorization = unavailable_authorization();
        let access = CatalogAccess::new(db.clone(), authorization.clone());
        let service = CatalogService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(EmptyFileAssetFacts),
            access.clone(),
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let command = CommandReceipt::from_resource_parts(
            "product-handover-",
            actor.id(),
            HANDOVER_ACTION,
            HANDOVER_RESOURCE,
            "unread-product",
            "key",
            ["payload".to_owned()],
        )
        .unwrap();
        let operation = ProductHandoverCommand {
            db,
            rbac: authorization,
            ports: CatalogHandoverPorts::new(access, service),
            id: "unread-product".into(),
            req: HandoverProductRequest {
                target_user_id: "unread-user".into(),
                target_org_unit_id: Some("unread-org".into()),
                reason: "交接".into(),
                expected_version: 1,
                idempotency_key: "key".into(),
            },
            actor,
            command,
            event_id: "event".into(),
        };
        let result = operation.execute(&mut ReadExecutor::default()).await;
        assert!(
            matches!(result, Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务")
        );
        let result = operation.execute(&mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
    }
}
