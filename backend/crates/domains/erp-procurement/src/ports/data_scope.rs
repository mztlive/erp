//! 采购范围授权 Port：只接收固定入口，并复用原执行器和共享授权凭证。

mod shared;
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::{AuthorizationService, CompiledAccess};
use erp_permissions::Permission;
use persistence_core::Executor;
pub use shared::PurchaseDataScope;

use crate::entity::purchase_order::PurchaseOrder;
use crate::error::{Error, Result};

/// 当前采购查询阶段凭证与分页元数据；不暴露权限规则。
#[derive(Clone)]
pub struct PurchaseResolvedScope {
    pub compiled: Arc<CompiledAccess>,
    pub authorization: Arc<AuthorizationService>,
    pub policy_version: u64,
    pub scope_version: String,
    pub as_of: Instant,
}

/// 采购域消费的窄授权 Port；adapter 调用身份域公共解析器。
#[async_trait]
pub trait PurchaseDataScopePort: Send + Sync {
    /// 按实际业务入口检查完整采购来源集合。
    /// # 参数
    /// actor、entry 为当前固定入口，ids 为完整来源，executor 为原事务。
    /// # 返回
    /// 全部来源可读时成功。
    /// # 错误
    /// 未接线或来源无权时拒绝。
    async fn require_source(
        &self,
        _actor: &AuditActor,
        _entry: &Permission,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(unwired())
    }

    /// 在原事务内以固定内部入口检查持久化采购身份。
    /// # 参数
    /// actor、entry 为真实身份与中央固定入口，id 为采购身份，executor 为原事务。
    /// # 返回
    /// 统一服务的完整记录授权成立时成功；调用方随后读取同快照对象。
    /// # 错误
    /// 未接线、未登记入口、功能或对象无权时拒绝。
    async fn require_record_entry(
        &self,
        _actor: &AuditActor,
        _entry: &Permission,
        _id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(unwired())
    }

    /// 按固定内部入口检查待创建采购候选。
    /// # 参数
    /// actor 为已认证身份，entry 为中央登记入口，order 为已校验候选事实，executor 为原事务。
    /// # 返回
    /// 入口的完整固定条件成立时成功。
    /// # 错误
    /// 未接线、入口不匹配或对象无权时拒绝。
    async fn require_entry(
        &self,
        _actor: &AuditActor,
        _entry: &Permission,
        _order: &PurchaseOrder,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(unwired())
    }

    /// 在调用方事务内证明采购资源动作并返回已解析事实。
    ///
    /// # 参数
    /// * `actor` - 服务端已认证身份
    /// * `entry` - 中央登记的固定采购入口
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已通过资格检查的范围事实；全量、空集、缺少个人上限可区分。
    ///
    /// # 错误
    /// 未装配、未注册、无动作权限或版本变化必须明确失败。
    ///
    /// # 关键业务约束
    /// 资源由本 Port 固定为采购单；不得把原始 `scope_type / scope_targets` 交给采购域再解析。
    async fn resolve(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseResolvedScope>;

    /// 按中央登记的固定查询入口返回采购查询凭证。
    /// # 参数
    /// actor 为已认证身份，entry 为固定权限常量，executor 为原读取执行器。
    /// # 返回
    /// 通过完整固定资格及范围条件的查询凭证。
    /// # 错误
    /// 未接线、入口未登记或无权时拒绝。
    async fn resolve_entry(
        &self,
        _actor: &AuditActor,
        _entry: &Permission,
        _executor: &mut dyn Executor,
    ) -> Result<PurchaseResolvedScope> {
        Err(unwired())
    }

    /// 展开请求中的组织及其可选下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织拒绝，不得忽略后查询全部组织。
    ///
    /// # 关键业务约束
    /// 筛选只能收窄授权结果；不得把完整组织树交给采购域自行重建。
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>>;

    /// 读取指定内部组织在给定时点的有效主属成员。
    ///
    /// # 参数
    /// * `org_unit_ids` - 已展开的内部组织
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回排序去重后的人员 ID。
    ///
    /// # 错误
    /// 组织关系非法或读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 过期或未生效成员不得进入当前采购组织筛选。
    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 查询账号在解析时点的唯一主属组织。
    ///
    /// # 参数
    /// * `user_id` - 当前账号
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 存在唯一主属组织时返回其 ID；没有主属组织时返回 `None`。
    ///
    /// # 错误
    /// 同一时点存在多条主属关系时拒绝。
    ///
    /// # 关键业务约束
    /// 不得默认放入根组织或公司范围。
    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>>;
}

/// 未接线时失败关闭，不得补公司范围。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedPurchaseDataScopePort;

impl FailClosedPurchaseDataScopePort {
    /// 返回未接线的共享端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可注入采购 Access 的失败关闭端口。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 仅用于不解析范围的装载路径；解析入口必须注入真实 adapter。
    pub fn shared() -> Arc<dyn PurchaseDataScopePort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl PurchaseDataScopePort for FailClosedPurchaseDataScopePort {
    /// 未接线时拒绝解析采购范围。
    ///
    /// # 参数
    /// * `_actor` - 已认证身份；本实现不读取。
    /// * `_entry` - 固定采购入口；本实现不读取。
    /// * `_executor` - 调用方执行器；本实现不读取。
    ///
    /// # 返回
    /// 不返回范围事实。
    ///
    /// # 错误
    /// 始终返回 `Error::Internal`，文案为采购范围端口未接线。
    async fn resolve(
        &self,
        _actor: &AuditActor,
        _entry: &Permission,
        _executor: &mut dyn Executor,
    ) -> Result<PurchaseResolvedScope> {
        Err(unwired())
    }

    /// 未接线时拒绝展开组织。
    ///
    /// # 参数
    /// * `_org_unit_ids` - 请求中的组织 ID；本实现不读取。
    /// * `_include_descendants` - 是否包含下级；本实现不读取。
    /// * `_executor` - 调用方执行器；本实现不读取。
    ///
    /// # 返回
    /// 不返回组织 ID。
    ///
    /// # 错误
    /// 始终返回 `Error::Internal`，文案为采购范围端口未接线。
    async fn expand_org_units(
        &self,
        _org_unit_ids: &[String],
        _include_descendants: bool,
        _executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Err(unwired())
    }

    /// 未接线时拒绝读取组织成员。
    ///
    /// # 参数
    /// * `_org_unit_ids` - 已展开的内部组织；本实现不读取。
    /// * `_at` - 解析时点；本实现不读取。
    /// * `_executor` - 调用方执行器；本实现不读取。
    ///
    /// # 返回
    /// 不返回人员 ID。
    ///
    /// # 错误
    /// 始终返回 `Error::Internal`，文案为采购范围端口未接线。
    async fn org_member_ids(
        &self,
        _org_unit_ids: &BTreeSet<String>,
        _at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(unwired())
    }

    /// 未接线时拒绝查询主属组织。
    ///
    /// # 参数
    /// * `_user_id` - 当前账号；本实现不读取。
    /// * `_at` - 解析时点；本实现不读取。
    /// * `_executor` - 调用方执行器；本实现不读取。
    ///
    /// # 返回
    /// 不返回组织 ID。
    ///
    /// # 错误
    /// 始终返回 `Error::Internal`，文案为采购范围端口未接线。
    async fn own_org(
        &self,
        _user_id: &str,
        _at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Err(unwired())
    }
}

/// 未装配授权 Port 时的失败关闭错误。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回内部错误，不退化为公司范围。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 禁止捕获后回退旧读取器或补 Company。
fn unwired() -> Error {
    Error::Internal("采购范围端口未接线".into())
}
