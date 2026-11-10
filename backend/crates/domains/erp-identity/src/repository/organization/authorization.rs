//! 组织事实的查询映射；授权与或关系由统一编译器决定。
use std::collections::BTreeSet;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::bson::{Bson, doc};

use super::ORG_MEMBERSHIPS;
use crate::ports::QueryFragment;

/// 以当前唯一主属关系判断指定账号是否属于组织集合。
/// # 参数
/// user 为服务端提供的账号表达式，orgs 为授权组织集合；None 只检查主属关系不重叠，at 为统一授权时点。
/// # 返回
/// 身份域拥有的关联存在性片段；组织匹配要求唯一关系；仅校验时允许没有关系，拒绝重叠关系。
/// # 错误
/// 不执行 I/O；数据库错误由执行查询的仓储传播。
pub fn primary_membership_query(user: Bson, orgs: Option<&BTreeSet<String>>, at: Instant) -> QueryFragment {
    let mut pipeline = vec![
        doc! { "$match": {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": { "$eq": ["$user_id", "$$member"] },
            "valid_from": { "$lte": at.unix_secs() },
            "$or": [{ "valid_to": null }, { "valid_to": { "$gt": at.unix_secs() } }],
        } },
        doc! { "$limit": 2 },
    ];
    match orgs {
        Some(orgs) => pipeline.extend([
            doc! { "$group": { "_id": null, "count": { "$sum": 1 }, "org": { "$first": "$org_unit_id" } } },
            doc! { "$match": { "count": 1, "org": { "$in": orgs.iter().cloned().collect::<Vec<_>>() } } },
        ]),
        None => pipeline.extend([
            doc! { "$facet": { "members": [{ "$limit": 2 }] } },
            doc! { "$match": { "$expr": { "$lte": [{ "$size": "$members" }, 1] } } },
        ]),
    }
    QueryFragment::Exists { collection: ORG_MEMBERSHIPS.into(), bindings: doc! { "member": user }, pipeline }
}
