//! 供应商对象读取与写入范围；维护人与业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, supplier};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::authorization::CompiledScope;
use crate::entity::supplier::SupplierAccount;
use crate::error::{Error, Result};

/// 供应商对象访问范围；列表、详情、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct SupplierAccess {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl SupplierAccess {
    /// 绑定供应商事实与共享身份设施。
    ///
    /// # 参数
    /// * `db` - 供应商集合所在数据库
    /// * `authorization` - 组合层注入的共享授权服务
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 在调用方事务内证明资源动作并映射供应商责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置供应商权限
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和供应商授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden。
    ///
    /// # 关键业务约束
    /// 组织范围按单据业务组织解释；能力负责人不扩大可见集合。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        ensure_permission(permission)?;
        let query = self.authorization.query(actor, permission, executor).await?;
        let compiled = Arc::new(query);
        Ok(CompiledScope {
            scope_version: compiled.version(),
            compiled,
            authorization: self.authorization.clone(),
        })
    }

    /// 在独立事务中重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置供应商权限
    /// * `supplier_id` - 目标供应商
    ///
    /// # 返回
    /// 完整对象授权通过时成功。
    ///
    /// # 错误
    /// 读取动作对不可见对象返回 NotFound；写动作返回 Forbidden。
    pub async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        supplier_id: &str,
    ) -> Result<()> {
        let this = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let supplier_id = supplier_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_with(&actor, &permission, &supplier_id, executor).await })
            })
            .await
    }

    /// 在调用方事务内重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置权限
    /// * `supplier_id` - 目标供应商
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 完整对象授权通过时成功。
    ///
    /// # 错误
    /// 账号、动作或对象资格失效时拒绝。
    pub async fn require_with(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_permission(permission)?;
        Ok(self
            .authorization
            .require(actor, permission, AccessTarget::record(supplier_id)?, executor)
            .await?)
    }

    /// 同时检查持久化供应商与已确认的目标维护人、业务组织。
    /// # 参数
    /// actor 为当前操作人，candidate 为已通过领域交接校验的拟保存账户，executor 为原写事务。
    /// # 返回
    /// 原记录与目标归属均满足供应商维护范围时成功。
    /// # 错误
    /// 账号失效、原对象或目标归属无权、事实读取失败时拒绝。
    pub async fn require_owner_change(
        &self,
        actor: &AuditActor,
        candidate: &SupplierAccount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization
            .require(
                actor,
                &supplier::HANDOVER,
                AccessTarget::OwnershipChange {
                    id: candidate.base.id.clone(),
                    candidate: ObjectFacts {
                        resource: "supplier".into(),
                        owner_id: FactValue::Known(Some(candidate.maintainer_user_id.clone())),
                        org_unit_id: FactValue::Known(Some(candidate.business_org_unit_id.clone())),
                        ..Default::default()
                    },
                },
                executor,
            )
            .await?)
    }

    /// 在调用方事务内证明创建资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `maintainer_user_id` - 拟写入的整体维护人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建资格通过且负责人具备有效组织时返回组织 ID。
    ///
    /// # 错误
    /// 无创建动作、范围为空或维护人没有有效主属组织时拒绝。
    ///
    /// # 关键业务约束
    /// 缺主属组织阻断；不得把创建人当作维护人兜底。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        maintainer_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let owner_org = OrganizationDirectoryService::new(self.db.clone())
            .own_org(maintainer_user_id, Instant::now(), executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?;
        self.authorization
            .require(
                actor,
                &supplier::IMPORT_SUPPLIER_IMPORT,
                AccessTarget::Candidate(ObjectFacts {
                    resource: "supplier".into(),
                    owner_id: FactValue::Known(Some(maintainer_user_id.into())),
                    org_unit_id: FactValue::Known(Some(owner_org.clone())),
                    ..Default::default()
                }),
                executor,
            )
            .await?;
        Ok(owner_org)
    }
}

/// 内置权限直接接入时仍校验拥有资源，禁止跨域凭证替换。
fn ensure_permission(permission: &Permission) -> Result<()> {
    if permission.resource() != "supplier" {
        return Err(Error::ValidationError("权限资源与对象访问器不符".into()));
    }
    Ok(())
}

#[cfg(test)]
mod permission_tests {
    use erp_permissions::sales_order;

    use super::*;

    #[test]
    fn accessor_accepts_its_constant_and_rejects_foreign_resource() {
        assert!(ensure_permission(&supplier::DETAIL).is_ok());
        assert!(matches!(ensure_permission(&sales_order::DETAIL), Err(Error::ValidationError(_))));
    }
}
