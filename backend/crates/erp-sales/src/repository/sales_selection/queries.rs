//! 选品集合仓储的域查询扩展。

use persistence_core::Repository;

use crate::entity::sales_selection::{
    SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionIdempotency, SalesSelectionPoolMember,
    SalesSelectionPrepareTask, SalesSelectionProposal, SalesSelectionProposalDisplayLine,
    SalesSelectionProposalSkuLine, SalesSelectionSession,
};

/// 选品册集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionBookletRepositoryExt {
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
    async fn find_by_token_hash(
        &self,
        token_hash: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionBooklet>>;

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
    async fn list_filtered(
        &self,
        customer_id: Option<&str>,
        form: Option<&str>,
        status: Option<&str>,
        submit_mode: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionBooklet>>;
}

impl SalesSelectionBookletRepositoryExt for Repository<'_, SalesSelectionBooklet> {
    async fn find_by_token_hash(
        &self,
        token_hash: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionBooklet>> {
        self.find_one(mongodb::bson::doc! { "link_token_hash": token_hash }, executor).await
    }

    async fn list_filtered(
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
        self.find_many(filter, executor).await
    }
}

/// 陈列项集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionDisplayItemRepositoryExt {
    /// 读取本册历史陈列，供已经过客户权限检查的图片查询使用。
    /// # 参数
    /// booklet_id 为所属册，executor 为当前读取边界。
    /// # 返回
    /// 返回该册历史陈列；不包含其他册资产。
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn list_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionDisplayItem>>;

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
    async fn list_by_batch(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionDisplayItem>>;
}

impl SalesSelectionDisplayItemRepositoryExt for Repository<'_, SalesSelectionDisplayItem> {
    async fn list_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionDisplayItem>> {
        self.find_many(mongodb::bson::doc! { "booklet_id": booklet_id }, executor).await
    }

    async fn list_by_batch(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionDisplayItem>> {
        self.find_many(mongodb::bson::doc! { "booklet_id": booklet_id, "batch_id": batch_id }, executor).await
    }
}

/// 商品池成员集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionPoolMemberRepositoryExt {
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
    async fn list_by_batch(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPoolMember>>;
}

impl SalesSelectionPoolMemberRepositoryExt for Repository<'_, SalesSelectionPoolMember> {
    async fn list_by_batch(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPoolMember>> {
        self.find_many(mongodb::bson::doc! { "booklet_id": booklet_id, "batch_id": batch_id }, executor).await
    }
}

/// 准备任务集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionPrepareTaskRepositoryExt {
    /// 按成功批次读取报告，未成功的任务不混入有效结果。
    /// # 参数
    /// booklet_id 和 batch_id 为所属册与结果批次，executor 为读取边界。
    /// # 返回
    /// 返回匹配任务，排序和逐档报告合并由服务完成。
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn list_by_result_batch(
        &self,
        booklet_id: &str,
        batch_id: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPrepareTask>>;

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
    async fn find_task(
        &self,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionPrepareTask>>;

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
    async fn list_active(
        &self,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPrepareTask>>;
}

impl SalesSelectionPrepareTaskRepositoryExt for Repository<'_, SalesSelectionPrepareTask> {
    async fn list_by_result_batch(
        &self,
        booklet_id: &str,
        batch_id: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPrepareTask>> {
        self.find_many(
            mongodb::bson::doc! { "booklet_id": booklet_id, "result_batch_id": batch_id },
            executor,
        )
        .await
    }

    async fn find_task(
        &self,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionPrepareTask>> {
        self.find_by_id(id, executor).await
    }

    async fn list_active(
        &self,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionPrepareTask>> {
        self.find_many(mongodb::bson::doc! { "status": { "$in": ["QUEUED", "RUNNING"] } }, executor).await
    }
}

/// 选品会话集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionSessionRepositoryExt {
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
    async fn find_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionSession>>;
}

impl SalesSelectionSessionRepositoryExt for Repository<'_, SalesSelectionSession> {
    async fn find_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionSession>> {
        self.find_one(mongodb::bson::doc! { "booklet_id": booklet_id }, executor).await
    }
}

/// 销售方案集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionProposalRepositoryExt {
    /// 客户授权与显式筛选取交集，禁止用客户筛选覆盖授权范围。
    /// # 参数
    /// authorized 为服务端解析的客户范围；空集合代表无任何客户权限。
    /// customer_id 与 booklet_id 为可选筛选，executor 为读取边界。
    /// # 返回
    /// 返回范围内的方案，服务层随后分页。
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn list_authorized(
        &self,
        customer_id: Option<&str>,
        booklet_id: Option<&str>,
        authorized: Option<&[String]>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposal>>;

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
    async fn find_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionProposal>>;

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
    async fn list_by_customer(
        &self,
        customer_id: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposal>>;
}

impl SalesSelectionProposalRepositoryExt for Repository<'_, SalesSelectionProposal> {
    async fn list_authorized(
        &self,
        customer_id: Option<&str>,
        booklet_id: Option<&str>,
        authorized: Option<&[String]>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposal>> {
        let mut filter = mongodb::bson::Document::new();
        if let Some(ids) = authorized {
            filter.insert("$and", vec![mongodb::bson::doc! { "customer_id": { "$in": ids } }]);
        }
        if let Some(id) = customer_id {
            filter.insert("customer_id", id);
        }
        if let Some(id) = booklet_id {
            filter.insert("booklet_id", id);
        }
        self.find_many(filter, executor).await
    }

    async fn find_by_booklet(
        &self,
        booklet_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionProposal>> {
        self.find_one(mongodb::bson::doc! { "booklet_id": booklet_id }, executor).await
    }

    async fn list_by_customer(
        &self,
        customer_id: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposal>> {
        let mut filter = mongodb::bson::Document::new();
        if let Some(customer_id) = customer_id {
            filter.insert("customer_id", customer_id);
        }
        self.find_many(filter, executor).await
    }
}

/// 方案陈列行集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionProposalDisplayLineRepositoryExt {
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
    async fn list_by_proposal(
        &self,
        proposal_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposalDisplayLine>>;
}

impl SalesSelectionProposalDisplayLineRepositoryExt for Repository<'_, SalesSelectionProposalDisplayLine> {
    async fn list_by_proposal(
        &self,
        proposal_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposalDisplayLine>> {
        self.find_many(mongodb::bson::doc! { "proposal_id": proposal_id }, executor).await
    }
}

/// 方案 SKU 行集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionProposalSkuLineRepositoryExt {
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
    async fn list_by_proposal(
        &self,
        proposal_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposalSkuLine>>;
}

impl SalesSelectionProposalSkuLineRepositoryExt for Repository<'_, SalesSelectionProposalSkuLine> {
    async fn list_by_proposal(
        &self,
        proposal_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<SalesSelectionProposalSkuLine>> {
        self.find_many(mongodb::bson::doc! { "proposal_id": proposal_id }, executor).await
    }
}

/// 选品幂等记录集合的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait SalesSelectionIdempotencyRepositoryExt {
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
    async fn find_by_key(
        &self,
        operation: &str,
        scope_id: &str,
        key: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionIdempotency>>;
}

impl SalesSelectionIdempotencyRepositoryExt for Repository<'_, SalesSelectionIdempotency> {
    async fn find_by_key(
        &self,
        operation: &str,
        scope_id: &str,
        key: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<SalesSelectionIdempotency>> {
        self.find_one(
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
