//! 结算对象读取与写入范围；对账负责人与业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_settlement::SupplierSettlementStatement;
use crate::error::{Error, Result};
use crate::ports::{SettlementDataScopePort, SettlementResolvedScope};
use crate::repository::SupplierSettlementExt;
use crate::repository::supplier_settlement::SettlementReadScope;

/// 结算对象访问范围；列表、详情和写命令复用同一解析。
#[derive(Clone)]
pub struct SettlementAccess {
    db: Database,
    scope: Arc<dyn SettlementDataScopePort>,
}

impl SettlementAccess {
    /// 绑定结算集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 结算集合所在数据库
    /// * `scope` - 组合层注入的结算范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, scope: Arc<dyn SettlementDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射结算责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和结算授权条件。
    ///
    /// # 错误
    /// 范围 Port 拒绝动作或解析失败时返回对应错误。
    ///
    /// # 关键业务约束
    /// 责任按对账负责人和业务组织解释；创建人不成为对账负责人。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SettlementResolvedScope, SettlementReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
        Ok((access.clone(), access))
    }

    /// 在调用方事务内重验结算对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
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
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatement> {
        self.scope.require(actor, action, id, executor).await?;
        self.db
            .supplier_settlement_statements()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算单不存在或无权访问".into()))
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
    /// 对账负责人没有主属组织或组织为 `"company"` 时返回 `ValidationError`。范围不覆盖该责任时返回 `Forbidden`。范围 Port 解析失败时返回对应错误。
    ///
    /// # 关键业务约束
    /// 缺主属组织阻断；不得把创建人当作对账负责人兜底。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        owner_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SettlementResolvedScope, String)> {
        let access = self.scope.resolve(actor, "create", executor).await?;
        let owner_org = self
            .scope
            .own_org(owner_user_id, access.as_of, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护对账负责人的有效主属组织".into()))?;
        if owner_org == "company" {
            return Err(Error::ValidationError("请先维护对账负责人的有效主属组织".into()));
        }
        Ok((access, owner_org))
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
    /// 范围 Port 展开失败时返回对应错误。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await
    }
}

/// 返回当前阶段的身份域查询凭证。
/// # 参数
/// access 为已编译凭证，user 为兼容调用参数。
/// # 返回
/// 相同凭证。
/// # 错误
/// 不执行I/O。
pub fn settlement_scope(access: &SettlementResolvedScope, _user: &str) -> SettlementReadScope {
    access.clone()
}
