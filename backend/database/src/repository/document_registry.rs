//! 域 D02 `document_registry` 仓储：business_document、document_relation、document_participant、workflow_action。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `extensions::DocumentRegistryExt`
//! 关联常量导入（conventions §4.3）。
//!
//! 筛选/行类型定义在本文件，经 `DocumentRegistryExt` 的关联类型对外暴露。

use entities::common::time::Instant;
use entities::document_registry::{
    BusinessDocument, BusinessDocumentId, DocumentParticipant, DocumentRelation, DocumentType,
    WorkflowAction, WorkflowActionType,
};
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::bpm::{assign_document_no_filter, classify_assign_document_no_miss, AssignDocumentNoOutcome};
use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

/// 单据注册列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessDocumentRow {
    /// 实体主键。
    pub id: String,
    /// 强类型业务表类型。
    pub document_type: DocumentType,
    /// 全局可查询业务编号。
    pub document_no: String,
    /// 首次正式化时间（秒级时间戳）。
    pub formalized_at: Option<u64>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 单据注册列表筛选条件。
#[derive(Debug, Clone)]
pub struct BusinessDocumentFilter {
    /// 强类型业务表类型；`None` 表示不筛选。
    pub document_type: Option<DocumentType>,
    /// 单据编号（忽略大小写字面量模糊匹配，支持全局搜索索引）；`None` 表示不筛选。
    pub document_no: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for BusinessDocumentFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(document_type) = self.document_type {
            filter.insert("document_type", document_type.as_str());
        }
        insert_literal_regex_filter(&mut filter, "document_no", self.document_no.as_deref());
        filter
    }
}

impl Pagination for BusinessDocumentFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, BusinessDocument> {
    /// 幂等注册业务单据。
    ///
    /// 跨域注册表入口（数据模型 §6.1）：空编号草稿可并存，但同一 `document_id`
    /// 始终最多一行，由 `uk_business_documents_id` 仲裁；非空
    /// `(document_type, document_no)` 由部分唯一索引 `uk_business_documents_identity`
    /// 承担并发仲裁。已存在同 ID 的注册视为幂等成功并返回已存在行；
    /// 同身份但 ID 不同的重复注册透出 [`crate::Error::DuplicateKey`]。
    ///
    /// 本方法采用「先插后查」：唯一索引保证并发下同 ID 最多一条注册行，不存在
    /// 读后写的竞态窗口，**不需要事务执行器**；传入 `NoTransaction` 时行为
    /// 可预期（单条写入自动提交）。非空身份字段全局唯一：注册行软删除后仍占用
    /// `(document_type, document_no)` 身份（与 accounts 处理一致），恢复语义
    /// 不被身份复用破坏。
    ///
    /// # 参数
    /// * `doc` - 待注册的单据（`document_no` 已由实体校验规范化）
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回 `Ok(None)` 表示本次写入新注册行；`Ok(Some(existing))` 表示同身份
    /// 同 ID 的幂等命中，返回已存在的注册行。
    ///
    /// # 错误
    /// 同身份不同 ID（含已软删除身份）写入时返回 [`crate::Error::DuplicateKey`]；
    /// 其他 MongoDB 写入或查询失败时返回错误。
    pub async fn register(
        &self,
        doc: &BusinessDocument,
        executor: &mut dyn Executor,
    ) -> Result<Option<BusinessDocument>> {
        match mongo_ops::insert_one(&self.collection(), doc, executor).await {
            Ok(()) => Ok(None),
            Err(Error::DuplicateKey(duplicate)) => {
                let existing = self.find_by_id(&doc.base.id, executor).await?;
                if same_id_registration(existing.as_ref(), &doc.base.id, |row| row.base.id.as_str()) {
                    Ok(existing)
                } else {
                    Err(Error::DuplicateKey(duplicate))
                }
            }
            Err(error) => Err(error),
        }
    }

    /// 注册已经通过实体无审批不变量校验的业务单据。
    ///
    /// 本方法提供给 `NO_APPROVAL` 创建路径使用；调用方必须先通过
    /// [`BusinessDocument::ensure_no_approval_registration`] 校验业务类型、注册行
    /// 预置绑定与统一绑定端口返回值，再调用本语义入口。
    ///
    /// # 参数
    /// * `doc` - 已校验为无审批注册的业务单据
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回值与 [`Self::register`] 相同。
    ///
    /// # 错误
    /// 唯一键冲突或 MongoDB 写入失败时返回错误。
    pub async fn register_no_approval_document(
        &self,
        doc: &BusinessDocument,
        executor: &mut dyn Executor,
    ) -> Result<Option<BusinessDocument>> {
        self.register(doc, executor).await
    }

    /// 以 `id + document_no 为空 + expected_version` 一次性赋值正式编号。
    ///
    /// 成功时同时写入 `document_no_assigned_at`，不得覆盖已有编号。同载荷回读
    /// 同一结果；不同编号竞争只允许一个成功。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 更新失败时返回错误。
    pub async fn assign_document_no(
        &self,
        id: &str,
        document_no: &str,
        expected_version: u64,
        assigned_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<AssignDocumentNoOutcome<BusinessDocument>> {
        let updated = mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            assign_document_no_filter(id, expected_version)?,
            assign_document_no_pipeline(document_no, assigned_at),
            executor,
        )
        .await?;
        if let Some(document) = updated {
            return Ok(AssignDocumentNoOutcome::Assigned(document));
        }
        let current = self.find_by_id(id, executor).await?;
        Ok(classify_assign_document_no_miss(
            current,
            expected_version,
            document_no,
            |row| row.base().version,
            |row| row.document_no.as_str(),
        ))
    }

    /// 分页检索单据注册列表（投影查询）。
    ///
    /// 只返回 [`BusinessDocumentRow`] 所需的列表字段，不加载整文档；
    /// `document_no` 按字面量忽略大小写模糊匹配（复用
    /// `repository::regex_filter`，禁止自拼正则）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_business_documents(
        &self,
        filter: &BusinessDocumentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BusinessDocumentRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(business_document_projection())
            .build();
        let collection = self.collection().clone_with_type::<BusinessDocumentRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, DocumentRelation> {
    /// 按源单据查询全部出向关系（变更、退货、退款、冲正、红票、派生）。
    ///
    /// # 参数
    /// * `from_document_id` - 源单据 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按创建时间升序排列的出向关系。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_from_document(
        &self,
        from_document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentRelation>> {
        self.find_many_sorted(
            doc! { "from_document_id": from_document_id.to_string() },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    /// 按目标单据查询全部入向关系（反向查询索引 `idx_document_relations_reverse`）。
    ///
    /// # 参数
    /// * `to_document_id` - 被引用或被纠正的原单 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按创建时间升序排列的入向关系。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_to_document(
        &self,
        to_document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentRelation>> {
        self.find_many_sorted(
            doc! { "to_document_id": to_document_id.to_string() },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, DocumentParticipant> {
    /// 按参与人查询其参与过的全部单据（“我的参与单据”）。
    ///
    /// # 参数
    /// * `user_id` - 参与人用户 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按参与时间倒序排列的参与记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_user(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentParticipant>> {
        self.find_many_sorted(
            doc! { "participant_user_id": user_id },
            doc! { "created_at": -1 },
            executor,
        )
        .await
    }
}

/// 工作流动作列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowActionRow {
    /// 实体主键。
    pub id: String,
    /// 业务单据 ID。
    pub document_id: String,
    /// 动作类型。
    pub action_type: WorkflowActionType,
    /// 迁移前状态代码。
    pub from_status: String,
    /// 迁移后状态代码。
    pub to_status: String,
    /// 实际操作者。
    pub actor_id: String,
    /// 动作发生时的责任角色。
    pub actor_role: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 工作流动作列表筛选条件。
#[derive(Debug, Clone)]
pub struct WorkflowActionFilter {
    /// 业务单据 ID；`None` 表示不筛选。
    pub document_id: Option<BusinessDocumentId>,
    /// 操作者（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub actor_id: Option<String>,
    /// 动作类型；`None` 表示不筛选。
    pub action_type: Option<WorkflowActionType>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for WorkflowActionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(document_id) = &self.document_id {
            filter.insert("document_id", document_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "actor_id", self.actor_id.as_deref());
        if let Some(action_type) = self.action_type {
            filter.insert("action_type", action_type.as_str());
        }
        filter
    }
}

impl Pagination for WorkflowActionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, WorkflowAction> {
    /// 分页检索工作流动作（投影查询）。
    ///
    /// 只返回 [`WorkflowActionRow`] 所需的列表字段，不加载整文档；
    /// `actor_id` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_workflow_actions(
        &self,
        filter: &WorkflowActionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WorkflowActionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(workflow_action_projection())
            .build();
        let collection = self.collection().clone_with_type::<WorkflowActionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按单据查询动作历史（`idx_workflow_actions_document_created`）。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按时间倒序排列的动作历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_document(
        &self,
        document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkflowAction>> {
        self.find_many_sorted(
            doc! { "document_id": document_id.to_string() },
            doc! { "created_at": -1 },
            executor,
        )
        .await
    }
}

/// 判断唯一键冲突是否为同一 `document_id` 的幂等回读。
///
/// 命中 `uk_business_documents_id` 且现有行 id 相同则回读；id 冲突或其它
/// 唯一键冲突（例如非空编号身份被另一行占用）不得视为同一注册。
///
/// # 参数
/// * `existing` - 按请求 `document_id` 回读到的现有行
/// * `expected_id` - 本次注册请求的稳定 `document_id`
/// * `id_of` - 从现有行取出 id 的函数
///
/// # 返回
/// 同 ID 时返回 `true`；无现有行或 id 不同时返回 `false`。
///
/// # 错误
/// 无。
fn same_id_registration<T>(existing: Option<&T>, expected_id: &str, id_of: impl FnOnce(&T) -> &str) -> bool {
    existing.is_some_and(|row| id_of(row) == expected_id)
}

/// 一次性编号赋值更新管道。
///
/// # 参数
/// * `document_no` - 要写入的正式编号
/// * `assigned_at` - 编号赋值时间
///
/// # 返回
/// 返回同时写入编号、赋值时间和版本递增的 `$set` 管道。
///
/// # 错误
/// 无。
fn assign_document_no_pipeline(document_no: &str, assigned_at: Instant) -> Vec<Document> {
    let assigned_at = assigned_at.unix_secs();
    vec![doc! {
        "$set": {
            "document_no": document_no,
            "document_no_assigned_at": assigned_at,
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": assigned_at,
        }
    }]
}

/// 构建排序文档（排序字段白名单化，禁止透传任意字段名）。
///
/// 仅允许 `created_at` / `updated_at`；未知字段回落默认 `created_at`。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或白名单外字段时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        _ => "created_at",
    };
    doc! { field: direction }
}

/// 单据注册列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn business_document_projection() -> Document {
    doc! {
        "id": 1,
        "document_type": 1,
        "document_no": 1,
        "formalized_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 工作流动作列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn workflow_action_projection() -> Document {
    doc! {
        "id": 1,
        "document_id": 1,
        "action_type": 1,
        "from_status": 1,
        "to_status": 1,
        "actor_id": 1,
        "actor_role": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        assign_document_no_pipeline, same_id_registration, sort_doc, BusinessDocumentFilter, QueryFilter,
        WorkflowActionFilter,
    };
    use crate::repository::bpm::{
        assign_document_no_filter, classify_assign_document_no_miss, AssignDocumentNoOutcome,
    };
    use entities::common::time::Instant;
    use entities::document_registry::{BusinessDocumentId, DocumentType, WorkflowActionType};
    use mongodb::bson::doc;

    #[test]
    fn business_document_filter_applies_type_and_no_regex() {
        let filter = BusinessDocumentFilter {
            document_type: Some(DocumentType::SalesOrder),
            document_no: Some("so-001".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("document_type").unwrap(), "sales_order");
        let no = document.get_document("document_no").unwrap();
        assert_eq!(no.get_str("$regex").unwrap(), r"so\-001");
        assert_eq!(no.get_str("$options").unwrap(), "i");
    }

    #[test]
    fn workflow_action_filter_applies_document_and_action_type() {
        let filter = WorkflowActionFilter {
            document_id: Some(BusinessDocumentId::new("order-1")),
            actor_id: None,
            action_type: Some(WorkflowActionType::Approve),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("document_id").unwrap(), "order-1");
        assert_eq!(document.get_str("action_type").unwrap(), "approve");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("created_at"), true), doc! { "created_at": 1 });
        assert_eq!(sort_doc(Some("updated_at"), false), doc! { "updated_at": -1 });
        assert_eq!(
            sort_doc(Some("document_no"), false),
            doc! { "created_at": -1 },
            "白名单外字段回落默认排序"
        );
    }

    #[test]
    fn empty_document_register_same_id_is_idempotent_reread() {
        assert!(same_id_registration(
            Some(&"bd-1".to_string()),
            "bd-1",
            String::as_str
        ));
        assert!(!same_id_registration(
            Some(&"bd-2".to_string()),
            "bd-1",
            String::as_str
        ));
        assert!(!same_id_registration::<String>(None, "bd-1", String::as_str));
    }

    #[test]
    fn assign_document_no_cas_allows_empty_drafts_and_rejects_overwrite() {
        let filter = assign_document_no_filter("bd-1", 2).unwrap();
        assert_eq!(filter.get_str("id").unwrap(), "bd-1");
        assert_eq!(filter.get_i64("version").unwrap(), 2);
        let alternatives = filter.get_array("$or").unwrap();
        assert_eq!(
            alternatives,
            &vec![
                mongodb::bson::Bson::Document(doc! { "document_no": "" }),
                mongodb::bson::Bson::Document(doc! { "document_no": mongodb::bson::Bson::Null }),
            ]
        );

        let pipeline = assign_document_no_pipeline("SO-1", Instant::from_unix_secs(99));
        let set = pipeline[0].get_document("$set").unwrap();
        assert_eq!(set.get_str("document_no").unwrap(), "SO-1");
        assert_eq!(set.get_i64("document_no_assigned_at").unwrap(), 99);
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((1_u64, "SO-1".to_string())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::SamePayload(_)
        ));
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((1_u64, "SO-2".to_string())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::NumberConflict(_)
        ));
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((2_u64, String::new())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::VersionConflict(_)
        ));
        assert!(matches!(
            classify_assign_document_no_miss(
                Some((1_u64, String::new())),
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
            AssignDocumentNoOutcome::VersionConflict(_)
        ));
    }
}
