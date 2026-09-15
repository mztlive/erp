//! 域 `sales_selection` 仓储：列表过滤、白名单排序与跨集合原子写入。
//!
//! 单集合 CRUD 直接复用 `owned` 仓储；本模块只承载跨集合多步骤写入入口与
//! 列表投影查询。集合名统一取 `SalesSelectionExt` 关联常量。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, mongo_ops};

use crate::entity::sales_selection::{
    SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionIdempotency, SalesSelectionPoolMember,
    SalesSelectionPrepareTask, SalesSelectionProposal, SalesSelectionProposalDisplayLine,
    SalesSelectionProposalSkuLine, SalesSelectionSession,
};
use crate::repository::extensions::SalesSelectionExt;

/// 选品册集合名（单一来源）。
const BOOKLETS: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_BOOKLETS;
/// 陈列项集合名。
const ITEMS: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_DISPLAY_ITEMS;
/// 商品池成员集合名。
const POOL: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_POOL_MEMBERS;
/// 方案陈列行集合名。
const PROPOSAL_DISPLAY_LINES: &str =
    <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_PROPOSAL_DISPLAY_LINES;
/// 方案 SKU 行集合名。
const PROPOSAL_SKU_LINES: &str = <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_PROPOSAL_SKU_LINES;

/// 选品册列表允许的排序字段白名单。
pub const BOOK_SORT_FIELDS: &[&str] = &["created_at", "status"];

/// 选品册列表筛选条件。
#[derive(Debug, Clone)]
pub struct SelectionBookFilter {
    /// 入口授权后的客户集合。None 表示全量，空集合表示无权查看任何客户。
    pub authorized_customer_ids: Option<Vec<String>>,
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

impl QueryFilter for SelectionBookFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(ids) = &self.authorized_customer_ids {
            filter.insert("$and", vec![doc! { "customer_id": { "$in": ids } }]);
        }
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
            filter.insert("customer_name", doc! { "$regex": regex_escape(q), "$options": "i" });
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

/// 转义正则特殊字符，避免关键字被当作模式。
///
/// # 参数
/// * `value` - 原始关键字
///
/// # 返回
/// 返回可安全用于 `$regex` 的字面量。
///
/// # 错误
/// 无。
fn regex_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '.' | '+' | '*' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
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
        let sort_field = filter.sort_by.clone().unwrap_or_else(|| "created_at".to_string());
        let direction = if filter.sort_ascending { 1 } else { -1 };
        let options = FindOptions::builder()
            .sort(doc! { sort_field: direction, "id": direction })
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let items = mongo_ops::find_many(
            &self.db.collection::<SalesSelectionBooklet>(BOOKLETS),
            filter.to_doc(),
            options,
            executor,
        )
        .await?;
        let total = mongo_ops::count_documents(
            &self.db.collection::<SalesSelectionBooklet>(BOOKLETS),
            filter.to_doc(),
            executor,
        )
        .await?;
        Ok(PageResult { items, total: total as i64 })
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

    /// 按选品册读取会话。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 存在时返回会话。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn find_session_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesSelectionSession>> {
        self.db.sales_selection_sessions().find_one(doc! { "booklet_id": booklet_id }, executor).await
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
            mongo_ops::insert_many(
                &self.db.collection::<SalesSelectionPoolMember>(POOL),
                members.to_vec(),
                executor,
            )
            .await?;
        }
        if !items.is_empty() {
            mongo_ops::insert_many(
                &self.db.collection::<SalesSelectionDisplayItem>(ITEMS),
                items.to_vec(),
                executor,
            )
            .await?;
        }
        Ok(())
    }

    /// 原子提交：建方案、冻结会话、册置已提交。
    ///
    /// # 参数
    /// * `proposal` - 方案表头
    /// * `display_lines` - 陈列项行
    /// * `sku_lines` - SKU 行
    /// * `session` - 已冻结的会话
    /// * `booklet` - 已标记提交的选品册
    /// * `executor` - 执行器，必须位于事务中
    ///
    /// # 返回
    /// 成功写入全部事实。
    ///
    /// # 错误
    /// 任一步失败全部回滚，不允许残缺方案。
    pub async fn submit_proposal_bundle(
        &self,
        proposal: &SalesSelectionProposal,
        display_lines: &[SalesSelectionProposalDisplayLine],
        sku_lines: &[SalesSelectionProposalSkuLine],
        session: &mut SalesSelectionSession,
        booklet: &mut SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.sales_selection_proposals().create(proposal, executor).await?;
        self.insert_proposal_lines(display_lines, sku_lines, executor).await?;
        self.db.sales_selection_sessions().update(session, executor).await?;
        self.db.sales_selection_booklets().update(booklet, executor).await
    }

    /// 插入方案双层明细。
    ///
    /// # 参数
    /// * `display_lines` - 陈列项行
    /// * `sku_lines` - SKU 行
    /// * `executor` - 执行器，必须位于事务中
    ///
    /// # 返回
    /// 成功插入。
    ///
    /// # 错误
    /// 写入失败时返回仓储错误。
    async fn insert_proposal_lines(
        &self,
        display_lines: &[SalesSelectionProposalDisplayLine],
        sku_lines: &[SalesSelectionProposalSkuLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self.db.collection::<SalesSelectionProposalDisplayLine>(PROPOSAL_DISPLAY_LINES),
            display_lines.to_vec(),
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<SalesSelectionProposalSkuLine>(PROPOSAL_SKU_LINES),
            sku_lines.to_vec(),
            executor,
        )
        .await
    }

    /// 写入幂等记录。
    ///
    /// # 参数
    /// * `record` - 幂等记录
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 成功写入。
    ///
    /// # 错误
    /// 唯一冲突或写入失败时返回仓储错误。
    pub async fn create_idempotency(
        &self,
        record: &SalesSelectionIdempotency,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.sales_selection_idempotency().create(record, executor).await
    }

    /// 按操作域与作用域读取幂等记录。
    ///
    /// # 参数
    /// * `operation` - 操作代码
    /// * `scope_id` - 作用域
    /// * `key` - 幂等键
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 存在时返回记录。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn find_idempotency(
        &self,
        operation: &str,
        scope_id: &str,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesSelectionIdempotency>> {
        self.db
            .sales_selection_idempotency()
            .find_one(doc! { "operation": operation, "scope_id": scope_id, "idempotency_key": key }, executor)
            .await
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::{Pagination, QueryFilter};

    use super::{SelectionBookFilter, validate_book_sort};

    fn filter() -> SelectionBookFilter {
        SelectionBookFilter {
            authorized_customer_ids: None,
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
        assert!(document.contains_key("deleted_at"));
        assert_eq!(filter().page_and_size(), (1, 20));
    }

    #[test]
    fn customer_scope_intersects_explicit_filter_and_empty_scope_stays_empty() {
        let mut scoped = filter();
        scoped.authorized_customer_ids = Some(vec!["allowed".into()]);
        let doc = scoped.to_doc();
        assert_eq!(doc.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(
            doc.get_array("$and").unwrap()[0].as_document().unwrap(),
            &mongodb::bson::doc! { "customer_id": { "$in": ["allowed"] } }
        );
        scoped.authorized_customer_ids = Some(vec![]);
        assert_eq!(
            scoped.to_doc().get_array("$and").unwrap()[0].as_document().unwrap(),
            &mongodb::bson::doc! { "customer_id": { "$in": [] } }
        );
    }

    #[test]
    fn sort_whitelist_defaults_and_rejects_unknown() {
        assert_eq!(validate_book_sort(None, false).unwrap(), ("created_at".to_string(), false));
        assert_eq!(validate_book_sort(Some("status"), true).unwrap(), ("status".to_string(), true));
        assert!(validate_book_sort(Some("price"), false).is_err());
        assert!(validate_book_sort(Some("  "), false).unwrap().0 == "created_at");
    }
}
