//! 集成详情根读取：仅消费当前完整对象凭证，再交由用例装配跨域事实。
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::QueryOutput;
use erp_integration::entity::integration_ops::{IntegrationErrorTask, ReconciliationDifference};
use erp_integration::repository::IntegrationOpsExt;
use mongodb::bson::doc;
use mongodb::{Collection, Database};
use persistence_core::{Executor, aggregate};
use serde::de::DeserializeOwned;

use crate::{Error, Result};

/// 按本阶段完整根条件读取指定异常；无权与缺失使用相同错误。
/// # 参数
/// db、executor 为原事务；id 为实际异常身份，access 为当前阶段的完整对象凭证。
/// # 返回
/// 满足全部授权条件的异常记录。
/// # 错误
/// 模型错配、边界失效、无权、缺失或数据库错误时拒绝。
pub(in crate::integration_center) async fn error_task(
    db: &Database,
    id: &str,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<IntegrationErrorTask> {
    authorized(db.integration_error_tasks().collection(), "integration_error_task", id, access, executor)
        .await
}

/// 按本阶段完整根条件读取指定差异；不先读取未经授权的根文档。
/// # 参数
/// db、executor 为原事务；id 为实际差异身份，access 为当前阶段的完整对象凭证。
/// # 返回
/// 满足全部授权条件的差异记录。
/// # 错误
/// 模型错配、边界失效、无权、缺失或数据库错误时拒绝。
pub(in crate::integration_center) async fn difference(
    db: &Database,
    id: &str,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<ReconciliationDifference> {
    authorized(
        db.reconciliation_differences().collection(),
        "reconciliation_difference",
        id,
        access,
        executor,
    )
    .await
}

/// 业务主键只收窄固定授权计划，不替换字段、输出或依赖。
async fn authorized<T: DeserializeOwned + Send + Sync>(
    collection: Collection<T>,
    resource: &str,
    id: &str,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<T> {
    access.ensure_current(executor)?;
    let model = access.model();
    if model.resource != resource
        || model.collection != collection.name()
        || model.key.as_str() != collection.name()
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(Error::ValidationError("集成详情授权模型与仓储不匹配".into()));
    }
    let mut stages = vec![doc! {"$match": {"id": id}}];
    stages.extend(access.stages().iter().cloned());
    stages.push(doc! {"$limit": 1});
    let mut rows = aggregate::rows(&collection, stages, executor).await?;
    access.ensure_current(executor)?;
    rows.pop().ok_or_else(|| Error::NotFound("集成对象不存在或无权查看".into()))
}
