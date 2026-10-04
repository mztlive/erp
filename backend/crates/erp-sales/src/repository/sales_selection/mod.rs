//! 域 `sales_selection` 仓储：列表过滤、白名单排序与跨集合原子写入。
//!
//! 单集合 CRUD 直接复用 `owned` 仓储；本模块只承载跨集合多步骤写入入口与
//! 列表投影查询。集合名统一取 `SalesSelectionExt` 关联常量。

mod proposal_scope;
mod queries;
mod rate;
pub mod scope;

use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
pub use queries::{
    SalesSelectionBookletRepositoryExt, SalesSelectionDisplayItemRepositoryExt,
    SalesSelectionIdempotencyRepositoryExt, SalesSelectionPoolMemberRepositoryExt,
    SalesSelectionPrepareTaskRepositoryExt, SalesSelectionProposalDisplayLineRepositoryExt,
    SalesSelectionProposalRepositoryExt, SalesSelectionProposalSkuLineRepositoryExt,
    SalesSelectionSessionRepositoryExt,
};
pub use rate::SalesSelectionRateRepository;
pub use scope::{SelectionReadScope, SelectionScopeClause};

use crate::entity::sales_selection::{
    SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionPoolMember, SalesSelectionPrepareTask,
    SalesSelectionProposal,
};
use crate::repository::extensions::SalesSelectionExt;
use crate::repository::filter::push_undeleted;

/// 选品册集合名（单一来源）。
const BOOKLETS: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_BOOKLETS;
/// 陈列项集合名。
const ITEMS: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_DISPLAY_ITEMS;
/// 商品池成员集合名。
const POOL: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_POOL_MEMBERS;
/// 方案集合名（单一来源）。
const PROPOSALS: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_PROPOSALS;

/// 选品册列表允许的排序字段白名单。
pub const BOOK_SORT_FIELDS: &[&str] = &["created_at", "status"];

/// 选品册列表筛选条件。
#[derive(Debug, Clone)]
pub struct SelectionBookFilter {
    /// 入口授权后的客户集合。None 表示全量，空集合表示无权查看任何客户。
    pub authorized_customer_ids: Option<Vec<String>>,
    /// 已证明的选品责任范围；仓储只执行条件，不推断权限。
    pub authorized_scope: SelectionReadScope,
    /// 负责人筛选；`None` 表示不筛选。
    pub owner_user_ids: Option<Vec<String>>,
    /// 业务组织筛选；`None` 表示不筛选。
    pub org_unit_ids: Option<Vec<String>>,
    /// 客户；`None` 表示不筛选。
    pub customer_id: Option<String>,
    /// 形态；`None` 表示不筛选。
    pub form: Option<crate::entity::sales_selection::SelectionForm>,
    /// 状态；`None` 表示不筛选。
    pub status: Option<crate::entity::sales_selection::BookletStatus>,
    /// 提交方式；`None` 表示不筛选。
    pub submit_mode: Option<crate::entity::sales_selection::SubmitMode>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
    /// 客户名称关键字。
    pub q: Option<String>,
}

impl Default for SelectionBookFilter {
    fn default() -> Self {
        Self {
            authorized_customer_ids: None,
            authorized_scope: SelectionReadScope::default(),
            owner_user_ids: None,
            org_unit_ids: None,
            customer_id: None,
            form: None,
            status: None,
            submit_mode: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
            q: None,
        }
    }
}

impl QueryFilter for SelectionBookFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut and: Vec<Document> = Vec::new();
        push_undeleted(&mut and);
        and.push(self.authorized_scope.document());
        if let Some(ids) = &self.authorized_customer_ids {
            and.push(doc! { "customer_id": { "$in": ids } });
        }
        if let Some(owners) = &self.owner_user_ids {
            and.push(doc! { "sales_owner_user_id": { "$in": owners } });
        }
        if let Some(orgs) = &self.org_unit_ids {
            and.push(doc! { "business_org_unit_id": { "$in": orgs } });
        }
        let mut filter = doc! { "$and": and };
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id);
        }
        if let Some(form) = self.form {
            filter.insert("form", form.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(mode) = self.submit_mode {
            filter.insert("submit_mode", mode.as_str());
        }
        if let Some(q) = self.q.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            insert_literal_regex_filter(&mut filter, "customer_name", Some(q));
        }
        filter
    }
}

impl Pagination for SelectionBookFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 方案列表筛选条件；方案沿所属册责任解释，不读提交人。
#[derive(Debug, Clone)]
pub struct SelectionProposalFilter {
    /// 入口授权后的客户集合。None 表示全量，空集合表示无权查看任何客户。
    pub authorized_customer_ids: Option<Vec<String>>,
    /// 已证明的方案责任范围；仓储只执行条件，不推断权限。
    pub authorized_scope: SelectionReadScope,
    /// 负责人筛选；`None` 表示不筛选。
    pub owner_user_ids: Option<Vec<String>>,
    /// 业务组织筛选；`None` 表示不筛选。
    pub org_unit_ids: Option<Vec<String>>,
    /// 客户；`None` 表示不筛选。
    pub customer_id: Option<String>,
    /// 选品册；指定时只返回该册唯一方案。
    pub booklet_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}

impl QueryFilter for SelectionProposalFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut and: Vec<Document> = Vec::new();
        push_undeleted(&mut and);
        if let Some(ids) = &self.authorized_customer_ids {
            and.push(doc! { "customer_id": { "$in": ids } });
        }
        let mut filter = doc! { "$and": and };
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id);
        }
        if let Some(booklet_id) = &self.booklet_id {
            filter.insert("booklet_id", booklet_id);
        }
        filter
    }
}

impl Pagination for SelectionProposalFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 校验列表排序字段白名单。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_ascending` - 是否升序
///
/// # 返回
/// 返回 `(字段, 升序)`；未提供时默认 `("created_at", false)`。
///
/// # 错误
/// 字段不在白名单时返回参数验证失败。
pub fn validate_book_sort(sort_by: Option<&str>, sort_ascending: bool) -> crate::Result<(String, bool)> {
    let field = sort_by.map(str::trim).filter(|value| !value.is_empty());
    let Some(field) = field else {
        return Ok(("created_at".to_string(), false));
    };
    if BOOK_SORT_FIELDS.contains(&field) {
        return Ok((field.to_string(), sort_ascending));
    }
    Err(crate::Error::ValidationError(format!("不支持的排序字段: {field}")))
}

/// 合并并集条件；空集合返回恒假，缺范围不得变全量。
///
/// # 参数
/// * `conditions` - 并集条件
///
/// # 返回
/// 返回 `$or` 或恒假条件。
///
/// # 错误
/// 无。
pub(crate) fn scope_union(conditions: Vec<Document>) -> Document {
    if conditions.is_empty() {
        return doc! { "$expr": false };
    }
    doc! { "$or": conditions }
}

/// 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入与列表查询。
pub struct SalesSelectionDomainRepository<'a> {
    db: &'a Database,
}

impl<'a> SalesSelectionDomainRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 分页检索选品册（整文档，供管理端列表）。
    ///
    /// 调用方必须先经 [`validate_book_sort`] 校验排序字段；本方法信任传入值。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页与总数。
    ///
    /// # 错误
    /// 查询或计数失败时返回仓储错误。
    pub async fn search_booklets(
        &self,
        filter: &SelectionBookFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesSelectionBooklet>> {
        let sort_field = filter.sort_by.as_deref().unwrap_or("created_at");
        let direction = if filter.sort_ascending { 1 } else { -1 };
        let options = FindOptions::builder()
            .sort(doc! { sort_field: direction, "id": direction })
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let condition = filter.to_doc();
        let items = mongo_ops::find_many(
            &self.db.collection::<SalesSelectionBooklet>(BOOKLETS),
            condition.clone(),
            options,
            executor,
        )
        .await?;
        let total = mongo_ops::count_documents(
            &self.db.collection::<SalesSelectionBooklet>(BOOKLETS),
            condition,
            executor,
        )
        .await?;
        Ok(PageResult { items, total: total as i64 })
    }

    /// 分页检索方案（整文档）；计数与取数使用同一授权条件。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页与总数，稳定排序含 ID。
    ///
    /// # 错误
    /// 查询或计数失败时返回仓储错误。
    pub async fn search_proposals(
        &self,
        filter: &SelectionProposalFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesSelectionProposal>> {
        proposal_scope::search(self.db, filter, executor).await
    }

    /// 列出某批次当前有效陈列。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 准备批次
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回有效且未删除的陈列项。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn list_effective_items(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesSelectionDisplayItem>> {
        self.db
            .sales_selection_display_items()
            .find_many(doc! { "booklet_id": booklet_id, "batch_id": batch_id, "effective": true }, executor)
            .await
    }

    /// 列出某批次冻结的商品池成员。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 准备批次
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回该批次全部成员。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn list_pool_members(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesSelectionPoolMember>> {
        self.db
            .sales_selection_pool_members()
            .find_many(doc! { "booklet_id": booklet_id, "batch_id": batch_id }, executor)
            .await
    }

    /// 按选品册读取方案。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 存在时返回方案；一册最多一份由唯一索引保证。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn find_proposal_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesSelectionProposal>> {
        self.db.sales_selection_proposals().find_one(doc! { "booklet_id": booklet_id }, executor).await
    }

    /// 持久化一次同步准备的全部结果。
    ///
    /// # 参数
    /// * `task` - 已标记成功的任务
    /// * `booklet` - 已完成准备的选品册
    /// * `members` - 冻结的商品池成员
    /// * `items` - 生成的陈列项
    /// * `executor` - 执行器，必须位于事务中
    ///
    /// # 返回
    /// 成功写入全部结果。
    ///
    /// # 错误
    /// 唯一冲突或写入失败时返回仓储错误，调用方回滚。
    pub async fn persist_prepare_success(
        &self,
        task: &SalesSelectionPrepareTask,
        booklet: &mut SalesSelectionBooklet,
        members: &[SalesSelectionPoolMember],
        items: &[SalesSelectionDisplayItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut task = task.clone();
        self.db.sales_selection_prepare_tasks().update(&mut task, executor).await?;
        if task.kind.reuses_current_batch() {
            let old = self
                .list_effective_items(
                    &booklet.base.id,
                    booklet.current_batch_id.as_deref().unwrap_or(""),
                    executor,
                )
                .await?;
            for mut item in old {
                let targeted = task.kind == crate::entity::sales_selection::PrepareKind::RegeneratedAll
                    || matches!(&item.kind, crate::entity::sales_selection::DisplayKind::Package { tier_id, .. }
                        if task.tier_ids.contains(tier_id));
                if targeted {
                    item.effective = false;
                    self.db.sales_selection_display_items().update(&mut item, executor).await?;
                }
            }
        }
        self.db.sales_selection_booklets().update(booklet, executor).await?;
        self.insert_members_and_items(members, items, executor).await
    }

    /// 插入商品池成员与陈列项。
    ///
    /// # 参数
    /// * `members` - 成员
    /// * `items` - 陈列项
    /// * `executor` - 执行器，必须位于事务中
    ///
    /// # 返回
    /// 成功插入。
    ///
    /// # 错误
    /// 写入失败时返回仓储错误。
    async fn insert_members_and_items(
        &self,
        members: &[SalesSelectionPoolMember],
        items: &[SalesSelectionDisplayItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !members.is_empty() {
            mongo_ops::insert_many(&self.db.collection::<SalesSelectionPoolMember>(POOL), members, executor)
                .await?;
        }
        if !items.is_empty() {
            mongo_ops::insert_many(&self.db.collection::<SalesSelectionDisplayItem>(ITEMS), items, executor)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::{Pagination, QueryFilter};

    use super::{SelectionBookFilter, validate_book_sort};

    fn filter() -> SelectionBookFilter {
        SelectionBookFilter {
            authorized_customer_ids: None,
            authorized_scope: super::SelectionReadScope {
                roles: vec![super::SelectionScopeClause { company: true, ..Default::default() }],
                ..Default::default()
            },
            owner_user_ids: None,
            org_unit_ids: None,
            customer_id: Some("cust-1".into()),
            form: None,
            status: Some(crate::entity::sales_selection::BookletStatus::Draft),
            submit_mode: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
            q: None,
        }
    }

    #[test]
    fn filter_applies_optional_fields_and_deleted_guard() {
        let document = filter().to_doc();
        assert_eq!(document.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(document.get_str("status").unwrap(), "DRAFT");
        let and = document.get_array("$and").unwrap();
        assert!(and.iter().any(|item| item.as_document().unwrap().contains_key("deleted_at")));
        assert_eq!(filter().page_and_size(), (1, 20));
    }

    #[test]
    fn customer_scope_intersects_explicit_filter_and_empty_scope_stays_empty() {
        let mut scoped = filter();
        scoped.authorized_customer_ids = Some(vec!["allowed".into()]);
        let doc = scoped.to_doc();
        assert_eq!(doc.get_str("customer_id").unwrap(), "cust-1");
        let and = doc.get_array("$and").unwrap();
        assert!(and.iter().any(|item| item.as_document().unwrap()
            == &mongodb::bson::doc! { "customer_id": { "$in": ["allowed"] } }));
        scoped.authorized_customer_ids = Some(vec![]);
        let doc = scoped.to_doc();
        let and = doc.get_array("$and").unwrap();
        assert!(
            and.iter()
                .any(|item| item.as_document().unwrap()
                    == &mongodb::bson::doc! { "customer_id": { "$in": [] } })
        );
    }

    #[test]
    fn owner_and_org_filters_intersect_authorized_scope() {
        let mut scoped = filter();
        scoped.owner_user_ids = Some(vec!["sales-a".into()]);
        scoped.org_unit_ids = Some(vec!["org-a".into()]);
        let doc = scoped.to_doc();
        let and = doc.get_array("$and").unwrap();
        assert!(and.iter().any(|item| item.as_document().unwrap()
            == &mongodb::bson::doc! { "sales_owner_user_id": { "$in": ["sales-a"] } }));
        assert!(and.iter().any(|item| item.as_document().unwrap()
            == &mongodb::bson::doc! { "business_org_unit_id": { "$in": ["org-a"] } }));
    }

    #[test]
    fn sort_whitelist_defaults_and_rejects_unknown() {
        assert_eq!(validate_book_sort(None, false).unwrap(), ("created_at".to_string(), false));
        assert_eq!(validate_book_sort(Some("status"), true).unwrap(), ("status".to_string(), true));
        assert!(validate_book_sort(Some("price"), false).is_err());
        assert!(validate_book_sort(Some("  "), false).unwrap().0 == "created_at");
    }
}
