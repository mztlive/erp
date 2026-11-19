use std::collections::BTreeSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::normalize_required_text_ref;
use erp_permissions::resources;
use serde::{Deserialize, Serialize};

use crate::{Error, Permission, Result};

/// 首批允许指定记录读取的资源；不接受任意集合名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    SalesOrder,
    PurchaseOrder,
}
impl ResourceKind {
    /// 根据服务端登记的实际入口选用记录读取动作：同资源仅支持 `list` 和 `detail`，跨资源沿用来源 `detail`。
    /// 同资源其他动作拒绝；跨资源入口的功能资格仍由统一授权解析器校验。
    pub fn scope_action(self, entry_permission: &Permission) -> Result<&'static str> {
        if entry_permission.resource() == self.code() {
            return match entry_permission.action() {
                "list" => Ok("list"),
                "detail" => Ok("detail"),
                _ => Err(Error::Forbidden("指定记录读取授权不能用于原单据写入".into())),
            };
        }
        Ok("detail")
    }
    /// 返回该记录类型在权限目录中的固定资源代码。
    pub fn code(self) -> &'static str {
        match self {
            Self::SalesOrder => resources::SALES_ORDER,
            Self::PurchaseOrder => resources::PURCHASE_ORDER,
        }
    }
}
/// 一个接收账号在同一业务来源和授权规则下的稳定身份；规则版本升级沿用此身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceKey {
    /// 产生读取授权的规则标识，例如人工授权或订单审批分配。
    pub rule: String,
    /// 规则所用业务事实的类型，用于区分任务、审批实例等业务身份。
    pub business_type: String,
    /// 该类型下稳定的业务记录或命令 ID。
    pub business_id: String,
    /// 读取授权的接收账号 ID；不同接收账号拥有独立来源。
    pub recipient: String,
}
impl SourceKey {
    /// 校验可信业务提供的四个身份字段，并以完整 JSON 编码生成来源主键，避免分隔符拼接碰撞。
    /// 字段为空、超过 128 字符或带首尾空白时拒绝。
    pub fn id(&self) -> Result<String> {
        self.validate()?;
        Ok(serde_json::to_string(self)?)
    }

    /// 校验组成来源身份的字段，供按字段查询时复用。
    pub(crate) fn validate(&self) -> Result<()> {
        for value in [&self.rule, &self.business_type, &self.business_id, &self.recipient] {
            if normalize_required_text_ref(value, "来源身份不能为空", 128, "来源身份过长")? != *value
            {
                return Err(Error::ValidationError("来源身份不能带首尾空白".into()));
            }
        }
        Ok(())
    }
}
/// 由资源类型和记录 ID 唯一标识的读取目标，权限仅限指定记录的读取。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GrantTarget {
    /// 被授权记录的业务资源类型，同 ID 的不同资源各自独立。
    pub resource: ResourceKind,
    /// 目标在所属资源中的业务记录 ID。
    pub id: String,
}
impl GrantTarget {
    // 对完整目标集按资源和 ID 排序去重，空集合表示清空；去重前最多 1000 项。
    // 单个 ID 为空、超过 128 字符或带首尾空白时拒绝。
    fn normalize(mut targets: Vec<Self>) -> Result<Vec<Self>> {
        if targets.len() > 1000 {
            return Err(Error::ValidationError("单来源最多 1000 个目标".into()));
        }
        for target in &targets {
            if normalize_required_text_ref(&target.id, "目标不能为空", 128, "目标过长")? != target.id
            {
                return Err(Error::ValidationError("目标身份不能带首尾空白".into()));
            }
        }

        targets.sort_unstable();
        targets.dedup();
        Ok(targets)
    }
}
/// 持久化单一读取授权来源的最新目标和版本；终止记录永久保留，操作人和原因与状态一并原子保存。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct GrantSource {
    /// 以稳定来源 ID 为主键的记录元数据；其版本用于授权行复验。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源的规则、业务记录和接收账号组成的稳定身份。
    pub key: SourceKey,
    /// 最近一次应用的授权规则版本；首次应用前为零。
    pub rule_version: u64,
    /// 最近一次应用的可信业务事实版本；首次应用前为零。
    pub fact_version: u64,
    /// 是否已永久撤销；终止后禁止应用后续结果。
    pub terminated: bool,
    /// 最近一次发布的排序去重目标集；来源终止后仍保留以记录影响面。
    pub targets: Vec<GrantTarget>,
    /// 最近一次实际应用或撤销的操作人标识，等待首次变更时为空。
    pub operator: String,
    /// 最近一次实际应用或撤销的审计原因，等待首次变更时为空。
    pub reason: String,
}
impl GrantSource {
    /// 提取当前目标涉及的资源类型，供发布前后取并集并递增相应授权修订号。
    pub fn resources(&self) -> BTreeSet<ResourceKind> {
        self.targets.iter().map(|target| target.resource).collect()
    }
    /// 校验稳定来源身份并创建待首次应用的来源，初始规则及事实版本为零，目标为空。
    pub fn new(key: SourceKey) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(key.id()?),
            key,
            rule_version: 0,
            fact_version: 0,
            terminated: false,
            targets: vec![],
            operator: String::new(),
            reason: String::new(),
        })
    }
    /// 校验可信且大于零的规则版本与事实版本、全量目标和审计信息后，原子更新来源。
    /// 终止、任一版本倒退、同版本目标不同或输入非法时拒绝，并保留原状态。
    /// 两个版本和规范化目标均相同时返回 `false`，保留原审计信息；实际更新返回 `true`。
    pub fn apply(
        &mut self,
        rule_version: u64,
        fact_version: u64,
        targets: Vec<GrantTarget>,
        operator: &str,
        reason: &str,
    ) -> Result<bool> {
        if self.terminated {
            return Err(Error::ConflictError("授权来源已经终止".into()));
        }

        let targets = GrantTarget::normalize(targets)?;
        let operator = normalize_required_text_ref(operator, "操作人不能为空", 128, "操作人过长")?;
        let reason = normalize_required_text_ref(reason, "原因不能为空", 512, "原因过长")?;

        if rule_version == 0
            || fact_version == 0
            || rule_version < self.rule_version
            || fact_version < self.fact_version
        {
            return Err(Error::ConflictError("授权事实或规则版本倒退".into()));
        }
        if (rule_version, fact_version) == (self.rule_version, self.fact_version) {
            return if targets == self.targets {
                Ok(false)
            } else {
                Err(Error::ConflictError("同版本授权结果不一致".into()))
            };
        }

        self.rule_version = rule_version;
        self.fact_version = fact_version;
        self.targets = targets;
        self.operator = operator;
        self.reason = reason;
        Ok(true)
    }
    /// 校验操作人和撤销原因后永久终止当前来源，并保存撤销审计信息；其他来源各自保留。
    /// 首次终止返回 `true`，已终止时返回 `false` 并保留原审计信息；非法审计输入返回错误。
    pub fn revoke(&mut self, operator: &str, reason: &str) -> Result<bool> {
        let operator = normalize_required_text_ref(operator, "操作人不能为空", 128, "操作人过长")?;
        let reason = normalize_required_text_ref(reason, "撤销原因不能为空", 512, "原因过长")?;
        if self.terminated {
            return Ok(false);
        }

        self.terminated = true;
        self.operator = operator;
        self.reason = reason;
        Ok(true)
    }
}
/// 单个来源对一条记录的读取授权行；判定有效性时须同时检查来源状态及版本。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ResourceGrant {
    /// 以来源、资源类型和目标 ID 定位的授权行元数据。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 获得该行读取授权的账号 ID，沿用来源接收人。
    pub recipient: String,
    /// 关联的来源主键，用于核查撤销状态和当前版本。
    pub source_id: String,
    /// 该行最近一次同步的来源记录版本，读取时必须与当前来源匹配。
    pub source_version: u64,
    /// 该行授权读取的资源类型。
    pub resource: ResourceKind,
    /// 该行授权读取的业务记录 ID。
    pub resource_id: String,
    /// 持久化动作固定为 `read`，供读取授权查询匹配。
    pub action: String,
    /// 目标是否仍在最近同步的来源结果中；有效读取仍要求来源未终止且版本一致。
    pub active: bool,
}

/// 来源的全量结果，由人工命令构造或由可信事实端口计算；接收时间不参与版本排序。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceUpdate {
    /// 要创建或更新的稳定来源身份。
    pub key: SourceKey,
    /// 计算本次读取授权所用的规则版本。
    pub rule_version: u64,
    /// 计算本次授权所依据的业务事实版本，用于拒绝过期结果。
    pub fact_version: u64,
    /// 本次应生效的完整目标集合，空集合明确清除已有目标。
    pub targets: Vec<GrantTarget>,
    /// 本次变更的审计原因，实际应用时写入来源。
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    // 构造等待首次应用的审批任务来源，固定接收账号供状态流转测试。
    fn source() -> GrantSource {
        GrantSource::new(SourceKey {
            rule: "approval".into(),
            business_type: "task".into(),
            business_id: "t1".into(),
            recipient: "u1".into(),
        })
        .unwrap()
    }
    // 用给定 ID 构造销售单读取目标，便于拼出重复或冲突目标集。
    fn target(id: &str) -> GrantTarget {
        GrantTarget { resource: ResourceKind::SalesOrder, id: id.into() }
    }
    // 验证目标排序去重保留资源类型，同版本等价目标重放返回无变化。
    #[test]
    fn targets_are_sorted_unique_and_keep_resource_identity() {
        let purchase = GrantTarget { resource: ResourceKind::PurchaseOrder, id: "a".into() };
        let mut source = source();
        source
            .apply(1, 1, vec![purchase.clone(), target("b"), target("a"), target("a")], "admin", "分配")
            .unwrap();
        assert_eq!(source.targets, vec![target("a"), target("b"), purchase]);
        assert!(
            !source
                .apply(1, 1, vec![source.targets[2].clone(), target("b"), target("a")], "admin", "重放")
                .unwrap()
        );
    }
    // 验证 1000 项上限按去重前的输入计数，超量失败保留原事实版本和目标。
    #[test]
    fn target_limit_applies_before_deduplication_and_failure_is_atomic() {
        let mut source = source();
        source.apply(1, 1, vec![target("a"); 1000], "admin", "分配").unwrap();
        assert!(matches!(
            source.apply(1, 2, vec![target("b"); 1001], "admin", "超量"),
            Err(Error::ValidationError(message)) if message == "单来源最多 1000 个目标"
        ));
        assert_eq!(source.fact_version, 1);
        assert_eq!(source.targets, vec![target("a")]);
    }
    // 验证同资源只接受明确的列表和详情动作，跨资源入口使用来源详情动作。
    #[test]
    fn source_read_does_not_authorize_original_document_writes() {
        for (resource, permission, action) in [
            (ResourceKind::SalesOrder, "sales_order:list", "list"),
            (ResourceKind::SalesOrder, "sales_order:detail", "detail"),
            (ResourceKind::SalesOrder, "invoice:create", "detail"),
            (ResourceKind::PurchaseOrder, "purchase_order:list", "list"),
            (ResourceKind::PurchaseOrder, "purchase_order:detail", "detail"),
            (ResourceKind::SalesOrder, " SALES_ORDER:LIST ", "list"),
        ] {
            assert_eq!(resource.scope_action(&Permission::parse(permission).unwrap()).unwrap(), action);
        }

        for (resource, permission) in [
            (ResourceKind::SalesOrder, "sales_order:update"),
            (ResourceKind::PurchaseOrder, "purchase_order:delete"),
            (ResourceKind::SalesOrder, "sales_order:*"),
        ] {
            assert!(matches!(
                resource.scope_action(&Permission::parse(permission).unwrap()),
                Err(Error::Forbidden(_))
            ));
        }
    }
    // 验证等价目标可重放，同版本目标冲突或事实版本倒退被拒绝，撤销后永久拒绝应用。
    #[test]
    fn replay_conflict_order_and_permanent_termination() {
        let mut s = source();
        assert!(s.apply(1, 2, vec![target("s1"), target("s1")], "admin", "分配").unwrap());
        assert_eq!(s.targets.len(), 1);

        assert!(!s.apply(1, 2, vec![target("s1")], "admin", "重试").unwrap());
        assert!(s.apply(1, 2, vec![target("s2")], "admin", "冲突").is_err());
        assert!(s.apply(2, 1, vec![], "admin", "旧事实").is_err());
        assert_eq!(s.targets, vec![target("s1")]);

        assert!(s.revoke("admin", "明确撤销").unwrap());
        assert!(!s.revoke("admin", "重试").unwrap());
        assert!(s.apply(2, 3, vec![target("s1")], "worker", "重算").is_err());
    }
    // 验证非法目标或撤销原因不改变状态，接收账号区分来源身份，并允许显式空目标。
    #[test]
    fn invalid_input_is_atomic_and_source_identity_is_unambiguous() {
        let mut s = source();
        assert!(s.apply(1, 1, vec![target(" ")], "admin", "原因").is_err());
        assert_eq!(s.fact_version, 0);
        assert!(s.revoke("admin", " ").is_err());
        assert!(!s.terminated);

        let mut other = s.key.clone();
        other.recipient = "u2".into();
        assert_ne!(s.key.id().unwrap(), other.id().unwrap());

        assert!(s.apply(1, 1, vec![], "admin", "明确空集").unwrap());
    }
}
