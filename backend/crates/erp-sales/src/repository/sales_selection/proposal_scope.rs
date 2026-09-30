//! 方案查询沿当前所属册授权；方案内冻结责任字段不参与当前权限判定。

use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result};
use serde::Deserialize;

use super::{BOOKLETS, PROPOSALS, SelectionProposalFilter};
use crate::entity::sales_selection::SalesSelectionProposal;
use crate::repository::filter::push_undeleted;

/// 聚合分页结果；同一管道内先授权，再计数和分页。
#[derive(Debug, Default, Deserialize)]
struct ProposalPage {
    items: Vec<SalesSelectionProposal>,
    total: Vec<ProposalCount>,
}

#[derive(Debug, Deserialize)]
struct ProposalCount {
    count: i64,
}

/// 用当前册责任过滤方案，缺少有效父册的方案保持不可见。
///
/// # 参数
/// * `db` - 本域数据库
/// * `filter` - 已证明方案动作权限的来源责任条件
/// * `executor` - 调用方执行器
/// # 返回
/// 授权后的分页及总数。
/// # 错误
/// 聚合或反序列化失败时返回仓储错误。
pub(super) async fn search(
    db: &Database,
    filter: &SelectionProposalFilter,
    executor: &mut dyn Executor,
) -> Result<PageResult<SalesSelectionProposal>> {
    let collection = db.collection::<Document>(PROPOSALS);
    let pipeline = pipeline(filter);
    let mut pages = if let Some(session) = executor.session() {
        collection
            .aggregate(pipeline)
            .with_type::<ProposalPage>()
            .session(&mut *session)
            .await?
            .stream(session)
            .try_collect::<Vec<_>>()
            .await?
    } else {
        collection.aggregate(pipeline).with_type::<ProposalPage>().await?.try_collect::<Vec<_>>().await?
    };
    let page = pages.pop().unwrap_or_default();
    Ok(PageResult { items: page.items, total: page.total.first().map_or(0, |row| row.count) })
}

/// 先按方案业务筛选缩小集合，再通过有索引的父册 ID 连接授权事实。
fn pipeline(filter: &SelectionProposalFilter) -> Vec<Document> {
    vec![
        doc! { "$match": filter.to_doc() },
        doc! { "$lookup": {
            "from": BOOKLETS,
            "localField": "booklet_id",
            "foreignField": "id",
            "pipeline": [{ "$match": booklet_condition(filter) }, { "$project": { "id": 1 } }],
            "as": "authorized_booklet"
        } },
        doc! { "$match": { "authorized_booklet.0": { "$exists": true } } },
        doc! { "$unset": "authorized_booklet" },
        doc! { "$facet": {
            "items": [
                { "$sort": { "submitted_at": -1, "id": -1 } },
                { "$skip": i64::try_from(filter.skip()).unwrap_or(i64::MAX) },
                { "$limit": filter.limit() }
            ],
            "total": [{ "$count": "count" }]
        } },
    ]
}

/// 负责人和组织筛选也按当前册解释，不读取方案提交时的冻结责任。
fn booklet_condition(filter: &SelectionProposalFilter) -> Document {
    let mut conditions = vec![filter.authorized_scope.document()];
    push_undeleted(&mut conditions);
    if let Some(owners) = &filter.owner_user_ids {
        conditions.push(doc! { "sales_owner_user_id": { "$in": owners } });
    }
    if let Some(orgs) = &filter.org_unit_ids {
        conditions.push(doc! { "business_org_unit_id": { "$in": orgs } });
    }
    doc! { "$and": conditions }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::sales_selection::{SelectionReadScope, SelectionScopeClause};

    fn filter(scope: SelectionReadScope) -> SelectionProposalFilter {
        SelectionProposalFilter {
            authorized_customer_ids: None,
            authorized_scope: scope,
            owner_user_ids: Some(vec!["current-owner".into()]),
            org_unit_ids: None,
            customer_id: None,
            booklet_id: None,
            page: 1,
            page_size: 20,
        }
    }

    #[test]
    fn responsibility_conditions_are_applied_to_current_booklet() {
        let scope = SelectionReadScope {
            roles: vec![SelectionScopeClause {
                owner_user_id: Some("current-owner".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let filter = filter(scope);
        let stages = pipeline(&filter);
        let lookup = stages[1].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("localField").unwrap(), "booklet_id");
        assert_eq!(lookup.get_str("from").unwrap(), BOOKLETS);
        assert_eq!(
            lookup.get_array("pipeline").unwrap()[0].as_document().unwrap().get_document("$match").unwrap(),
            &booklet_condition(&filter)
        );
        assert!(!filter.to_doc().get_array("$and").unwrap().iter().any(|condition| {
            condition.as_document().is_some_and(|doc| doc.contains_key("sales_owner_user_id"))
        }));
        assert_eq!(stages[2], doc! { "$match": { "authorized_booklet.0": { "$exists": true } } });
    }

    #[test]
    fn empty_scope_remains_denied_before_pagination() {
        let filter = filter(SelectionReadScope::default());
        let condition = booklet_condition(&filter);
        assert_eq!(condition.get_array("$and").unwrap()[0].as_document(), Some(&doc! { "$expr": false }));
        let stages = pipeline(&filter);
        assert!(stages.last().unwrap().contains_key("$facet"));
    }
}
