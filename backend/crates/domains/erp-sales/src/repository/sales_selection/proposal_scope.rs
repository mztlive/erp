//! 方案查询沿当前所属册授权；方案内冻结责任字段不参与当前权限判定。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, aggregate};
use serde::Deserialize;

use super::{BOOKLETS, PROPOSALS, SelectionProposalFilter, SelectionReadScope};
use crate::Result;
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
    authorized_scope: &SelectionReadScope,
    executor: &mut dyn Executor,
) -> Result<PageResult<SalesSelectionProposal>> {
    authorized_scope.ensure_current(PROPOSALS, executor)?;
    let collection = db.collection::<ProposalPage>(PROPOSALS);
    let mut pages =
        aggregate::rows(&collection, pipeline(filter, authorized_scope.stages()), executor).await?;
    authorized_scope.ensure_current(PROPOSALS, executor)?;
    let page = pages.pop().unwrap_or_default();
    Ok(PageResult { items: page.items, total: page.total.first().map_or(0, |row| row.count) })
}

/// 先按方案业务筛选缩小集合，再通过有索引的父册 ID 连接授权事实。
fn pipeline(filter: &SelectionProposalFilter, authorization: Vec<Document>) -> Vec<Document> {
    let mut stages = vec![doc! { "$match": filter.to_doc() }];
    stages.extend(authorization);
    stages.extend(vec![
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
    ]);
    stages
}

/// 负责人和组织筛选也按当前册解释，不读取方案提交时的冻结责任。
fn booklet_condition(filter: &SelectionProposalFilter) -> Document {
    let mut conditions = Vec::new();
    push_undeleted(&mut conditions);
    if let Some(owners) = &filter.owner_user_ids {
        conditions.push(doc! { "sales_owner_user_id": { "$in": owners } });
    }
    if let Some(orgs) = &filter.org_unit_ids {
        conditions.push(doc! { "business_org_unit_id": { "$in": orgs } });
    }
    doc! { "$and": conditions }
}
