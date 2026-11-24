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
use erp_permissions::{Permission, warehouse};
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
    /// 注入仓库数据库、共享授权和经办人、审计、指纹端口，供服务在同一事务校验并写入；敏感值指纹由组合层接入统一实现。
    pub fn new(
        db: Database,
        identity: Arc<dyn IdentityFactPort>,
        audit: Arc<dyn WarehouseAuditPort>,
        fingerprint: Arc<dyn AttachmentFingerprintPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, identity, audit, fingerprint, fingerprint_key: FINGERPRINT_KEY, authorization }
    }

    /// 在一个事务创建仓库稳定身份、首修订和审计，返回新仓库视图。地址和联系人封装为 `SensitiveText`，查询指纹须为带密钥 HMAC，不可用裸摘要。请求非法返回 `ValidationError`，代码唯一性冲突返回 `ConflictError`。
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

    /// 在一个事务追加资料修订、更新仓库稳定身份和审计，不改仓库代码。对象缺失返回 `NotFound`，期望版本不符返回 `ConflictError`。有库存或有效预占不得停用，但此处尚未接入库存域事实。
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

    /// 在事务内校验并更新入库与仓发经办人，配置只影响之后创建的履约任务，不改派已开放任务。仓库不存在、版本冲突、账号不可用或缺少对应权限时拒绝。
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

    /// 在查询快照中验证更新仓库资格，列出可担任入库或仓发经办人的管理端账号。
    pub async fn warehouse_fulfillment_handler_options(
        &self,
        actor: &AuditActor,
    ) -> Result<Vec<WarehouseFulfillmentHandlerOptionView>> {
        self.handler_options(actor, warehouse::UPDATE).await
    }

    /// 在查询快照中验证创建仓库资格，列出可担任入库或仓发经办人的管理端账号。
    pub async fn warehouse_create_handler_options(
        &self,
        actor: &AuditActor,
    ) -> Result<Vec<WarehouseFulfillmentHandlerOptionView>> {
        self.handler_options(actor, warehouse::CREATE).await
    }

    async fn handler_options(
        &self,
        actor: &AuditActor,
        permission: Permission,
    ) -> Result<Vec<WarehouseFulfillmentHandlerOptionView>> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.handler_options_snapshot(&actor, &permission, executor).await })
            })
            .await
    }

    /// 在一个授权快照内按仓库和名称分页读取修订，不返回地址、联系人等敏感字段。分页或排序参数非法时返回 `ValidationError`。
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

    // 用公开入口提供的同一快照检查修订读取资格和筛选，再查询列表投影及总数，避免先取业务记录。
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

    /// 在一个授权快照内按仓库、SKU 和状态读取预警策略分页。分页非法或排序字段超出白名单时返回 `ValidationError`。
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

    // 用原快照校验策略筛选并检查读取资格，再取分页和总数并转为视图。
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

    /// 在事务内按期望版本更新预警阈值或启停状态，仓库和 SKU 身份不变，成功后返回新策略视图。
    /// 策略缺失为 `NotFound`，版本不符为 `ConflictError`，启用区间重叠为 `BusinessLogicError`，请求非法为 `ValidationError`；负阈值、非法迁移、写入和审计错误沿用对应实体或下层分类。
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

    /// 在事务中按实际版本软删除策略并写审计；无匹配策略时返回 `NotFound`，CAS 并发冲突返回 `ConflictError`。
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

    // 用原执行器读取经办账号身份，验证其可以登录且具备指定入库或仓发权限，避免配置不可履约的人员。
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

// 让更新入口共用期望版本核对；不一致时返回保持 HTTP 约定文案的 `ConflictError`。
fn ensure_expected_version(current: u64, expected: u64) -> Result<()> {
    if current != expected {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
    }
    Ok(())
}

// 等待仓储分页投影后逐行转成视图，统一仓库、修订、策略列表的响应形状；查询失败时保留仓储错误，不生成空成功结果。
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
    // 将规范化仓库列表输入转成仓储条件，保留指定范围、经办筛选和 `Some` 白名单排序字段。
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
    // 将规范化修订的仓库、名称和分页转成仓储条件，排序保留已校验字段。
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
    // 将规范化的仓库、SKU、状态和分页转成策略仓储条件，保留 `Some` 白名单排序字段。
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
    // 将仓库列表行映射为响应，保留仓库身份及经办配置；仓储行的 `#[serde(default)]` 兼容规则不转移到视图。
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
    // 将修订列表投影转成不含地址、联系人等敏感字段的展示视图。
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
    // 由列表投影构造预警策略视图，保留定点阈值、启停与有效日期。
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

// 按职责核验经办账号事实，分别把缺失或停用、不可登录、缺少操作权限映射为 `BusinessLogicError`，提示重新选择，缺权限时也可先调整角色。
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

// 把公司经办候选转成界面选项，只保留可登录且具备至少一项履约资格的账号，再按显示名、账号 ID 稳定排序。
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

// 先用注入密钥为地址和联系人计算 HMAC 查询指纹，再验证敏感值、叶级地区、名称、变更原因与生效区间并构造修订。字段不合法直接返回。
// 当前敏感列以明文占位，未接入生产级加密；列表投影和 `Debug` 不输出该列。服务使用 `FINGERPRINT_KEY`，这里由调用方传入密钥。
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
