//! 组织事实的查询映射；授权与或关系由统一编译器决定。
use std::collections::BTreeSet;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::bson::{Bson, doc};

use super::ORG_MEMBERSHIPS;
use crate::ports::QueryFragment;

/// 构造主属组织存在性片段，不执行 I/O。给定组织集合时，要求 `user` 在统一时点 `at` 有且仅有一条有效关系且归属命中；不指定组织时只拒绝重叠关系，允许没有关系。查询错误由执行方传播。
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
