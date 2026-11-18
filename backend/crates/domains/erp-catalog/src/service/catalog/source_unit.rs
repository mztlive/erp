//! 选品单位的只读解析和确认后的基础资料创建。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::internal;
use persistence_core::Executor;

use super::CatalogService;
use crate::entity::catalog::UnitOfMeasure;
use crate::repository::CatalogExt;
use crate::{Error, Result};

impl CatalogService {
    /// 只读解析来源单位；缺项返回待创建单位，不在预览时写入。
    /// # 参数
    /// * `executor` - 原授权事务或读取阶段的执行器。
    ///
    /// 来源单位、已认证操作人与调用方执行器。
    /// # 返回
    /// 已有启用单位或具有稳定身份的新单位。
    /// # 错误
    /// 来源非法、候选冲突或数据库读取失败时返回错误。
    pub async fn source_unit(
        &self,
        name: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<UnitOfMeasure> {
        let candidates = self.db.catalog().source_unit_candidates(name, executor).await?;
        UnitOfMeasure::from_source(name, candidates, actor.id())
    }

    /// 在选品原事务核验完整资格，再创建缺少的单位并保存审计。
    /// # 参数
    /// 来源单位、已预览的单位身份、操作人与选品原事务执行器。
    /// # 返回
    /// 已有单位直接复用；新增单位保存到基础资料。
    /// # 错误
    /// 匹配身份变化、并发冲突或写入失败时拒绝，不自动重放未知提交。
    pub async fn ensure_source_unit(
        &self,
        name: &str,
        expected_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.data_scope
            .authorization
            .require(actor, &internal::CATALOG_ADOPTION, AccessTarget::Operation, executor)
            .await?;

        let unit = self.source_unit(name, actor, executor).await?;
        if unit.base.id != expected_id {
            return Err(Error::ConflictError("计量单位匹配结果已变化，请重新匹配后入库".into()));
        }

        if self.db.unit_of_measures().find_by_id(&unit.base.id, executor).await?.is_none() {
            self.db.unit_of_measures().create(&unit, executor).await?;
            let audit = self.audit.resource_log(
                actor.clone(),
                "unit_of_measure.create",
                "unit_of_measure",
                unit.base.id,
            )?;
            self.audit.persist(&audit, executor).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::testing::{actor, denied, service};
    use crate::Error;

    #[tokio::test]
    async fn source_unit_authorizes_before_dictionary_reads_and_rejects_non_snapshot_writes() {
        let service = service().await;
        denied(service.ensure_source_unit("件", "unit", &actor(), &mut SnapshotExecutor::default()).await);
        assert!(matches!(
            service.ensure_source_unit("件", "unit", &actor(), &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
        ));
    }
}
