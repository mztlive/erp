//! 客户事实映射；角色、范围、读取补充由 identity 统一编译。
mod context;
mod facts;
use std::sync::Arc;

pub use context::CustomerResolvedScope;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessMapping, QueryFragment, RelationMapping};
use erp_identity::repository::organization::primary_membership_query;
use erp_identity::{Error as IdentityError, Result as IdentityResult};
pub use facts::CustomerFacts;
use mongodb::Database;
use mongodb::bson::{Bson, doc};

use crate::repository::CustomerExt;
use crate::repository::customer_shared::active_window_filter;
use crate::service::customer::access::business_date;

/// 客户完整对象映射；多个有效负责人分别提供存在性事实。
pub struct CustomerMapping {
    model: QueryModel,
}

impl CustomerMapping {
    /// 构造固定客户模型。
    /// # 参数
    /// _db 为组合根统一构造接口传入的数据库句柄，映射不保存该句柄。
    /// # 返回
    /// 客户主键粒度的共享映射。
    /// # 错误
    /// 固定模型非法时启动失败；不执行 I/O。
    pub fn shared(_db: Database) -> Arc<dyn AccessMapping> {
        Arc::new(Self::new())
    }

    fn new() -> Self {
        Self {
            model: QueryModel {
                key: QueryModelKey::parse("customer_accounts").expect("固定模型"),
                resource: "customer".into(),
                collection: Database::CUSTOMER_ACCOUNTS.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [
                    FactKind::Owner,
                    FactKind::Collaborators,
                    FactKind::Organization,
                    FactKind::HistoryParticipants,
                ]
                .into(),
            },
        }
    }
}

impl AccessMapping for CustomerMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }

    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }

    fn valid_until(&self, context: &AccessContext) -> Option<Instant> {
        Some(next_day(context.as_of))
    }

    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": NOT_DELETED_TIMESTAMP_BSON}))
    }

    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<QueryFragment> {
        if fact == &AccessFact::Collaborating {
            return Ok(QueryFragment::Match(doc! {"$expr": false}));
        }

        let date = business_date(context.as_of)
            .map_err(|error| IdentityError::ValidationError(error.to_string()))?;
        let mut filter = doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "assignment_role": "OWNER",
            "$expr": {"$eq": ["$customer_id", "$$customer"]},
        };
        if (context.permission.resource() != "customer" || context.permission.action() != "create")
            && !matches!(fact, AccessFact::HistoryParticipant)
        {
            filter = active_window_filter(&date, Some(filter));
        }

        let mut pipeline = Vec::new();
        match fact {
            AccessFact::Owned | AccessFact::HistoryParticipant => {
                filter.insert("user_id", context.actor.id());
            },
            AccessFact::OrganizationIn(orgs) => {
                let QueryFragment::Exists { collection, bindings, pipeline: membership } =
                    primary_membership_query("$user_id".into(), Some(orgs), context.as_of)
                else {
                    unreachable!("组织事实固定关联")
                };
                pipeline.push(doc! {"$lookup": {"from": collection, "let": bindings, "pipeline": membership, "as": "__customer_org"}});
                pipeline.push(doc! {"$match": {"__customer_org.0": {"$exists": true}}});
            },
            _ => return Err(IdentityError::ValidationError("客户不支持该授权事实".into())),
        }
        pipeline.insert(0, doc! {"$match": filter});
        Ok(QueryFragment::Exists {
            collection: Database::CUSTOMER_ASSIGNMENTS.into(),
            bindings: doc! {"customer": "$id"},
            pipeline,
        })
    }

    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("客户没有来源关系".into()))
    }
}

/// 归属按上海自然日解释，授权在下一日边界失效。
fn next_day(at: Instant) -> Instant {
    let midnight = ((at.unix_secs() + 8 * 3600) / 86400 + 1) * 86400 - 8 * 3600;
    Instant::from_unix_secs(midnight)
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_permissions::customer;

    use super::*;

    fn context(action: &str, at: i64) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin),
            permission: match action {
                "create" => customer::CREATE,
                "update" => customer::UPDATE,
                _ => customer::DETAIL,
            },
            resource: "customer".into(),
            model: None,
            as_of: Instant::from_unix_secs(at),
        }
    }

    #[test]
    fn static_mapping_expires_at_shanghai_midnight() {
        let mapping = CustomerMapping::new();
        for (at, midnight) in [(0, 57600), (57599, 57600), (57600, 144000)] {
            assert_eq!(mapping.valid_until(&context("detail", at)), Some(Instant::from_unix_secs(midnight)));
            assert!(mapping.preparation().is_none());
        }
    }

    #[test]
    fn current_owner_uses_the_active_window_and_create_keeps_the_proposed_assignment() {
        let mapping = CustomerMapping::new();
        for (action, fact, active) in [
            ("detail", AccessFact::Owned, true),
            ("update", AccessFact::Owned, true),
            ("create", AccessFact::Owned, false),
            ("detail", AccessFact::HistoryParticipant, false),
        ] {
            let QueryFragment::Exists { bindings, pipeline, .. } =
                mapping.predicate(&fact, &context(action, 0), None).unwrap()
            else {
                panic!("客户责任事实必须来自实际归属");
            };
            assert_eq!(bindings, doc! {"customer": "$id"});
            let filter = pipeline[0].get_document("$match").unwrap();
            assert_eq!(filter.get_str("user_id").unwrap(), "sales");
            assert_eq!(filter.get_str("assignment_role").unwrap(), "OWNER");
            assert_eq!(filter.contains_key("valid_from"), active);
            assert_eq!(filter.contains_key("$or"), active);
        }
    }

    #[test]
    fn history_supplements_are_read_only_and_unsupported_facts_fail_closed() {
        let mapping = CustomerMapping::new();
        assert!(mapping.predicate(&AccessFact::Participant, &context("detail", 0), None).is_err());
        assert!(
            mapping.relation(&RelationKey::parse("orders").unwrap(), &context("detail", 0), None).is_err()
        );
    }
}
