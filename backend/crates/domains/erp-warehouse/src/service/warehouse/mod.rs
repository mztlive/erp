//! 域 D11 `warehouse` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 仓库创建与修订追加：跨集合（warehouses + warehouse_revisions + 审计）→
//!   `persistence_core::Transactional::with_transaction`，保证「仓库身份 + 当前修订指针 +
//!   修订快照」原子可见（数据模型 §6.3）；
//! - 仓库与预警策略的授权、决定写入的读取、写入和审计共用入口事务。
//!
//! 业务规则来自实体（`Warehouse::new`/`WarehouseRevision::new` 完成校验与
//! 规范化，`WarehouseSkuPolicy` 封装生效区间与重叠规则，`SensitiveText` 封装
//! 敏感列），Service 编排经办人资格、修订序号查询与事务写入。地址与联系人指纹经
//! `AttachmentFingerprintPort` 计算；跨域只经本 crate 端口，禁止 Service 依赖 Service。

mod commands;
mod reads;

use std::future::Future;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{WarehouseId, WarehouseRevisionId};
use erp_core::region::RegionCode;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::warehouse;
use mongodb::Database;
use persistence_core::{Executor, PageResult, Transactional};
use validator::Validate;

pub use crate::dto::warehouse::{
    CreateWarehouseRequest, CreateWarehouseSkuPolicyRequest, PageView,
    UpdateWarehouseFulfillmentHandlersRequest, UpdateWarehouseRequest, UpdateWarehouseSkuPolicyRequest,
    WarehouseFulfillmentHandlerOptionView, WarehouseListParams, WarehouseRevisionListParams,
    WarehouseRevisionView, WarehouseSkuPolicyListParams, WarehouseSkuPolicyView, WarehouseView,
};
use crate::dto::warehouse::{WarehouseListQuery, WarehouseRevisionListQuery, WarehouseSkuPolicyListQuery};
use crate::entity::warehouse::warehouse_revision::{SensitiveText, WarehouseRevision, WarehouseRevisionData};
use crate::error::{Error, Result};
use crate::ports::{
    AttachmentFingerprintPort, HandlerDuty, HandlerIdentityFact, IdentityFactPort, WarehouseAuditPort,
};
use crate::repository::prelude::*;
use crate::repository::{
    WarehouseExt, WarehouseFilter, WarehouseRevisionFilter, WarehouseRevisionRow, WarehouseRow,
    WarehouseSkuPolicyFilter, WarehouseSkuPolicyRow,
};

/// 敏感字段指纹密钥（HMAC-SHA256，带密钥禁止裸摘要）。
///
/// 地基修订候选：密钥应从 `config` 注入（services 层当前只持有 `Database`），
/// 此处使用固定占位密钥；指纹算法与实体形态已固化（数据模型 §4.5.5）。
const FINGERPRINT_KEY: &[u8] = b"erp-warehouse-sensitive-fingerprint-key-v1";
/// 仓库域服务。
///
/// 提供仓库稳定身份、仓库修订与仓库-SKU 预警策略的创建、查询、更新编排。
#[derive(Clone)]
pub struct WarehouseService {
    db: Database,
    identity: Arc<dyn IdentityFactPort>,
    audit: Arc<dyn WarehouseAuditPort>,
    fingerprint: Arc<dyn AttachmentFingerprintPort>,
    fingerprint_key: &'static [u8],
    authorization: Arc<AuthorizationService>,
}

impl WarehouseService {
    /// 创建仓库域服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `identity` - 经办人身份与权限事实端口
    /// * `audit` - 审计写入端口
    /// * `fingerprint` - 敏感字段指纹端口（委托 support 唯一实现）
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        identity: Arc<dyn IdentityFactPort>,
        audit: Arc<dyn WarehouseAuditPort>,
        fingerprint: Arc<dyn AttachmentFingerprintPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, identity, audit, fingerprint, fingerprint_key: FINGERPRINT_KEY, authorization }
    }

    /// 创建仓库（仓库稳定身份 + 首个修订，跨集合事务）。
    ///
    /// 地址与联系人生成带密钥 HMAC 指纹的 `SensitiveText` 后落库
    /// （数据模型 §4.5.5：数据库加密列 + 查询指纹，禁止裸摘要）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建仓库的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - warehouse_code 重复（唯一索引透出）
    pub async fn warehouse_create(
        &self,
        req: CreateWarehouseRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseView> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.create(&actor, req, executor).await })
            })
            .await
    }

    /// 更新仓库（追加新修订并更新稳定身份，跨集合事务）。
    ///
    /// `warehouse_code` 是稳定代码不可修改；「有库存或有效预占时不得停用」
    /// 需要库存域数据，当前未接线（见域报告「未实现且已知的缺口」）。
    ///
    /// # 参数
    /// * `id` - 仓库 ID
    /// * `req` - 更新请求（含期望版本与新修订快照）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后仓库的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 仓库不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn warehouse_update(
        &self,
        id: &str,
        req: UpdateWarehouseRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.update(&actor, &id, req, executor).await })
            })
            .await
    }

    /// 更新仓库入库与仓发经办人。
    ///
    /// # 参数
    /// * `id` - 仓库稳定 ID
    /// * `req` - 期望版本与两个具体经办人
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的仓库稳定视图。
    ///
    /// # 错误
    /// 仓库不存在、版本冲突、账号不可用或缺少对应履约权限时返回错误。
    ///
    /// # 关键业务约束
    /// 配置变更只用于之后新建的履约任务，不改派已开放任务。
    pub async fn warehouse_fulfillment_handlers_update(
        &self,
        id: &str,
        req: UpdateWarehouseFulfillmentHandlersRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.update_handlers(&actor, &id, req, executor).await })
            })
            .await
    }

    /// 列出仓库收发责任配置可选的具体账号。
    ///
    /// # 参数
    /// actor 为检查仓库配置资格的当前账号。
    ///
    /// # 返回
    /// 返回可登录管理账号及其入库、仓发权限资格。
    ///
    /// # 错误
    /// 当前账号缺少仓库配置资格时返回 `Forbidden`；授权、身份端口及事务错误按原分类传播。
    pub async fn warehouse_fulfillment_handler_options(
        &self,
        actor: &AuditActor,
    ) -> Result<Vec<WarehouseFulfillmentHandlerOptionView>> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.handler_options_snapshot(&actor, executor).await })
            })
            .await
    }

    /// 分页查询仓库修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`warehouse_id`/`name` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（不含加密地址/联系人等敏感字段）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn warehouse_revision_list(
        &self,
        actor: &AuditActor,
        params: &WarehouseRevisionListParams,
    ) -> Result<PageView<WarehouseRevisionView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.warehouse_revision_list_snapshot(&actor, &params, executor).await },
                )
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn warehouse_revision_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &WarehouseRevisionListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<WarehouseRevisionView>> {
        let filter = self.revision_filter(actor, params, executor).await?;
        map_search_page(
            self.db.warehouse_revisions().search_warehouse_revisions(&filter, executor),
            WarehouseRevisionView::from_row,
            filter.page,
            filter.page_size,
        )
        .await
    }

    /// 分页查询仓库-SKU 预警策略列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`warehouse_id`/`sku_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn warehouse_sku_policy_list(
        &self,
        actor: &AuditActor,
        params: &WarehouseSkuPolicyListParams,
    ) -> Result<PageView<WarehouseSkuPolicyView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.warehouse_sku_policy_list_snapshot(&actor, &params, executor).await
                })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn warehouse_sku_policy_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &WarehouseSkuPolicyListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<WarehouseSkuPolicyView>> {
        let filter = self.sku_policy_filter(actor, params, executor).await?;
        map_search_page(
            self.db.warehouse_sku_policies().search_warehouse_sku_policies(&filter, executor),
            WarehouseSkuPolicyView::from_row,
            filter.page,
            filter.page_size,
        )
        .await
    }

    /// 更新仓库-SKU 预警策略（乐观锁语义；`warehouse_id`/`sku_id` 是策略身份）。
    ///
    /// # 参数
    /// * `id` - 策略 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后策略的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 策略不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `BusinessLogicError` - 同一仓库和 SKU 的启用区间重叠
    ///
    /// 请求体校验失败时返回 `ValidationError`；预警阈值为负数或启停迁移被拒绝时返回实体错误；
    /// 写入或审计持久化失败时返回仓储或下层错误。
    pub async fn warehouse_sku_policy_update(
        &self,
        id: &str,
        req: UpdateWarehouseSkuPolicyRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseSkuPolicyView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.update_policy(&actor, &id, req, executor).await })
            })
            .await
    }

    /// 删除仓库-SKU 预警策略（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 策略 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 策略不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn warehouse_sku_policy_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.delete_policy(&actor, &id, executor).await })
            })
            .await
    }

    /// 校验仓储经办人是当前有效账号且具备对应正式操作权限。
    async fn ensure_handler_eligible(
        &self,
        account_id: &str,
        duty: HandlerDuty,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account_id = account_id.trim();
        let fact = self.identity.handler_identity(account_id, executor).await?;
        ensure_handler_fact_eligible(fact.as_ref(), duty)
    }
}

/// 校验乐观锁期望版本一致（三处更新守卫共用；冲突文案保持 HTTP 契约）。
///
/// # 参数
/// * `current` - 当前版本
/// * `expected` - 调用方期望版本
///
/// # 返回
/// 版本一致时返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回 `ConflictError`。
fn ensure_expected_version(current: u64, expected: u64) -> Result<()> {
    if current != expected {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
    }
    Ok(())
}

/// 执行投影分页查询并映射视图行（三类列表共用编排）。
///
/// 各列表方法经 crate 私有 `into_filter` / `from_row` 组装筛选与视图；查询语义与返回形状不变。
///
/// # 参数
/// * `search` - 投影分页查询 future
/// * `map_row` - 投影行转视图闭包
/// * `page` - 页码（1 起）
/// * `page_size` - 单页条数
///
/// # 返回
/// 返回契约形状的分页视图。
///
/// # 错误
/// 投影查询失败时返回仓储错误。
async fn map_search_page<Row, View>(
    search: impl Future<Output = persistence_core::Result<PageResult<Row>>>,
    mut map_row: impl FnMut(Row) -> View,
    page: u64,
    page_size: u32,
) -> Result<PageView<View>> {
    let page_result = search.await.map_err(Error::from)?;
    Ok(PageView {
        items: page_result.items.into_iter().map(&mut map_row).collect(),
        total: page_result.total,
        page,
        page_size,
    })
}

impl WarehouseListQuery {
    /// 转为仓储筛选；`sort_by` 保持 `Some(白名单字段)`。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与规范化查询字段一一对应的 [`WarehouseFilter`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn into_filter(self) -> WarehouseFilter {
        WarehouseFilter {
            warehouse_id: self.warehouse_id,
            require_inbound_handler: self.require_inbound_handler,
            q: self.q,
            warehouse_code: self.warehouse_code,
            status: self.status,
            page: self.paging.page,
            page_size: self.paging.page_size,
            sort_by: Some(self.paging.sort_field()),
            sort_ascending: self.paging.sort_ascending(),
        }
    }
}

impl WarehouseRevisionListQuery {
    /// 转为仓储筛选；`sort_by` 保持 `Some(白名单字段)`。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与规范化查询字段一一对应的 [`WarehouseRevisionFilter`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn into_filter(self) -> WarehouseRevisionFilter {
        WarehouseRevisionFilter {
            warehouse_id: self.warehouse_id,
            name: self.name,
            page: self.paging.page,
            page_size: self.paging.page_size,
            sort_by: Some(self.paging.sort_field()),
            sort_ascending: self.paging.sort_ascending(),
        }
    }
}

impl WarehouseSkuPolicyListQuery {
    /// 转为仓储筛选；`sort_by` 保持 `Some(白名单字段)`。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与规范化查询字段一一对应的 [`WarehouseSkuPolicyFilter`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn into_filter(self) -> WarehouseSkuPolicyFilter {
        WarehouseSkuPolicyFilter {
            warehouse_id: self.warehouse_id,
            sku_id: self.sku_id,
            status: self.status,
            page: self.paging.page,
            page_size: self.paging.page_size,
            sort_by: Some(self.paging.sort_field()),
            sort_ascending: self.paging.sort_ascending(),
        }
    }
}

impl WarehouseView {
    /// 由列表投影行构造响应视图（不搬迁行上的 `#[serde(default)]`）。
    ///
    /// # 参数
    /// * `row` - 仓库列表投影行。
    ///
    /// # 返回
    /// 返回契约形状的仓库视图。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn from_row(row: WarehouseRow) -> Self {
        Self {
            id: row.id,
            warehouse_code: row.warehouse_code,
            status: row.status,
            inbound_handler_user_id: row.inbound_handler_user_id,
            outbound_handler_user_id: row.outbound_handler_user_id,
            created_at: row.created_at,
            version: row.version,
        }
    }
}

impl WarehouseRevisionView {
    /// 由列表投影行构造响应视图。
    ///
    /// # 参数
    /// * `row` - 仓库修订列表投影行。
    ///
    /// # 返回
    /// 返回不含敏感字段的修订视图。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn from_row(row: WarehouseRevisionRow) -> Self {
        Self {
            id: row.id,
            warehouse_id: row.warehouse_id,
            revision_no: row.revision_no,
            name: row.name,
            region_name: row.region_code.as_ref().map(|code| code.full_name()),
            region_code: row.region_code.as_ref().map(ToString::to_string),
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            change_reason: row.change_reason,
            created_at: row.created_at,
            version: row.version,
        }
    }
}

impl WarehouseSkuPolicyView {
    /// 由列表投影行构造响应视图。
    ///
    /// # 参数
    /// * `row` - 预警策略列表投影行。
    ///
    /// # 返回
    /// 返回策略列表视图。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn from_row(row: WarehouseSkuPolicyRow) -> Self {
        Self {
            id: row.id,
            warehouse_id: row.warehouse_id,
            sku_id: row.sku_id,
            minimum_available_quantity: row.minimum_available_quantity,
            status: row.status,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            created_at: row.created_at,
            version: row.version,
        }
    }
}

/// 校验身份事实是否满足仓库经办人资格，并保留原错误文案。
///
/// # 参数
/// * `fact` - 身份端口返回的事实；`None` 表示账号不存在或已停用。
/// * `duty` - 入库或仓发责任。
///
/// # 返回
/// 账号可登录且具备对应资格时返回 `Ok(())`。
///
/// # 错误
/// `fact` 为 `None` 时返回 `BusinessLogicError`，文案为「{入库|仓发}经办人账号不存在或已停用，请重新选择」；
/// 不可登录时文案为「{入库|仓发}经办人账号不可用，请重新选择」；
/// 缺少对应资格时文案为「{入库|仓发}经办人缺少对应操作权限，请先调整角色或重新选择」。
pub(crate) fn ensure_handler_fact_eligible(
    fact: Option<&HandlerIdentityFact>,
    duty: HandlerDuty,
) -> Result<()> {
    let label = duty.label();
    let Some(fact) = fact else {
        return Err(Error::BusinessLogicError(format!("{label}经办人账号不存在或已停用，请重新选择")));
    };
    if !fact.can_login {
        return Err(Error::BusinessLogicError(format!("{label}经办人账号不可用，请重新选择")));
    }
    if duty.is_eligible(fact) {
        return Ok(());
    }
    Err(Error::BusinessLogicError(format!("{label}经办人缺少对应操作权限，请先调整角色或重新选择")))
}

/// 把身份事实投影为经办人选项。
///
/// 只保留可登录且具备入库或仓发至少一项资格的账号，再按显示名、账号 ID 排序。
///
/// # 参数
/// * `facts` - 全公司经办人身份事实。
///
/// # 返回
/// 返回经办人选项；不可登录，或两项资格都没有的账号不会出现。
///
/// # 错误
/// 不返回错误。
pub(crate) fn handler_option_views(
    mut facts: Vec<HandlerIdentityFact>,
) -> Vec<WarehouseFulfillmentHandlerOptionView> {
    facts.retain(|fact| fact.can_login && (fact.inbound_eligible || fact.outbound_eligible));
    facts.sort_by(|left, right| {
        left.display_name.cmp(&right.display_name).then_with(|| left.user_id.cmp(&right.user_id))
    });

    facts
        .into_iter()
        .map(|fact| WarehouseFulfillmentHandlerOptionView {
            user_id: fact.user_id,
            display_name: fact.display_name,
            account: fact.account,
            inbound_eligible: fact.inbound_eligible,
            outbound_eligible: fact.outbound_eligible,
        })
        .collect()
}

/// 仓库修订构建输入（名称/地址/联系人/生效区间/变更原因）。
struct WarehouseRevisionInput {
    /// 仓库名称。
    name: String,
    /// 所在地区代码。
    region_code: String,
    /// 地址明文。
    address: String,
    /// 联系人明文。
    contact: String,
    /// 生效起始日。
    effective_from: BusinessDate,
    /// 生效截止日。
    effective_to: Option<BusinessDate>,
    /// 变更原因。
    change_reason: String,
}

/// 构造仓库修订（名称/变更原因校验 + 地址/联系人敏感值指纹化）。
///
/// 地址与联系人按数据模型 §4.5.5 生成带密钥 HMAC 指纹的 `SensitiveText`；
/// 密文列当前无生产级加密原语（见域报告「地基修订候选」），P3 落库为
/// 明文占位 + 真 HMAC 查询指纹，列表投影与 Debug 均不暴露。
///
/// # 参数
/// * `warehouse_id` - 所属仓库
/// * `revision_id` - 修订 ID
/// * `revision_no` - 修订序号
/// * `input` - 修订内容
/// * `fingerprint` - 敏感字段指纹端口
/// * `fingerprint_key` - 指纹密钥（默认 `FINGERPRINT_KEY`，经服务构造参数注入）
///
/// # 返回
/// 返回仓库修订实体。
///
/// # 错误
/// 字段校验失败时返回错误。
fn build_warehouse_revision(
    warehouse_id: WarehouseId,
    revision_id: WarehouseRevisionId,
    revision_no: u32,
    input: WarehouseRevisionInput,
    fingerprint: &dyn AttachmentFingerprintPort,
    fingerprint_key: &[u8],
) -> Result<WarehouseRevision> {
    let address_fingerprint = fingerprint.content_fingerprint(&input.address, fingerprint_key);
    let address = SensitiveText::new(input.address, address_fingerprint)?;
    let contact_fingerprint = fingerprint.content_fingerprint(&input.contact, fingerprint_key);
    let contact = SensitiveText::new(input.contact, contact_fingerprint)?;

    let region_code = RegionCode::parse_leaf(&input.region_code)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(WarehouseRevision::new(
        revision_id,
        WarehouseRevisionData {
            warehouse_id,
            revision_no,
            name: input.name,
            region_code,
            address,
            contact,
            effective_from: input.effective_from,
            effective_to: input.effective_to,
            change_reason: input.change_reason,
        },
    )?)
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WarehouseId;

    use super::{
        HandlerDuty, WarehouseListParams, WarehouseRevisionListParams, WarehouseSkuPolicyListParams,
        WarehouseView, ensure_handler_fact_eligible, handler_option_views,
    };
    use crate::entity::warehouse::EnableStatus;
    use crate::entity::warehouse::warehouse_entity::{
        Warehouse, WarehouseData, WarehouseFulfillmentOperation, WarehouseUpdate,
    };
    use crate::error::Error;
    use crate::ports::HandlerIdentityFact;

    fn fact(
        user_id: &str,
        display_name: &str,
        can_login: bool,
        inbound: bool,
        outbound: bool,
    ) -> HandlerIdentityFact {
        HandlerIdentityFact {
            user_id: user_id.to_string(),
            display_name: display_name.to_string(),
            account: format!("{user_id}-login"),
            can_login,
            inbound_eligible: inbound,
            outbound_eligible: outbound,
        }
    }

    #[test]
    fn handler_eligibility_rejects_missing_disabled_and_unauthorized() {
        let missing = ensure_handler_fact_eligible(None, HandlerDuty::Inbound);
        assert!(
            matches!(missing, Err(Error::BusinessLogicError(message)) if message == "入库经办人账号不存在或已停用，请重新选择")
        );

        let disabled = fact("u-1", "张三", false, true, true);
        let disabled_err = ensure_handler_fact_eligible(Some(&disabled), HandlerDuty::Inbound);
        assert!(
            matches!(disabled_err, Err(Error::BusinessLogicError(message)) if message == "入库经办人账号不可用，请重新选择")
        );

        let unauthorized = fact("u-1", "张三", true, false, true);
        let inbound_err = ensure_handler_fact_eligible(Some(&unauthorized), HandlerDuty::Inbound);
        assert!(
            matches!(inbound_err, Err(Error::BusinessLogicError(message)) if message == "入库经办人缺少对应操作权限，请先调整角色或重新选择")
        );

        let outbound_ok = ensure_handler_fact_eligible(Some(&unauthorized), HandlerDuty::Outbound);
        assert!(outbound_ok.is_ok());
    }

    #[test]
    fn handler_options_keep_company_scope_and_stable_sort() {
        let options = handler_option_views(vec![
            fact("u-org-b", "李四", true, true, false),
            fact("u-org-a", "李四", true, false, true),
            fact("u-disabled", "王五", false, true, true),
            fact("u-none", "赵六", true, false, false),
        ]);

        assert_eq!(
            options.iter().map(|item| item.user_id.as_str()).collect::<Vec<_>>(),
            vec!["u-org-a", "u-org-b"]
        );
        assert!(options.iter().all(|item| item.inbound_eligible || item.outbound_eligible));
        assert!(
            options.iter().all(|item| item.user_id.starts_with("u-org-")),
            "选项保持公司范围，不按组织裁剪"
        );
    }

    #[test]
    fn disabled_warehouse_fail_closes_fulfillment_handler_and_version_conflicts() {
        let mut warehouse = Warehouse::new(
            WarehouseId::new("wh-1"),
            WarehouseData::new("WH-1")
                .with_status(EnableStatus::Active)
                .with_inbound_handler_user_id("inbound-1")
                .with_outbound_handler_user_id("outbound-1"),
            "admin-1",
        )
        .unwrap();
        warehouse.base.version = 3;
        assert!(warehouse.matches_version(3));
        assert!(!warehouse.matches_version(2));

        warehouse
            .update(
                WarehouseUpdate {
                    status: Some(EnableStatus::Disabled),
                    inbound_handler_user_id: None,
                    outbound_handler_user_id: None,
                },
                "admin-2",
            )
            .unwrap();
        let disabled = warehouse
            .fulfillment_handler(WarehouseFulfillmentOperation::Receipt)
            .expect_err("停用仓库必须失败关闭");
        assert_eq!(disabled.to_string(), "目标仓库已停用，请先更换仓库");
    }

    #[test]
    fn version_conflict_message_matches_http_contract() {
        let error = Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string());
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }

    #[test]
    fn list_queries_into_filter_keep_sort_by_some() {
        let warehouse: WarehouseListParams = serde_json::from_value(serde_json::json!({
            "warehouse_id": "wh-1",
            "require_inbound_handler": true,
            "q": " 北京 ",
            "warehouse_code": " WH-1 ",
            "status": "active",
            "page": 2,
            "page_size": 50,
            "sort_by": "warehouse_code",
            "sort_dir": "asc",
        }))
        .unwrap();
        let warehouse_filter = warehouse.normalized().unwrap().into_filter();
        assert_eq!(warehouse_filter.warehouse_id.as_deref(), Some("wh-1"));
        assert!(warehouse_filter.require_inbound_handler);
        assert_eq!(warehouse_filter.q.as_deref(), Some("北京"));
        assert_eq!(warehouse_filter.warehouse_code.as_deref(), Some("WH-1"));
        assert_eq!(warehouse_filter.status, Some(EnableStatus::Active));
        assert_eq!(warehouse_filter.page, 2);
        assert_eq!(warehouse_filter.page_size, 50);
        assert_eq!(warehouse_filter.sort_by.as_deref(), Some("warehouse_code"));
        assert!(warehouse_filter.sort_ascending);

        let default_warehouse: WarehouseListParams = serde_json::from_value(serde_json::json!({})).unwrap();
        let default_filter = default_warehouse.normalized().unwrap().into_filter();
        assert_eq!(default_filter.sort_by.as_deref(), Some("created_at"));
        assert!(!default_filter.sort_ascending);
        assert_eq!((default_filter.page, default_filter.page_size), (1, 20));

        let revision: WarehouseRevisionListParams = serde_json::from_value(serde_json::json!({
            "warehouse_id": "wh-1",
            "name": " 北京 ",
            "sort_by": "revision_no",
            "sort_dir": "asc",
        }))
        .unwrap();
        let revision_filter = revision.normalized().unwrap().into_filter();
        assert_eq!(revision_filter.warehouse_id.as_deref(), Some("wh-1"));
        assert_eq!(revision_filter.name.as_deref(), Some("北京"));
        assert_eq!(revision_filter.sort_by.as_deref(), Some("revision_no"));
        assert!(revision_filter.sort_ascending);

        let policy: WarehouseSkuPolicyListParams = serde_json::from_value(serde_json::json!({
            "status": "disabled",
            "page": 3,
            "page_size": 10,
        }))
        .unwrap();
        let policy_filter = policy.normalized().unwrap().into_filter();
        assert_eq!(policy_filter.status, Some(EnableStatus::Disabled));
        assert_eq!(policy_filter.sort_by.as_deref(), Some("created_at"));
        assert_eq!((policy_filter.page, policy_filter.page_size), (3, 10));
        assert!(!policy_filter.sort_ascending);
    }

    #[test]
    fn warehouse_view_from_row_keeps_optional_handlers() {
        use crate::repository::WarehouseRow;

        let view = WarehouseView::from_row(WarehouseRow {
            id: "wh-1".into(),
            warehouse_code: "WH-1".into(),
            status: EnableStatus::Active,
            inbound_handler_user_id: None,
            outbound_handler_user_id: Some("u-1".into()),
            version: 3,
            created_at: 1,
        });
        assert_eq!(view.id, "wh-1");
        assert_eq!(view.warehouse_code, "WH-1");
        assert_eq!(view.status, EnableStatus::Active);
        assert!(view.inbound_handler_user_id.is_none());
        assert_eq!(view.outbound_handler_user_id.as_deref(), Some("u-1"));
        assert_eq!(view.created_at, 1);
        assert_eq!(view.version, 3);
    }

    #[test]
    fn revision_fingerprint_uses_untrimmed_request_plaintext() {
        use erp_core::common::time::BusinessDate;
        use erp_core::ids::{WarehouseId, WarehouseRevisionId};
        use hmac::{Hmac, KeyInit, Mac};
        use sha2::Sha256;

        use super::{FINGERPRINT_KEY, WarehouseRevisionInput, build_warehouse_revision};
        use crate::ports::AttachmentFingerprintPort;

        struct SupportFingerprint;
        impl AttachmentFingerprintPort for SupportFingerprint {
            fn content_fingerprint(&self, plain: &str, key: &[u8]) -> String {
                let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC 接受任意长度密钥");
                mac.update(plain.as_bytes());
                mac.finalize().into_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
            }
        }

        let address = "  北京市朝阳区望京街道 1 号  ".to_string();
        let contact = " 张三 ".to_string();
        let revision = build_warehouse_revision(
            WarehouseId::new("wh-1"),
            WarehouseRevisionId::new("rev-1"),
            1,
            WarehouseRevisionInput {
                name: "北京仓".to_string(),
                region_code: "110105".to_string(),
                address: address.clone(),
                contact: contact.clone(),
                effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
                change_reason: "期初建仓".to_string(),
            },
            &SupportFingerprint,
            FINGERPRINT_KEY,
        )
        .unwrap();

        assert_eq!(
            revision.address.fingerprint_value(),
            SupportFingerprint.content_fingerprint(&address, FINGERPRINT_KEY)
        );
        assert_eq!(
            revision.contact.fingerprint_value(),
            SupportFingerprint.content_fingerprint(&contact, FINGERPRINT_KEY)
        );
        assert_ne!(
            revision.address.fingerprint_value(),
            SupportFingerprint.content_fingerprint(address.trim(), FINGERPRINT_KEY),
            "生产指纹不 trim，与实体测试内 trim 指纹区分"
        );
    }
}
