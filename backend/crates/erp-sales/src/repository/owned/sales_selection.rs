//! 销售选品拥有仓储。

use crate::entity::sales_selection::{
    SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionIdempotency, SalesSelectionPoolMember,
    SalesSelectionPrepareTask, SalesSelectionProposal, SalesSelectionProposalDisplayLine,
    SalesSelectionProposalSkuLine, SalesSelectionSession,
};

macro_rules! owned_repo {
    ($name:ident, $entity:ty, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<'a> {
            inner: persistence_core::Repository<'a, $entity>,
        }

        impl<'a> $name<'a> {
            /// 创建仓储。
            ///
            /// # 参数
            /// * `db` - 数据库
            /// * `collection_name` - 集合名
            ///
            /// # 返回
            /// 返回拥有仓储。
            ///
            /// # 错误
            /// 无。
            pub fn new(db: &'a mongodb::Database, collection_name: &'a str) -> Self {
                Self {
                    inner: persistence_core::Repository::new(db, collection_name),
                }
            }

            /// 插入实体。
            ///
            /// # 参数
            /// * `entity` - 实体
            /// * `executor` - 执行器
            ///
            /// # 返回
            /// 成功插入。
            ///
            /// # 错误
            /// 唯一键或写入失败。
            pub async fn create(
                &self,
                entity: &$entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()> {
                self.inner.create(entity, executor).await
            }

            /// 按身份读取未删除实体。
            ///
            /// # 参数
            /// * `id` - 身份
            /// * `executor` - 执行器
            ///
            /// # 返回
            /// 存在时返回实体。
            ///
            /// # 错误
            /// 查询失败。
            pub async fn find_by_id(
                &self,
                id: &str,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Option<$entity>> {
                self.inner.find_by_id(id, executor).await
            }

            /// 乐观锁更新。
            ///
            /// # 参数
            /// * `entity` - 实体
            /// * `executor` - 执行器
            ///
            /// # 返回
            /// 成功更新。
            ///
            /// # 错误
            /// 版本冲突或写入失败。
            pub async fn update(
                &self,
                entity: &mut $entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()>
            where
                $entity: entity_core::HasBaseModel,
            {
                self.inner.update(entity, executor).await
            }

            /// 按过滤条件读取未删除实体。
            ///
            /// # 参数
            /// * `filter` - 过滤
            /// * `executor` - 执行器
            ///
            /// # 返回
            /// 返回匹配实体。
            ///
            /// # 错误
            /// 查询失败。
            pub async fn find_many(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.find_many(filter, executor).await
            }

            /// 按过滤条件读取一条未删除实体。
            ///
            /// # 参数
            /// * `filter` - 过滤
            /// * `executor` - 执行器
            ///
            /// # 返回
            /// 存在时返回实体。
            ///
            /// # 错误
            /// 查询失败。
            pub async fn find_one(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Option<$entity>> {
                self.inner.find_one(filter, executor).await
            }
        }
    };
}

owned_repo!(
    SalesSelectionBookletRepository,
    SalesSelectionBooklet,
    "选品册仓储。"
);
owned_repo!(
    SalesSelectionDisplayItemRepository,
    SalesSelectionDisplayItem,
    "陈列项仓储。"
);
owned_repo!(
    SalesSelectionPoolMemberRepository,
    SalesSelectionPoolMember,
    "商品池成员仓储。"
);
owned_repo!(
    SalesSelectionPrepareTaskRepository,
    SalesSelectionPrepareTask,
    "准备任务仓储。"
);
owned_repo!(
    SalesSelectionSessionRepository,
    SalesSelectionSession,
    "选品会话仓储。"
);
owned_repo!(
    SalesSelectionProposalRepository,
    SalesSelectionProposal,
    "销售方案仓储。"
);
owned_repo!(
    SalesSelectionProposalDisplayLineRepository,
    SalesSelectionProposalDisplayLine,
    "方案陈列行仓储。"
);
owned_repo!(
    SalesSelectionProposalSkuLineRepository,
    SalesSelectionProposalSkuLine,
    "方案 SKU 行仓储。"
);
owned_repo!(
    SalesSelectionIdempotencyRepository,
    SalesSelectionIdempotency,
    "幂等记录仓储。"
);

impl SalesSelectionBookletRepository<'_> {
    /// 按令牌哈希查找选品册。
    ///
    /// # 参数
    /// * `token_hash` - 令牌哈希
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 存在时返回选品册。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn find_by_token_hash(
        &self,
        token_hash: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionBooklet>> {
        self.inner
            .find_one(mongodb::bson::doc! { "link_token_hash": token_hash }, executor)
            .await
    }

    /// 按客户、形态、状态、提交方式列表查询。
    ///
    /// # 参数
    /// * `customer_id` - 客户
    /// * `form` - 形态代码
    /// * `status` - 状态代码
    /// * `submit_mode` - 提交方式代码
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回匹配选品册。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn list_filtered(
        &self,
        customer_id: Option<&str>,
        form: Option<&str>,
        status: Option<&str>,
        submit_mode: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionBooklet>> {
        let mut filter = mongodb::bson::Document::new();
        if let Some(customer_id) = customer_id {
            filter.insert("customer_id", customer_id);
        }
        if let Some(form) = form {
            filter.insert("form", form);
        }
        if let Some(status) = status {
            filter.insert("status", status);
        }
        if let Some(submit_mode) = submit_mode {
            filter.insert("submit_mode", submit_mode);
        }
        self.inner.find_many(filter, executor).await
    }
}

impl SalesSelectionDisplayItemRepository<'_> {
    /// 读取本册历史陈列，供已经过客户权限检查的图片查询使用。
    /// # 参数
    /// booklet_id 为所属册，executor 为当前读取边界。
    /// # 返回
    /// 返回该册历史陈列；不包含其他册资产。
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn list_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionDisplayItem>> {
        self.inner
            .find_many(mongodb::bson::doc! { "booklet_id": booklet_id }, executor)
            .await
    }

    /// 读取一册一批次陈列。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回陈列项。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn list_by_batch(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionDisplayItem>> {
        self.inner
            .find_many(
                mongodb::bson::doc! { "booklet_id": booklet_id, "batch_id": batch_id },
                executor,
            )
            .await
    }
}

impl SalesSelectionPoolMemberRepository<'_> {
    /// 读取批次商品池。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回成员。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn list_by_batch(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPoolMember>> {
        self.inner
            .find_many(
                mongodb::bson::doc! { "booklet_id": booklet_id, "batch_id": batch_id },
                executor,
            )
            .await
    }
}

impl SalesSelectionPrepareTaskRepository<'_> {
    /// 按成功批次读取报告，未成功的任务不混入有效结果。
    /// # 参数
    /// booklet_id 和 batch_id 为所属册与结果批次，executor 为读取边界。
    /// # 返回
    /// 返回匹配任务，排序和逐档报告合并由服务完成。
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn list_by_result_batch(
        &self,
        booklet_id: &str,
        batch_id: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPrepareTask>> {
        self.inner
            .find_many(
                mongodb::bson::doc! { "booklet_id": booklet_id, "result_batch_id": batch_id },
                executor,
            )
            .await
    }

    /// 按身份读取任务。
    ///
    /// # 参数
    /// * `id` - 任务身份
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 存在时返回任务。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn find_task(
        &self,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionPrepareTask>> {
        self.inner.find_by_id(id, executor).await
    }

    /// 读取可领取或已到期的活动任务。
    ///
    /// # 参数
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回排队或运行中的任务。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn list_active(
        &self,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPrepareTask>> {
        self.inner
            .find_many(
                mongodb::bson::doc! { "status": { "$in": ["QUEUED", "RUNNING"] } },
                executor,
            )
            .await
    }
}

impl SalesSelectionSessionRepository<'_> {
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
    /// 查询失败。
    pub async fn find_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionSession>> {
        self.inner
            .find_one(mongodb::bson::doc! { "booklet_id": booklet_id }, executor)
            .await
    }
}

impl SalesSelectionProposalRepository<'_> {
    /// 客户授权与显式筛选取交集，禁止用客户筛选覆盖授权范围。
    /// # 参数
    /// authorized 为服务端解析的客户范围；空集合代表无任何客户权限。
    /// customer_id 与 booklet_id 为可选筛选，executor 为读取边界。
    /// # 返回
    /// 返回范围内的方案，服务层随后分页。
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn list_authorized(
        &self,
        customer_id: Option<&str>,
        booklet_id: Option<&str>,
        authorized: Option<&[String]>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposal>> {
        let mut filter = mongodb::bson::Document::new();
        if let Some(ids) = authorized {
            filter.insert(
                "$and",
                vec![mongodb::bson::doc! { "customer_id": { "$in": ids } }],
            );
        }
        if let Some(id) = customer_id {
            filter.insert("customer_id", id);
        }
        if let Some(id) = booklet_id {
            filter.insert("booklet_id", id);
        }
        self.inner.find_many(filter, executor).await
    }

    /// 按选品册读取方案。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 存在时返回方案。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn find_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionProposal>> {
        self.inner
            .find_one(mongodb::bson::doc! { "booklet_id": booklet_id }, executor)
            .await
    }

    /// 按客户列出方案。
    ///
    /// # 参数
    /// * `customer_id` - 客户
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回方案。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn list_by_customer(
        &self,
        customer_id: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposal>> {
        let mut filter = mongodb::bson::Document::new();
        if let Some(customer_id) = customer_id {
            filter.insert("customer_id", customer_id);
        }
        self.inner.find_many(filter, executor).await
    }
}

impl SalesSelectionProposalDisplayLineRepository<'_> {
    /// 读取方案陈列行。
    ///
    /// # 参数
    /// * `proposal_id` - 方案
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回陈列行。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn list_by_proposal(
        &self,
        proposal_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposalDisplayLine>> {
        self.inner
            .find_many(mongodb::bson::doc! { "proposal_id": proposal_id }, executor)
            .await
    }
}

impl SalesSelectionProposalSkuLineRepository<'_> {
    /// 读取方案 SKU 行。
    ///
    /// # 参数
    /// * `proposal_id` - 方案
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回 SKU 行。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn list_by_proposal(
        &self,
        proposal_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposalSkuLine>> {
        self.inner
            .find_many(mongodb::bson::doc! { "proposal_id": proposal_id }, executor)
            .await
    }
}

/// 公开限流窗口仓储。
pub struct SalesSelectionRateRepository<'a> {
    db: &'a mongodb::Database,
}

impl<'a> SalesSelectionRateRepository<'a> {
    /// 创建限流仓储。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回仓储。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a mongodb::Database) -> Self {
        Self { db }
    }

    /// 原子占用一分钟窗口配额。
    ///
    /// # 参数
    /// * `key` - 限流键
    /// * `limit` - 每分钟上限
    /// * `window_id` - 分钟窗口
    ///
    /// # 返回
    /// 未超限返回 `Ok(())`。
    ///
    /// # 错误
    /// 超限返回业务冲突由调用方映射；仓储失败返回存储错误。
    pub async fn admit(&self, key: &str, limit: i64, window_id: i64) -> persistence_core::Result<bool> {
        let id = format!("{key}:{window_id}");
        let collection = self.db.collection::<mongodb::bson::Document>(
            <mongodb::Database as crate::repository::SalesSelectionExt>::SALES_SELECTION_RATE_WINDOWS,
        );
        let result = collection
            .find_one_and_update(
                mongodb::bson::doc! { "_id": &id },
                mongodb::bson::doc! {
                    "$inc": { "count": 1_i64 },
                    "$setOnInsert": { "expires_at": mongodb::bson::DateTime::from_millis(
                        window_id.saturating_add(2).saturating_mul(60_000)) }
                },
            )
            .upsert(true)
            .return_document(mongodb::options::ReturnDocument::After)
            .await?;
        let count = result
            .as_ref()
            .and_then(|doc| doc.get_i64("count").ok())
            .unwrap_or(1);
        Ok(count <= limit)
    }
}

impl SalesSelectionIdempotencyRepository<'_> {
    /// 按操作、作用域和键读取幂等记录。
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
    /// 查询失败。
    pub async fn find_by_key(
        &self,
        operation: &str,
        scope_id: &str,
        key: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionIdempotency>> {
        self.inner
            .find_one(
                mongodb::bson::doc! {
                    "operation": operation,
                    "scope_id": scope_id,
                    "idempotency_key": key,
                },
                executor,
            )
            .await
    }
}
