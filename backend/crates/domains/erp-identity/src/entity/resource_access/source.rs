use std::collections::BTreeSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::normalize_required_text_ref;
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
    /// 固定读取范围动作，拒绝把记录读取权用于原单据写入。
    /// # 参数
    /// entry_permission 为服务端登记并解析的实际入口权限。
    /// # 返回
    /// 同资源列表采用 list，其余读取来源采用 detail。
    /// # 错误
    /// 同资源非读取动作拒绝；跨资源入口仍须由统一解析器校验功能权限。
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
    /// 取得固定资源代码。
    /// # 参数
    /// 无。
    /// # 返回
    /// 权限目录资源名。
    /// # 错误
    /// 不返回错误。
    pub fn code(self) -> &'static str {
        match self {
            Self::SalesOrder => "sales_order",
            Self::PurchaseOrder => "purchase_order",
        }
    }
}
/// 来源稳定身份；规则版本升级不得改变该身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceKey {
    pub rule: String,
    pub business_type: String,
    pub business_id: String,
    pub recipient: String,
}
impl SourceKey {
    /// 校验并生成无分隔符碰撞的来源身份。
    /// # 参数
    /// 无；字段来自可信业务来源。
    /// # 返回
    /// 结构化来源身份的完整 JSON 编码。
    /// # 错误
    /// 空值、超长或未规范化身份拒绝。
    pub fn id(&self) -> Result<String> {
        for value in [&self.rule, &self.business_type, &self.business_id, &self.recipient] {
            if normalize_required_text_ref(value, "来源身份不能为空", 128, "来源身份过长")? != *value
            {
                return Err(Error::ValidationError("来源身份不能带首尾空白".into()));
            }
        }
        Ok(serde_json::to_string(self)?)
    }
}
/// 读取目标；不包含写入或任务办理动作。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GrantTarget {
    pub resource: ResourceKind,
    pub id: String,
}
impl GrantTarget {
    /// 校验完整目标集合并去重。
    /// # 参数
    /// targets 为来源计算出的全部读取目标。
    /// # 返回
    /// 排序后的唯一目标，空集表示明确清空。
    /// # 错误
    /// 超过 1000 项、空或未规范化身份拒绝。
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
/// 来源终止记录永久保留；审计人与原因随状态原子保存。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct GrantSource {
    #[serde(flatten)]
    pub base: BaseModel,
    pub key: SourceKey,
    pub rule_version: u64,
    pub fact_version: u64,
    pub terminated: bool,
    pub targets: Vec<GrantTarget>,
    pub operator: String,
    pub reason: String,
}
impl GrantSource {
    /// 取得当前来源影响的资源类型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 去重后的资源类型，供修改前后取并集并递增授权修订号。
    /// # 错误
    /// 不返回错误。
    pub fn resources(&self) -> BTreeSet<ResourceKind> {
        self.targets.iter().map(|target| target.resource).collect()
    }
    /// 构造尚未应用事实的来源。
    /// # 参数
    /// key 为稳定来源身份。
    /// # 返回
    /// 等待首次应用的来源。
    /// # 错误
    /// 来源身份非法时拒绝。
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
    /// 原子计算来源的新状态；失败不修改当前值。
    /// # 参数
    /// rule_version、fact_version 为可信正版本；targets 是完整结果；operator、reason 为审计事实。
    /// # 返回
    /// 是否发生变化；完全相同的重放返回 false。
    /// # 错误
    /// 终止、版本倒退、同版本异载荷及非法输入拒绝。
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
    /// 永久终止单一来源，不影响其他来源。
    /// # 参数
    /// operator、reason 为撤销审计事实。
    /// # 返回
    /// 首次终止为 true，重放为 false。
    /// # 错误
    /// 操作人或原因非法时拒绝。
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
/// 每个来源独立保存的读取行；有效性必须同时检查来源状态及版本。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ResourceGrant {
    #[serde(flatten)]
    pub base: BaseModel,
    pub recipient: String,
    pub source_id: String,
    pub source_version: u64,
    pub resource: ResourceKind,
    pub resource_id: String,
    pub action: String,
    pub active: bool,
}

/// 来源全量结果；版本由正式事实提供，不能使用接收时间代替。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceUpdate {
    pub key: SourceKey,
    pub rule_version: u64,
    pub fact_version: u64,
    pub targets: Vec<GrantTarget>,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> GrantSource {
        GrantSource::new(SourceKey {
            rule: "approval".into(),
            business_type: "task".into(),
            business_id: "t1".into(),
            recipient: "u1".into(),
        })
        .unwrap()
    }
    fn target(id: &str) -> GrantTarget {
        GrantTarget { resource: ResourceKind::SalesOrder, id: id.into() }
    }
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
