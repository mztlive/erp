//! 客户事实映射；角色、范围、读取补充由 identity 统一编译。
mod context;
use std::sync::Arc;

use async_trait::async_trait;
pub use context::CustomerResolvedScope;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, AccessSubject, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput,
    ReadAccess, RelationKey,
};
use erp_identity::ports::{AccessMapping, QueryFragment, RelationMapping};
use erp_identity::repository::organization::primary_membership_query;
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AuthorizationRegistry, AuthorizationService, DataAccess,
    Error as IdentityError, Permission, Result as IdentityResult, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

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
                facts: [FactKind::Owner, FactKind::Organization, FactKind::HistoryParticipants].into(),
            },
        }
    }
}

/// 登记客户固定动作；列表与详情按接线参数开启历史参与。
/// # 参数
/// db、rbac 为身份设施；mapping 为登记及编译使用的同一实例；history 控制读取历史补充。
/// # 返回
/// 固定目录的统一授权服务。
/// # 错误
/// 固定登记非法时启动失败；不执行 I/O。
pub fn service(
    db: Database,
    rbac: SharedRbacService,
    mapping: Arc<dyn AccessMapping>,
    history: bool,
) -> AuthorizationService {
    let entries = ["list", "detail", "create", "update", "delete"]
        .into_iter()
        .map(|action| registration(action, mapping.clone(), history))
        .collect();
    let registry = AuthorizationRegistry::new(entries).expect("客户固定目录完整");
    AuthorizationService::new(db, rbac, registry)
}

fn registration(action: &str, mapping: Arc<dyn AccessMapping>, history: bool) -> AccessRegistration {
    let permission = Permission::parse(format!("customer:{action}")).expect("固定权限");
    AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Query {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                policy: AccessPolicy::Scope {
                    permission,
                    read: ReadAccess {
                        history: history && matches!(action, "list" | "detail"),
                        records: false,
                    },
                },
            },
            mapping,
        },
    }
}

#[async_trait]
impl AccessMapping for CustomerMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }

    fn key(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<Bson> {
        Ok("$id".into())
    }

    async fn prepare(&self, context: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
        // 归属读取仅使用原事务快照；跨页完整成员版本由客户仓储指纹校验。
        let now = context.as_of.unix_secs();
        let midnight = ((now + 8 * 3600) / 86400 + 1) * 86400 - 8 * 3600;
        Ok(MappingFacts { version: None, valid_until: Some(Instant::from_unix_secs(midnight)) })
    }

    fn records(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": NOT_DELETED_TIMESTAMP_BSON}))
    }

    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        let date = business_date(context.as_of)
            .map_err(|error| IdentityError::ValidationError(error.to_string()))?;
        let mut filter = doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "assignment_role": "OWNER",
            "$expr": {"$eq": ["$customer_id", "$$customer"]},
        };
        if context.permission.action() != "create" && !matches!(fact, AccessFact::HistoryParticipant) {
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
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("客户没有来源关系".into()))
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;

    fn context(action: &str, at: i64) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin),
            permission: Permission::parse(format!("customer:{action}")).unwrap(),
            resource: "customer".into(),
            model: None,
            as_of: Instant::from_unix_secs(at),
        }
    }

    #[tokio::test]
    async fn mapping_requires_the_original_transaction_and_expires_at_shanghai_midnight() {
        let mapping = CustomerMapping::new();
        for (at, midnight) in [(0, 57600), (57599, 57600), (57600, 144000)] {
            let facts = mapping.prepare(&context("detail", at), &mut NoTransaction).await.unwrap();
            assert_eq!(facts.valid_until, Some(Instant::from_unix_secs(midnight)));
            assert!(facts.validate(true, Instant::from_unix_secs(at)).is_ok());
            assert!(facts.validate(false, Instant::from_unix_secs(at)).is_err());
            assert!(facts.validate(true, Instant::from_unix_secs(midnight)).is_err());
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
                mapping.predicate(&fact, &context(action, 0), &MappingFacts::default()).unwrap()
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
        let mapping: Arc<dyn AccessMapping> = Arc::new(CustomerMapping::new());
        for (action, history, expected) in [
            ("list", true, true),
            ("detail", true, true),
            ("detail", false, false),
            ("create", true, false),
            ("update", true, false),
            ("delete", true, false),
        ] {
            let entry = registration(action, mapping.clone(), history);
            let AccessMode::Query { data, .. } = &entry.access else {
                panic!("客户使用完整对象查询模型")
            };
            let AccessPolicy::Scope { read, .. } = &data.policy else {
                panic!("客户使用标准范围政策")
            };
            assert_eq!(*read, ReadAccess { history: expected, records: false });
            assert!(AuthorizationRegistry::new(vec![entry]).is_ok());
        }
        assert!(
            mapping
                .predicate(&AccessFact::Participant, &context("detail", 0), &MappingFacts::default())
                .is_err()
        );
        assert!(
            mapping
                .relation(
                    &RelationKey::parse("orders").unwrap(),
                    &context("detail", 0),
                    &MappingFacts::default()
                )
                .is_err()
        );
    }
}
