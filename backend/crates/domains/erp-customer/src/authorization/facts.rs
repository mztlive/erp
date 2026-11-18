//! 客户完整对象事实；多名负责人及其组织均按原执行器读取。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts, ResourceTargets};
use erp_identity::{AccessFactsPort, Error, OrganizationDirectoryService, Result};
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use crate::entity::customer::{AssignmentRole, CustomerAccount, CustomerAssignment};
use crate::repository::CustomerExt;
use crate::service::customer::access::business_date;

/// 客户拥有领域提供的完整客户及多负责人事实。
pub struct CustomerFacts(Database);

impl CustomerFacts {
    /// 绑定拥有领域的客户和归属集合。
    pub(super) fn new(db: Database) -> Self {
        Self(db)
    }

    /// 创建可供跨域来源授权复用的客户事实端口。
    /// # 参数
    /// db 为客户集合所在数据库。
    /// # 返回
    /// 仅装载完整业务事实、不组合权限政策的共享端口。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn shared(db: Database) -> Arc<dyn AccessFactsPort> {
        Arc::new(Self::new(db))
    }

    /// 读取当前负责人组织；缺失主属组织保持已知空值，重叠主属关系返回错误。
    async fn owner_organizations(
        &self,
        owners: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<(BTreeSet<String>, Instant)> {
        let directory = OrganizationDirectoryService::new(self.0.clone());
        let mut organizations = BTreeSet::new();
        let mut valid_until = super::next_day(at);
        for owner in owners {
            let (organization, boundary) = directory.own_org_with_validity(owner, at, executor).await?;
            if let Some(org) = organization {
                organizations.insert(org);
            }
            if let Some(boundary) = boundary {
                valid_until = valid_until.min(boundary);
            }
        }
        Ok((organizations, valid_until))
    }
}

#[async_trait]
impl AccessFactsPort for CustomerFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let assignments = self.0.customer_assignments().find_many(
            doc! {"customer_id": {"$in": ids.ids().iter().collect::<Vec<_>>()}, "assignment_role": "OWNER"},
            executor,
        ).await?;

        let mut objects = Vec::new();
        for id in ids.ids() {
            let Some(account) = self.0.customer_accounts().find_by_id(id, executor).await? else {
                continue;
            };
            let rows = assignments.iter().filter(|row| row.customer_id.as_ref() == id).collect::<Vec<_>>();
            let (owners, history) = responsibility(&rows, context)?;
            let (organizations, valid_until) =
                self.owner_organizations(&owners, context.as_of, executor).await?;
            objects.push(object(&account, &rows, owners, organizations, history, valid_until));
        }
        Ok(objects)
    }
}

/// 当前读写取有效负责人；创建检查已写入的拟生效归属，历史参与保留全部 OWNER。
fn responsibility(
    rows: &[&CustomerAssignment],
    context: &AccessContext,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let date = business_date(context.as_of).map_err(|error| Error::ValidationError(error.to_string()))?;
    let owners = rows
        .iter()
        .filter(|row| {
            !row.base.is_deleted()
                && row.assignment_role == AssignmentRole::Owner
                && ((context.permission.resource() == "customer" && context.permission.action() == "create")
                    || row.is_active_on(date))
        })
        .map(|row| row.user_id.clone())
        .collect();

    let history = rows
        .iter()
        .filter(|row| !row.base.is_deleted() && row.assignment_role == AssignmentRole::Owner)
        .map(|row| row.user_id.clone())
        .collect();
    Ok((owners, history))
}

/// 完整归属版本参与事实复验；日期边界使到期关系不能跨日继续使用。
fn object(
    account: &CustomerAccount,
    assignments: &[&CustomerAssignment],
    owners: BTreeSet<String>,
    organizations: BTreeSet<String>,
    history: BTreeSet<String>,
    valid_until: Instant,
) -> ObjectFacts {
    let mut revisions = assignments.iter().map(|row| (&row.base.id, row.base.version)).collect::<Vec<_>>();
    revisions.sort_unstable();
    ObjectFacts {
        resource: "customer".into(),
        id: account.base.id.clone(),
        version: format!("{}:{revisions:?}", account.base.version),
        valid_until: Some(valid_until),
        owner_ids: FactValue::Known(owners),
        collaborators: FactValue::Known(BTreeSet::new()),
        org_unit_ids: FactValue::Known(organizations),
        history_participants: FactValue::Known(history),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::ids::CustomerAccountId;
    use erp_permissions::{Permission, customer, sales_selection};

    use super::*;

    fn assignment(user: &str, from: &str, to: Option<&str>) -> CustomerAssignment {
        CustomerAssignment {
            base: BaseModel::fake(),
            customer_id: CustomerAccountId::new("customer"),
            user_id: user.into(),
            assignment_role: AssignmentRole::Owner,
            valid_from: from.parse().unwrap(),
            valid_to: to.map(|value| value.parse().unwrap()),
            change_reason: "test".into(),
        }
    }

    fn context(permission: Permission) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("a".into(), "a".into(), AccountKind::Admin),
            permission,
            resource: "customer".into(),
            model: None,
            as_of: Instant::from_unix_secs(86400),
        }
    }

    #[test]
    fn responsibility_preserves_every_current_owner_and_read_only_history() {
        let rows = [
            assignment("current-a", "1970-01-01", None),
            assignment("current-b", "1970-01-02", None),
            assignment("past", "1970-01-01", Some("1970-01-02")),
            assignment("future", "1970-01-03", None),
        ];
        let references = rows.iter().collect::<Vec<_>>();
        let (owners, history) = responsibility(&references, &context(customer::DETAIL)).unwrap();
        assert_eq!(owners, ["current-a".into(), "current-b".into()].into());
        assert_eq!(history, rows.iter().map(|row| row.user_id.clone()).collect());

        let (proposed, _) = responsibility(&references, &context(customer::CREATE)).unwrap();
        assert_eq!(proposed, history);
    }

    #[test]
    fn deleted_and_collaborator_rows_cannot_supply_owner_facts() {
        let mut deleted = assignment("deleted", "1970-01-01", None);
        deleted.base.deleted_at = 1;
        let mut collaborator = assignment("collaborator", "1970-01-01", None);
        collaborator.assignment_role = AssignmentRole::Collaborator;
        let (owners, history) =
            responsibility(&[&deleted, &collaborator], &context(customer::DETAIL)).unwrap();
        assert!(owners.is_empty());
        assert!(history.is_empty());
    }

    /// 旧协作行不能使对象判定或查询事实获得协作资格；当前负责人及公司范围仍按原规则工作。
    #[test]
    fn retired_collaboration_is_known_empty_and_matches_the_query_denial() {
        use erp_core::ids::PartyId;
        use erp_identity::AccessMapping;
        use erp_identity::entity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
        use erp_identity::entity::authorization::{AccessFact, FactKind};
        use erp_identity::ports::QueryFragment;

        use crate::authorization::CustomerMapping;
        use crate::entity::customer::{CustomerAccountData, CustomerAccountStatus};

        let owner = assignment("current-owner", "1970-01-01", None);
        let mut legacy = assignment("legacy-helper", "1970-01-01", None);
        legacy.assignment_role = AssignmentRole::Collaborator;
        let rows = [&owner, &legacy];
        let (owners, history) = responsibility(&rows, &context(customer::DETAIL)).unwrap();
        let account = CustomerAccount::new(
            CustomerAccountId::new("customer"),
            CustomerAccountData {
                party_id: PartyId::new("party"),
                customer_no: "C-1".into(),
                default_payment_term_id: None,
                status: CustomerAccountStatus::Active,
            },
            "creator",
        )
        .unwrap();
        let facts = object(
            &account,
            &rows,
            owners,
            ["org".into()].into(),
            history,
            super::super::next_day(context(customer::DETAIL).as_of),
        );
        let known = facts.collaborators.get().expect("协作事实必须已知，不能以Missing绕过完整性");
        assert!(known.is_empty());

        let mapping = CustomerMapping::new();
        assert!(mapping.model().facts.contains(&FactKind::Collaborators));
        let QueryFragment::Match(filter) =
            mapping.predicate(&AccessFact::Collaborating, &context(customer::DETAIL), None).unwrap()
        else {
            panic!("退役协作查询不得读取协作集合");
        };
        let query_allows = filter.get_bool("$expr").unwrap();

        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { collaborative: true, ..Default::default() }],
            user_limit: None,
        };
        for actor in ["legacy-helper", "current-owner", "unrelated"] {
            let scoped = ScopedObject {
                owned: facts.owner_ids.get().unwrap().contains(actor),
                collaborating: known.contains(actor),
                historical_read_participant: false,
                org_unit_id: Some("org"),
                settlement_party_id: None,
                warehouse_id: None,
            };
            assert_eq!(scope.allows(&scoped, false), query_allows);
            assert!(!query_allows);
            let company = ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            };
            assert!(company.allows(&scoped, false));
        }
    }

    #[test]
    fn cross_domain_create_keeps_customer_current_owner_window() {
        let ended = assignment("past", "1970-01-01", Some("1970-01-02"));
        let source = context(sales_selection::BOOKLET_CREATE);
        let (owners, history) = responsibility(&[&ended], &source).unwrap();
        assert!(owners.is_empty());
        assert_eq!(history, ["past".into()].into());
    }
}
