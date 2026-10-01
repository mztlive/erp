//! 单一来源应收/应付：先形成真实来源条件，再分页计数及投影完整轻量版本。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use erp_core::money::Amount;
use erp_finance::dto::payable::PayableAccountListQuery;
use erp_finance::dto::receivable::ReceivableAccountListQuery;
use erp_finance::repository::keyword::FinanceSearchTarget;
use erp_finance::repository::receivable::ReceivableAccountRow;
use erp_finance::repository::{
    PayableAccountFilter, PayableAccountRow, PayableExt, ReceivableAccountFilter, ReceivableExt,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_supply::repository::SupplierSettlementExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, QueryFilter};
use serde::Deserialize;

use super::super::{FundsAccess, FundsAuthorization, FundsLinkedCondition};
use super::{
    aggregate, linked_condition_document, page_facet, sort_document, source_scope_document, source_stages,
};
use crate::finance::search::keyword_ids;
use crate::{Error, Result};

/// 当前页只投影列表字段及一条真实来源责任事实。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct AccountPageRow<T> {
    #[serde(flatten)]
    pub(in crate::finance::funds_scope) row: T,
    #[serde(rename = "_source")]
    pub(in crate::finance::funds_scope) source: AccountSource,
}

/// 页面来源投影不读取销售、采购或结算完整实体。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct AccountSource {
    pub(in crate::finance::funds_scope) id: String,
    pub(in crate::finance::funds_scope) owner_user_id: Option<String>,
    pub(in crate::finance::funds_scope) business_org_unit_id: String,
    pub(in crate::finance::funds_scope) version: u64,
    pub(in crate::finance::funds_scope) document_no: String,
}

/// 全部最终匹配行的最小跨页材料，顺序保持原列表排序。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct AccountVersion {
    pub(in crate::finance::funds_scope) id: String,
    pub(in crate::finance::funds_scope) version: u64,
    pub(in crate::finance::funds_scope) source_id: String,
    pub(in crate::finance::funds_scope) source_version: u64,
}

/// 全范围金额摘要只取原加总所需金额和归属，不载入完整行。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct AccountSummaryRow {
    pub(in crate::finance::funds_scope) id: String,
    pub(in crate::finance::funds_scope) order_id: String,
    pub(in crate::finance::funds_scope) owner_user_id: Option<String>,
    pub(in crate::finance::funds_scope) gross_total: Amount,
    pub(in crate::finance::funds_scope) settled_total: Amount,
}

/// 同一事务和同一最终条件生成的页面、计数、版本和金额输入。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct AccountListSnapshot<T> {
    pub(in crate::finance::funds_scope) items: Vec<AccountPageRow<T>>,
    pub(in crate::finance::funds_scope) versions: Vec<AccountVersion>,
    pub(in crate::finance::funds_scope) summary: Vec<AccountSummaryRow>,
    total: Vec<AccountCount>,
}

#[derive(Deserialize)]
struct AccountCount {
    count: u64,
}

impl<T> AccountListSnapshot<T> {
    /// 返回数据库完整计数；空结果保持零。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 与最终条件一致的总数。
    /// # 错误
    /// 无。
    pub(in crate::finance::funds_scope) fn total(&self) -> u64 {
        self.total.first().map_or(0, |row| row.count)
    }

    /// 保留原授权版本、每条匹配行与真实来源版本的哈希输入顺序。
    ///
    /// # 参数
    /// 原事务中解析的授权上下文。
    /// # 返回
    /// 完整匹配集合的跨页指纹。
    /// # 错误
    /// 匹配数量达到原一万条保护时整体拒绝。
    pub(in crate::finance::funds_scope) fn version(
        &self,
        authorization: &FundsAuthorization,
    ) -> Result<String> {
        if self.versions.len() >= 10_000 {
            return Err(Error::ValidationError(format!(
                "{}查询超过上限，请收窄组织或负责人条件",
                if authorization.context.resource == "receivable_account" { "应收" } else { "应付" }
            )));
        }
        let mut fingerprint = DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for row in &self.versions {
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            row.source_id.hash(&mut fingerprint);
            row.source_version.hash(&mut fingerprint);
        }
        Ok(format!("{:x}", fingerprint.finish()))
    }
}

impl FundsAccess {
    /// 应收查询的授权和来源筛选先进入关联管道，页外仅返回版本与金额摘要。
    ///
    /// # 参数
    /// 规范化业务条件、已解析授权、已展开关联条件、是否生成页面及原执行器。
    /// # 返回
    /// 同一快照的最终页面、计数、完整轻量版本和窄金额输入。
    /// # 错误
    /// 关键词查询、MongoDB 聚合、分页或解码失败时拒绝。
    pub(in crate::finance::funds_scope) async fn receivable_database_snapshot(
        &self,
        query: &ReceivableAccountListQuery,
        authorization: &FundsAuthorization,
        condition: &FundsLinkedCondition,
        include_page: bool,
        executor: &mut dyn Executor,
    ) -> Result<AccountListSnapshot<ReceivableAccountRow>> {
        let filter = receivable_filter(
            query,
            keyword_ids(&self.db, query.q.as_deref(), FinanceSearchTarget::Receivable).await?,
        );
        let scope = doc! { "$and": [authorization.sales.document(),
        source_scope_document(&authorization.context, "sales_owner_user_id"),
        linked_condition_document(condition, "sales_owner_user_id")] };
        let mut pipeline = vec![doc! { "$match": filter.to_doc() }];
        if let Some(ids) = &condition.operator_user_ids {
            pipeline.push(doc! { "$match": { "created_by": { "$in": ids } } });
        }
        pipeline.extend(source_stages(
            <Database as SalesOrderExt>::SALES_ORDERS,
            Bson::String("$sales_order_id".into()),
            "_source",
            scope,
            "sales_owner_user_id",
            "order_no",
        ));
        pipeline.push(doc! { "$match": { "_source_allowed": true } });
        let sort = sort_document(
            query.paging.sort_by,
            filter.sort_ascending,
            &[
                "account_seq",
                "gross_total",
                "settled_total",
                "open_total",
                "open_invoiceable_total",
                "created_at",
            ],
        );
        self.account_snapshot(
            <Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
            pipeline,
            sort,
            query.paging.page,
            query.paging.page_size,
            "sales_order_id",
            include_page,
            executor,
        )
        .await
    }

    /// 应付按真实来源类型关联；采购与结算同名主键不能串权。
    ///
    /// # 参数
    /// 规范化业务条件、已解析真实来源范围、关联条件、页面开关及原执行器。
    /// # 返回
    /// 同一最终来源条件的页面、计数、版本和金额摘要。
    /// # 错误
    /// 查询、分页或类型化解码失败时返回错误。
    pub(in crate::finance::funds_scope) async fn payable_database_snapshot(
        &self,
        query: &PayableAccountListQuery,
        authorization: &FundsAuthorization,
        condition: &FundsLinkedCondition,
        include_page: bool,
        executor: &mut dyn Executor,
    ) -> Result<AccountListSnapshot<PayableAccountRow>> {
        let filter = payable_filter(
            query,
            keyword_ids(&self.db, query.q.as_deref(), FinanceSearchTarget::Payable).await?,
        );
        let mut pipeline = vec![doc! { "$match": filter.to_doc() }];
        pipeline.extend(payable_source_stages(authorization, condition));
        pipeline.push(doc! { "$match": { "_source_allowed": true } });
        let sort = sort_document(
            query.paging.sort_by,
            filter.sort_ascending,
            &["gross_total", "settled_total", "open_total", "open_invoiceable_total", "created_at"],
        );
        self.account_snapshot(
            <Database as PayableExt>::PAYABLE_ACCOUNTS,
            pipeline,
            sort,
            query.paging.page,
            query.paging.page_size,
            "source_document_id",
            include_page,
            executor,
        )
        .await
    }

    /// 一次最终条件生成页与完整轻量索引；重验仅保留版本分支。
    #[allow(clippy::too_many_arguments)]
    async fn account_snapshot<T: for<'de> Deserialize<'de> + Send + Sync>(
        &self,
        collection: &str,
        mut pipeline: Vec<Document>,
        sort: Document,
        page: u64,
        size: u32,
        source_field: &str,
        include_page: bool,
        executor: &mut dyn Executor,
    ) -> Result<AccountListSnapshot<T>> {
        let versions = doc! { "_id": 0, "id": 1, "version": 1,
        "source_id": format!("${source_field}"), "source_version": "$_source.version" };
        let summary = vec![
            doc! { "$sort": sort.clone() },
            doc! { "$limit": 10001 },
            doc! { "$project": { "_id": 0, "id": 1, "order_id": format!("${source_field}"),
            "owner_user_id": "$_source.owner_user_id", "gross_total": 1, "settled_total": 1 } },
        ];
        let facet = if include_page {
            page_facet(page, size, sort, account_projection(), versions, summary)?
        } else {
            doc! { "$facet": { "items": [{ "$match": { "$expr": false } }], "total": [{ "$match": { "$expr": false } }],
            "summary": [{ "$match": { "$expr": false } }], "versions": [
                { "$sort": sort }, { "$limit": 10001 }, { "$project": versions }] } }
        };
        pipeline.push(facet);
        aggregate(self.db.collection(collection), pipeline, executor)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::ValidationError("资金查询快照缺失".into()))
    }
}

/// 保留应收领域原业务条件，分页由最终范围管道执行。
fn receivable_filter(
    query: &ReceivableAccountListQuery,
    keyword_ids: Option<Vec<String>>,
) -> ReceivableAccountFilter {
    ReceivableAccountFilter {
        keyword_ids,
        account_id: query.account_id.clone(),
        customer_id: query.customer_id.clone(),
        counterparty_party_id: query.counterparty_party_id.clone(),
        status: query.status,
        sales_order_id: query.sales_order_id.clone(),
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, application_core::SortDir::Asc),
        ..Default::default()
    }
}

/// 保留应付领域业务条件，不把来源授权解释放入财务领域仓储。
fn payable_filter(query: &PayableAccountListQuery, keyword_ids: Option<Vec<String>>) -> PayableAccountFilter {
    PayableAccountFilter {
        source_document_id: query.source_document_id.clone(),
        keyword_ids,
        supplier_id: query.supplier_id.clone(),
        source_type: query.source_type,
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, application_core::SortDir::Asc),
    }
}

/// 当前页投影沿用财务领域列表字段，附带最小真实来源事实。
fn account_projection() -> Document {
    doc! { "_id": 0, "id": 1, "status": 1, "current_revision_id": 1,
    "created_by": 1, "updated_by": 1, "sales_order_id": 1, "account_seq": 1,
    "source_document_id": 1, "source_type": 1, "supplier_id": 1,
    "customer_id": 1, "counterparty_party_id": 1, "gross_total": 1,
    "settled_total": 1, "open_total": 1, "invoiceable_total": 1,
    "invoiced_total": 1, "open_invoiceable_total": 1, "version": 1,
    "created_at": 1, "_source": 1 }
}

/// 两种来源独立授权后按真实类型选择；未知类型保持不可见。
fn payable_source_stages(
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
) -> Vec<Document> {
    let scope = authorization
        .purchase_scope
        .as_ref()
        .map_or_else(|| doc! { "$expr": false }, |scope| scope.document());
    let mut stages = source_stages(
        <Database as PurchaseOrderExt>::PURCHASE_ORDERS,
        Bson::String("$source_document_id".into()),
        "_purchase",
        doc! { "$and": [scope, linked_condition_document(condition, "owner_user_id")] },
        "owner_user_id",
        "purchase_no",
    );
    let settlement_scope = authorization
        .settlement
        .as_ref()
        .map_or_else(|| doc! { "$expr": false }, |scope| source_scope_document(scope, "prepared_by"));
    stages.extend(source_stages(
        <Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        Bson::String("$source_document_id".into()),
        "_settlement",
        doc! { "$and": [settlement_scope, linked_condition_document(condition, "prepared_by")] },
        "prepared_by",
        "statement_no",
    ));
    stages.push(doc! { "$set": {
        "_source": { "$switch": { "branches": [
            { "case": { "$eq": ["$source_type", "purchase_order"] }, "then": "$_purchase" },
            { "case": { "$eq": ["$source_type", "supplier_settlement"] }, "then": "$_settlement" }], "default": Bson::Null } },
        "_source_allowed": { "$switch": { "branches": [
            { "case": { "$eq": ["$source_type", "purchase_order"] }, "then": "$_purchase_allowed" },
            { "case": { "$eq": ["$source_type", "supplier_settlement"] }, "then": "$_settlement_allowed" }], "default": false } },
    } });
    stages
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_identity::access_control::ResolvedScope;
    use erp_identity::service::access_control::resolve::AuthorizedDataScope;
    use erp_sales::repository::sales_order::scope::SalesReadScope;

    use super::*;

    /// 构造已解析上下文；版本测试不访问持久化或静态资格。
    fn authorization() -> FundsAuthorization {
        FundsAuthorization {
            sales: SalesReadScope::default(),
            ledger_read: false,
            settlement: None,
            purchase_scope: None,
            no_scope: false,
            context: AuthorizedDataScope {
                user_id: "u".into(),
                resource: "receivable_account".into(),
                action: "list".into(),
                scope: ResolvedScope { role_clauses: Vec::new(), user_limit: None },
                role_scopes: Default::default(),
                organizations: Default::default(),
                policy_version: 1,
                scope_version: "scope".into(),
                as_of: Instant::from_unix_secs(1),
            },
        }
    }

    /// 只保留范围内两个账户的版本，当前实体页为空以验证页外变化仍有效。
    fn snapshot() -> AccountListSnapshot<ReceivableAccountRow> {
        AccountListSnapshot {
            items: vec![],
            summary: vec![],
            total: vec![AccountCount { count: 2 }],
            versions: vec![
                AccountVersion { id: "a".into(), version: 2, source_id: "so-a".into(), source_version: 3 },
                AccountVersion { id: "b".into(), version: 4, source_id: "so-b".into(), source_version: 5 },
            ],
        }
    }

    /// 页外记录、来源交接、插入、删除和授权变化均使完整指纹失效。
    #[test]
    fn account_version_covers_all_matches_and_their_real_sources() {
        let access = authorization();
        let mut rows = snapshot();
        let original = rows.version(&access).unwrap();
        assert_eq!(rows.total(), 2);
        rows.versions[1].source_version += 1;
        assert_ne!(rows.version(&access).unwrap(), original);
        rows.versions[1].source_version -= 1;
        rows.versions[1].version += 1;
        assert_ne!(rows.version(&access).unwrap(), original);
        rows.versions.pop();
        assert_ne!(rows.version(&access).unwrap(), original);
        rows.versions.push(AccountVersion {
            id: "new".into(),
            version: 1,
            source_id: "so-new".into(),
            source_version: 1,
        });
        assert_ne!(rows.version(&access).unwrap(), original);
        let mut changed = authorization();
        changed.context.scope_version = "revoked".into();
        assert_ne!(snapshot().version(&changed).unwrap(), original);
    }

    /// 原有恰一万条拒绝边界保持，数据库计数不以当前页长度代替。
    #[test]
    fn account_complete_index_keeps_existing_capacity_boundary() {
        let access = authorization();
        let mut rows = snapshot();
        rows.versions = (0..9999)
            .map(|index| AccountVersion {
                id: index.to_string(),
                version: 1,
                source_id: "so".into(),
                source_version: 1,
            })
            .collect();
        assert!(rows.version(&access).is_ok());
        rows.versions.push(AccountVersion {
            id: "10000".into(),
            version: 1,
            source_id: "so".into(),
            source_version: 1,
        });
        assert!(matches!(rows.version(&access), Err(Error::ValidationError(_))));
    }
}
