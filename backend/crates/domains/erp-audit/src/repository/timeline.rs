//! 已授权业务对象的展示查询；不提供业务恢复或身份判断。
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::AuditLog;

/// 精确资源身份；类型与 ID 必须成对匹配。
#[derive(Debug, Clone)]
pub struct TimelineTarget {
    pub resource_type: String,
    pub resource_id: String,
}

/// 按业务对象读取成功事件，供跨域时间线归集。
#[allow(async_fn_in_trait)]
pub trait AuditTimelineRepositoryExt {
    /// 读取已授权目标的下一批历史记录。
    ///
    /// # 参数
    /// `targets` 是已授权对象；`before` 是时间与原始事件 ID；`limit` 为批次大小。
    /// `executor` 沿用调用方的读取事务。
    /// # 返回
    /// 按记录时间、ID 倒序返回，不读取失败尝试或未结构化的旧消息。
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn timeline_events(
        &self,
        targets: &[TimelineTarget],
        before: Option<(u64, &str)>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AuditLog>>;
}

impl AuditTimelineRepositoryExt for Repository<'_, AuditLog> {
    async fn timeline_events(
        &self,
        targets: &[TimelineTarget],
        before: Option<(u64, &str)>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AuditLog>> {
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let options =
            FindOptions::builder().sort(doc! { "created_at": -1, "id": -1 }).limit(i64::from(limit)).build();
        mongo_ops::find_many(&self.collection(), filter(targets, before), options, executor).await
    }
}

fn filter(targets: &[TimelineTarget], before: Option<(u64, &str)>) -> Document {
    let identities = targets
        .iter()
        .map(|target| doc! { "resource_type": &target.resource_type, "resource_id": &target.resource_id })
        .collect::<Vec<_>>();
    let mut filter = doc! {
        "deleted_at": 0_i64,
        "success": true,
        "structured_event.result": "succeeded",
        "$or": identities,
    };
    if let Some((time, id)) = before {
        let time = i64::try_from(time).unwrap_or(i64::MAX);
        filter.insert(
            "$and",
            vec![doc! { "$or": [
                { "created_at": { "$lt": time } },
                { "created_at": time, "id": { "$lt": id } }
            ] }],
        );
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_identity_and_cursor_are_conjoined() {
        let targets = vec![
            TimelineTarget { resource_type: "sales_order".into(), resource_id: "a".into() },
            TimelineTarget { resource_type: "invoice".into(), resource_id: "b".into() },
        ];
        let filter = filter(&targets, Some((10, "event")));
        assert_eq!(filter.get_str("structured_event.result").unwrap(), "succeeded");
        assert_eq!(filter.get_array("$or").unwrap().len(), 2);
        assert_eq!(filter.get_array("$and").unwrap().len(), 1);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
    }
}
