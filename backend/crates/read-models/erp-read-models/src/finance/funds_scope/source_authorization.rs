//! 资金查询一次解析并共享固定来源计划及交付凭证。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::Permission as AccessPermission;
use persistence_core::Executor;

use super::authorization::FundsReadContext;
use super::repository::source_access::SourceQuery;
use super::repository::whole_access::WholeAccess;
use super::{FundsAccess, FundsAuthorization};
use crate::Result;
impl FundsAccess {
    // 确定财务来源范围的适用条件时，用同一次解析取得固定根及全部来源计划。
    //
    // 调用参数：
    // actor 为账号；resource/action 为固定业务入口，purchase/dual 只决定业务读取方向。
    //
    // 共享单一凭证的查询元信息和各来源只读计划。
    //
    // 失败边界：
    // 入口、绑定或授权失败原样返回。
    pub(super) async fn resolve_sources(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        _purchase: bool,
        _dual: bool,
        executor: &mut dyn Executor,
    ) -> Result<(FundsReadContext, FundsAuthorization)> {
        let ledger_read = self
            .rbac
            .has_permission(actor, &erp_permissions::policy_permission::FINANCE_LEDGER_READ, executor)
            .await?;

        let access = Arc::new(self.rbac.query(actor, permission, executor).await?);
        let sales = SourceQuery::new(access.clone(), "sales_source")?;
        let purchase_scope = SourceQuery::new(access.clone(), "purchase_source")?;
        let settlement = SourceQuery::new(access.clone(), "settlement_source")?;
        let whole = WholeAccess::new(access.clone());

        let context = FundsReadContext {
            resource: permission.resource().into(),
            scope_version: access.version(),
            as_of: Instant::now(),
        };
        Ok((
            context.clone(),
            FundsAuthorization {
                access: Some(access),
                service: Some(self.rbac.clone()),
                whole,
                sales,
                ledger_read,
                settlement,
                purchase_scope,
                context,
            },
        ))
    }
}

impl FundsAuthorization {
    // 校验财务来源范围时，仓储只核验边界和期限；交付时由读取用例统一复验根凭证。
    //
    // 调用参数：
    // executor 为原读取执行器。
    //
    // 所有来源仍属于同一阶段时成功。
    //
    // 失败边界：
    // 跨阶段、事务失效或到期拒绝。
    pub(super) async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        if let Some(access) = &self.access {
            access.ensure_current(executor)?;
        }
        Ok(())
    }

    // 处理财务来源范围时，完成所有仓储读取后执行一次结果交付检查。
    //
    // 调用参数：
    // executor 为本次原执行器。
    //
    // 当前模式的边界、期限和必要依赖都有效时成功。
    //
    // 失败边界：
    // 任一授权依赖变化或阶段失效即丢弃结果。
    pub(crate) async fn deliver(&self, executor: &mut dyn Executor) -> Result<()> {
        if let (Some(access), Some(service)) = (&self.access, &self.service) {
            service.validate(access, executor).await?;
        }
        Ok(())
    }
}
