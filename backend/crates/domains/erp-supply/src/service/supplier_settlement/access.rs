//! 结算对象读取与写入范围；对账负责人与业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, supplier_settlement};
use mongodb::Database;
use persistence_core::Executor;

use super::authorization::{SettlementAction, SettlementActionAccess};
use crate::authorization::{Authorization, CompiledScope};
use crate::entity::supplier_settlement::SupplierSettlementStatement;
use crate::error::{Error, Result};
use crate::repository::SupplierSettlementExt;

/// 结算对象访问范围；列表、详情和写命令复用同一解析。
#[derive(Clone)]
pub struct SettlementAccess {
    db: Database,
    authorization: Option<Authorization>,
}

impl SettlementAccess {
    /// 绑定结算集合与共享身份设施。
    ///
    /// # 参数
    /// * `db` - 结算集合所在数据库
    /// * `rbac` - 共享身份设施
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, service: Arc<AuthorizationService>) -> Self {
        let authorization = Authorization::new(service.clone(), "supplier_settlement_statement")
            .expect("固定资源授权目录有效");
        Self { db, authorization: Some(authorization) }
    }

    /// 创建尚未接线的访问器，授权调用保持失败关闭。
    /// # 参数
    /// db 为业务集合所在数据库。
    /// # 返回
    /// 未绑定身份设施的访问器。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn unwired(db: Database) -> Self {
        Self { db, authorization: None }
    }

    /// 取得已注入的身份设施，禁止默认扩大范围。
    fn authorization(&self) -> Result<&Authorization> {
        self.authorization.as_ref().ok_or_else(|| Error::Internal("供应链授权未接线".into()))
    }

    pub(crate) async fn require_operation(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization()?
            .service
            .require(actor, permission, AccessTarget::Operation, executor)
            .await?)
    }

    /// 在调用方事务内证明资源动作并映射结算责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置权限
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回本次查询的单份授权凭证。
    ///
    /// # 错误
    /// 授权设施 拒绝动作或解析失败时返回对应错误。
    ///
    /// # 关键业务约束
    /// 责任按对账负责人和业务组织解释；创建人不成为对账负责人。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        Ok(self.authorization()?.resolve(actor, permission, executor).await?)
    }

    /// 在调用方事务内重验结算对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置权限
    /// * `id` - 结算单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回结算单。
    ///
    /// # 错误
    /// 读取动作对不可见对象返回 NotFound；写动作返回 Forbidden。
    pub async fn require_statement(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatement> {
        self.authorization()?.require(actor, permission, id, executor).await?;
        self.db
            .supplier_settlement_statements()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算单不存在或无权访问".into()))
    }

    /// 在交接事务内同时检查原结算单资格与拟保存的目标归属。
    /// # 参数
    /// actor 为发起交接的账号，statement 为服务端完成归属变更后的实体，executor 为原写事务。
    /// # 返回
    /// 原对象和目标负责人、业务组织均在当前操作范围内时成功。
    /// # 错误
    /// 账号、原对象或目标归属无权，以及必要事实缺失时拒绝。
    pub async fn require_owner_change(
        &self,
        actor: &AuditActor,
        statement: &SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization()?
            .service
            .require(
                actor,
                &supplier_settlement::STATEMENT_HANDOVER,
                AccessTarget::OwnershipChange {
                    id: statement.base.id.clone(),
                    candidate: ObjectFacts {
                        resource: "supplier_settlement_statement".into(),
                        id: statement.base.id.clone(),
                        owner_id: FactValue::Known(Some(statement.prepared_by.clone())),
                        org_unit_id: FactValue::Known(Some(statement.business_org_unit_id.clone())),
                        ..Default::default()
                    },
                },
                executor,
            )
            .await?)
    }

    /// 在原执行器核验具体结算命令的功能权限及实际责任人。
    /// # 参数
    /// actor 为真实账号，action 为固定命令用途，id 为实际结算单，executor 为原执行器。
    /// # 返回
    /// identity 证明既有入口范围与制单人或差异处理人资格时成功。
    /// # 错误
    /// 无权限、责任人不符、对象缺失或设施读取失败时拒绝。
    pub async fn require_action(
        &self,
        actor: &AuditActor,
        action: SettlementAction,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization()?
            .service
            .require(actor, &action.permission(), AccessTarget::record(id)?, executor)
            .await?)
    }

    /// 为详情动作读取同一对象的完整 identity 资格。
    /// # 参数
    /// actor、id 为详情账号与实际结算单，executor 为读取执行器。
    /// # 返回
    /// 各具体命令的对象授权结果，不包含业务状态限制。
    /// # 错误
    /// 设施错误透传；功能或对象资格拒绝转换为对应动作不可用。
    pub async fn actions(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SettlementActionAccess> {
        let mut result = SettlementActionAccess::default();
        for (action, allowed) in [
            (SettlementAction::AppendEvidence, &mut result.append_evidence),
            (SettlementAction::Refresh, &mut result.refresh),
            (SettlementAction::Void, &mut result.void),
            (SettlementAction::ResolveDifference, &mut result.resolve_difference),
            (SettlementAction::Submit, &mut result.submit),
        ] {
            match self.require_action(actor, action, id, executor).await {
                Ok(()) => *allowed = true,
                Err(Error::Forbidden(_) | Error::NotFound(_)) => {},
                Err(error) => return Err(error),
            }
        }
        Ok(result)
    }

    /// 在调用方事务内证明创建资格并解析对账负责人主属组织。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `owner_user_id` - 拟写入的对账负责人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建范围覆盖拟写入责任时返回授权上下文与业务组织。
    ///
    /// # 错误
    /// 对账负责人没有主属组织或组织为 `"company"` 时返回 `ValidationError`。范围不覆盖该责任时返回 `Forbidden`。授权设施 解析失败时返回对应错误。
    ///
    /// # 关键业务约束
    /// 缺主属组织阻断；不得把创建人当作对账负责人兜底。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        owner_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let owner_org = OrganizationDirectoryService::new(self.db.clone())
            .own_org(owner_user_id, erp_core::common::time::Instant::now(), executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护对账负责人的有效主属组织".into()))?;
        if owner_org == "company" {
            return Err(Error::ValidationError("请先维护对账负责人的有效主属组织".into()));
        }

        self.authorization()?
            .service
            .require(
                actor,
                &supplier_settlement::STATEMENT_CREATE,
                AccessTarget::Candidate(ObjectFacts {
                    resource: "supplier_settlement_statement".into(),
                    id: "new_statement".into(),
                    owner_id: FactValue::Known(Some(owner_user_id.into())),
                    org_unit_id: FactValue::Known(Some(owner_org.clone())),
                    ..Default::default()
                }),
                executor,
            )
            .await?;
        Ok(owner_org)
    }

    /// 展开请求组织及其可选下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织身份。
    /// * `include_descendants` - 为真时包含下级组织。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回展开后的组织身份集合。
    ///
    /// # 错误
    /// 授权设施 展开失败时返回对应错误。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone())
            .expand_org_units(org_unit_ids, include_descendants, executor)
            .await?)
    }
}
