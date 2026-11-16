//! 主体读取的授权及查询准备步骤；数据库读取沿用同一执行器。
use application_core::normalized_text;

use super::*;
use crate::dto::party::{PARTY_REVISION_SORT_FIELDS, PartyListQuery, normalize_paging};

impl PartyService {
    /// 证明列表读取资格后规范化筛选条件。
    /// # 参数
    /// actor 为读取账号，params 为原始筛选，executor 为原读取快照。
    /// # 返回
    /// 通过授权和输入校验的列表条件。
    /// # 错误
    /// 缺少资格、非快照或筛选非法时拒绝。
    pub(super) async fn list_query(
        &self,
        actor: &AuditActor,
        params: &PartyListParams,
        executor: &mut dyn Executor,
    ) -> Result<PartyListQuery> {
        self.authorization.require(actor, &party::LIST, AccessTarget::Operation, executor).await?;
        params.validate()?;
        params.normalized()
    }

    /// 证明详情读取资格后返回仓储使用的主体编号。
    /// # 参数
    /// actor 为读取账号，id 为目标主体，executor 为原读取快照。
    /// # 返回
    /// 授权通过后返回详情仓储使用的编号。
    /// # 错误
    /// 缺少资格或非快照时拒绝。
    pub(super) async fn detail_id(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PartyId> {
        self.authorization.require(actor, &party::DETAIL, AccessTarget::Operation, executor).await?;
        Ok(PartyId::new(id))
    }

    /// 证明修订读取资格后构造分页筛选。
    /// # 参数
    /// actor 为读取账号，party_id 为目标主体，params 为原始筛选，executor 为原读取快照。
    /// # 返回
    /// 通过授权和输入校验的修订分页条件。
    /// # 错误
    /// 缺少资格、非快照或筛选非法时拒绝。
    pub(super) async fn revision_filter(
        &self,
        actor: &AuditActor,
        party_id: &str,
        params: &PartyRevisionListParams,
        executor: &mut dyn Executor,
    ) -> Result<PartyRevisionFilter> {
        self.authorization.require(actor, &party::REVISION_LIST, AccessTarget::Operation, executor).await?;
        params.validate()?;
        let paging = normalize_paging(
            &params.sort_by,
            &params.sort_dir,
            params.page,
            params.page_size,
            PARTY_REVISION_SORT_FIELDS,
        )?;
        Ok(PartyRevisionFilter {
            party_id: Some(PartyId::new(party_id)),
            legal_name: normalized_text(params.legal_name.as_deref()),
            short_name: normalized_text(params.short_name.as_deref()),
            page: paging.page,
            page_size: paging.page_size,
            sort_by: Some(paging.sort_by.to_string()),
            sort_ascending: matches!(paging.sort_dir, SortDir::Asc),
        })
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::fixed_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{FailClosedAuditPort, FailClosedSupplierRolePort};

    async fn service(granted: bool) -> PartyService {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("party_read_authorization");
        let permissions =
            if granted { vec![party::LIST, party::DETAIL, party::REVISION_LIST] } else { vec![] };
        let authorization = fixed_authorization(
            &permissions,
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        PartyService::new(
            db,
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedSupplierRolePort),
            authorization,
        )
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn reads_allow_permissions_and_prepare_real_list_detail_and_revision_inputs() {
        let service = service(true).await;
        let mut executor = SnapshotExecutor::default();
        let params = PartyListParams {
            keyword: Some("  主体编号  ".into()),
            page: Some(2),
            page_size: Some(7),
            ..Default::default()
        };
        let query = service.list_query(&actor(), &params, &mut executor).await.unwrap();
        assert_eq!(query.keyword.as_deref(), Some("主体编号"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 7);
        assert_eq!(service.detail_id(&actor(), "party-1", &mut executor).await.unwrap().as_ref(), "party-1");
        let params =
            serde_json::from_value(serde_json::json!({"legal_name":"  法定名称  ","page":3,"page_size":5}))
                .unwrap();
        let filter = service.revision_filter(&actor(), "party-1", &params, &mut executor).await.unwrap();
        assert_eq!(filter.party_id, Some(PartyId::new("party-1")));
        assert_eq!(filter.legal_name.as_deref(), Some("法定名称"));
        assert_eq!(filter.page, 3);
        assert_eq!(filter.page_size, 5);
    }

    #[tokio::test]
    async fn reads_reject_missing_permissions_before_party_business_queries() {
        let service = service(false).await;
        let params = serde_json::from_value(serde_json::json!({})).unwrap();
        let mut executor = SnapshotExecutor::default();
        let errors = [
            service
                .party_list_snapshot(&actor(), &PartyListParams::default(), &mut executor)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .party_detail_snapshot(&actor(), "unread-party", &mut executor)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .party_revision_list_snapshot(&actor(), "unread-party", &params, &mut executor)
                .await
                .map(|_| ())
                .unwrap_err(),
        ];
        for error in errors {
            assert!(matches!(error, Error::Forbidden(_)));
        }
    }

    #[tokio::test]
    async fn reads_reject_no_transaction_despite_valid_permissions() {
        let service = service(true).await;
        let params = serde_json::from_value(serde_json::json!({})).unwrap();
        let errors = [
            service
                .party_list_snapshot(&actor(), &PartyListParams::default(), &mut NoTransaction)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .party_detail_snapshot(&actor(), "unread-party", &mut NoTransaction)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .party_revision_list_snapshot(&actor(), "unread-party", &params, &mut NoTransaction)
                .await
                .map(|_| ())
                .unwrap_err(),
        ];
        for error in errors {
            assert!(
                matches!(error, Error::ValidationError(message) if message == "无对象操作及其业务读取必须使用快照事务")
            );
        }
    }
}
