//! 来源授权记录按页同步，保留逐行乐观锁并批量新增。
use entity_core::BaseModel;
use mongodb::Database;
use persistence_core::{Executor, mongo_ops};

use super::{GRANTS, grants, row_id, source_rows};
use crate::Result;
use crate::entity::resource_access::rows::GrantRows;
use crate::entity::resource_access::{GrantSource, ResourceGrant};

/// 在来源发布事务内分页刷新已保存来源的历史授权行，保留逐行乐观锁并分批新增缺失目标；查询、版本冲突、唯一键冲突或写入失败时直接返回错误。
pub(crate) async fn sync_rows(
    db: &Database,
    source: &GrantSource,
    executor: &mut dyn Executor,
) -> Result<()> {
    let repo = grants(db);
    let mut delta = GrantRows::new(source);
    let mut after = None;
    loop {
        let mut rows = source_rows(db, &source.base.id, after.as_deref(), executor).await?;
        let Some(last) = rows.last() else { break };
        after = Some(last.base.id.clone());
        delta.refresh(&mut rows);
        for row in &mut rows {
            repo.update(row, executor).await?;
        }
    }

    let rows = delta
        .missing()
        .map(|target| {
            Ok(ResourceGrant {
                base: BaseModel::new(row_id(&source.base.id, target.resource, &target.id)?),
                recipient: source.key.recipient.clone(),
                source_id: source.base.id.clone(),
                source_version: source.base.version,
                resource: target.resource,
                resource_id: target.id.clone(),
                action: "read".into(),
                active: true,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    for page in rows.chunks(200) {
        mongo_ops::insert_many(&db.collection::<ResourceGrant>(GRANTS), page, executor).await?;
    }
    Ok(())
}
