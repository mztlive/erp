//! 供给对象读取与写入范围；显式维护人和业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, supplier_offering};
use mongodb::Database;
use persistence_core::Executor;

use crate::authorization::{Authorization, CompiledScope};
use crate::entity::supplier_offering::SupplierOffering;
use crate::error::{Error, Result};
use crate::repository::SupplierOfferingExt;

/// 供给对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct OfferingAccess {
    db: Database,
    authorization: Option<Authorization>,
}

impl OfferingAccess {
    /// 绑定供给集合与共享身份设施。
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    /// 账号和功能资格由 identity 执行；业务组织只作为写入事实。
    pub fn new(db: Database, service: Arc<AuthorizationService>) -> Self {
        let authorization = Authorization::new(service, "supplier_offering").expect("固定资源授权目录有效");
        Self { db, authorization: Some(authorization) }
    }

    /// 创建尚未接线的访问器，授权调用保持失败关闭。
    /// 未绑定身份设施的访问器。
    /// 构造不执行 I/O。
    pub fn unwired(db: Database) -> Self {
        Self { db, authorization: None }
    }

    // 提供已注入的身份设施，禁止默认扩大范围。
    fn authorization(&self) -> Result<&Authorization> {
        self.authorization.as_ref().ok_or_else(|| Error::Internal("供应链授权未接线".into()))
    }

    /// 供列表或导出调用：在当前执行器内请身份域按固定供给动作编译数据范围。负责人与组织取显式事实，不以
    /// 创建人替代维护人；动作无权或授权未装配时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        Ok(self.authorization()?.resolve(actor, permission, executor).await?)
    }

    /// 在详情、条款历史或写命令读取前，用原事务和指定动作授权真实供给，再读取实体。缺记录返回 `NotFound`；
    /// 身份域的拒绝不绕过，列表凭证不代替对象授权。
    pub async fn require_offering(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOffering> {
        self.authorization()?.require(actor, permission, id, executor).await?;
        self.db
            .supplier_offerings()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))
    }

    // 写入交接前，用原事务同时检查旧供给与拟保存的维护人、业务组织；任一归属、功能权限或读取边界不满足均
    // 拒绝。
    pub(crate) async fn require_owner_change(
        &self,
        actor: &AuditActor,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization()?
            .service
            .require(
                actor,
                &supplier_offering::REVISE,
                AccessTarget::OwnershipChange {
                    id: offering.base.id.clone(),
                    candidate: ObjectFacts {
                        resource: "supplier_offering".into(),
                        id: offering.base.id.clone(),
                        owner_id: FactValue::Known(Some(offering.maintainer_user_id.clone())),
                        org_unit_id: FactValue::Known(Some(offering.business_org_unit_id.clone())),
                        ..Default::default()
                    },
                },
                executor,
            )
            .await?)
    }

    /// 查询账号在解析时点的唯一主属组织。
    /// 存在唯一主属组织时返回其 ID。
    /// 同一时点多条主属关系时拒绝。
    /// `at`：授权时点。
    pub async fn own_org(
        &self,
        user_id: &str,
        at: erp_core::common::time::Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone()).own_org(user_id, at, executor).await?)
    }

    /// 解析拟写入的维护人及其主属组织，缺组织阻断。
    /// 返回维护人 ID 与其主属组织。
    /// 维护人为空或缺少有效主属组织时拒绝。
    /// 不得用创建人字段回填；组织取其主属组织，不默认公司。
    /// `requested`：空则使用操作人。
    pub async fn maintainer_org(
        &self,
        requested: Option<&str>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        let maintainer = requested.map(str::trim).filter(|value| !value.is_empty()).unwrap_or(actor.id());
        if maintainer.is_empty() {
            return Err(Error::ValidationError("供给维护人不能为空".into()));
        }
        let as_of = erp_core::common::time::Instant::now();
        let org = self.own_org(maintainer, as_of, executor).await?.ok_or_else(|| {
            Error::BusinessLogicError("维护人缺少有效主属组织，请先维护组织成员关系".into())
        })?;
        Ok((maintainer.to_string(), org))
    }

    /// 检查由服务端构造的供给创建候选。
    /// 创建功能资格与目标归属全部通过时成功。
    /// 资格、范围或候选不符合时拒绝。
    /// actor、offering 为真实账号和已验证候选，executor 为创建事务。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization()?
            .service
            .require(
                actor,
                &supplier_offering::CREATE,
                AccessTarget::Candidate(ObjectFacts {
                    resource: "supplier_offering".into(),
                    id: offering.base.id.clone(),
                    owner_id: FactValue::Known(Some(offering.maintainer_user_id.clone())),
                    org_unit_id: FactValue::Known(Some(offering.business_org_unit_id.clone())),
                    ..Default::default()
                }),
                executor,
            )
            .await?)
    }

    /// 检查共享资料当前动作的功能资格；对象操作另行检查完整目标。
    /// 当前有效账号具备动作资格时成功。
    /// 未接线、未登记动作或账号无操作资格时拒绝。
    /// `permission`：内置写权限。
    pub async fn ensure_writable(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization()?.ensure_permission(permission)?;
        if !self.authorization()?.service.has_permission(actor, permission, executor).await? {
            return Err(Error::Forbidden("没有该操作权限".into()));
        }
        Ok(())
    }
}
