//! 选品单位的只读解析和确认后的基础资料创建。
use application_core::AuditActor;
use persistence_core::{Executor, Transactional};

use super::CatalogService;
use crate::entity::catalog::UnitOfMeasure;
use crate::repository::CatalogExt;
use crate::{Error, Result};

impl CatalogService {
    /// 只读解析来源单位；缺项返回待创建单位，不在预览时写入。
    /// # 参数
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

    /// 确认选品后创建缺少的单位，单位与审计共用事务。
    /// # 参数
    /// 来源单位、已预览的单位身份与操作人。
    /// # 返回
    /// 已有单位直接复用；新增单位保存到基础资料。
    /// # 错误
    /// 匹配身份变化、并发冲突或写入失败时拒绝，不自动重放未知提交。
    pub async fn ensure_source_unit(&self, name: &str, expected_id: &str, actor: &AuditActor) -> Result<()> {
        let (service, name, expected_id, actor) =
            (self.clone(), name.to_owned(), expected_id.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let unit = service.source_unit(&name, &actor, executor).await?;
                    if unit.base.id != expected_id {
                        return Err(Error::ConflictError("计量单位匹配结果已变化，请重新匹配后入库".into()));
                    }
                    if service.db.unit_of_measures().find_by_id(&unit.base.id, executor).await?.is_none() {
                        service.db.unit_of_measures().create(&unit, executor).await?;
                        let audit = service.audit.resource_log(
                            actor,
                            "unit_of_measure.create",
                            "unit_of_measure",
                            unit.base.id,
                        )?;
                        service.audit.persist(&audit, executor).await?;
                    }
                    Ok(())
                })
            })
            .await
    }
}
