use std::collections::{BTreeMap, BTreeSet};

use application_core::AuditActor;
use erp_core::common::time::Instant;

use super::{FactValue, ResourceTargets};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::{Error, Permission, Result};

macro_rules! access_key {
    ($name:ident) => {
        /// 类型化的登记键，区分查询模型与来源关系。
        #[doc = "服务端登记的授权标识，不接受集合名或字段表达式。"]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(
            // 私有文本由 `parse` 校验后保存。
            String,
        );
        impl $name {
            /// 解析服务端登记键；仅接受 1 至 64 字节的 ASCII 小写字母、数字或下划线。
            pub fn parse(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 64
                    || !value.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
                {
                    return Err(Error::ValidationError("授权登记标识非法".into()));
                }
                Ok(Self(value))
            }
            /// 借用键文本，供登记查找、比较及版本编码。
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
access_key!(RelationKey);
access_key!(QueryModelKey);

/// 本次操作的完整目标；候选事实由业务模块根据经过校验的输入构造。
#[derive(Debug, Clone)]
pub enum AccessTarget {
    /// 仅核对操作资格，用于登记为无对象的动作。
    Operation,
    /// 读取已有记录的当前事实后检查单条或批量目标。
    Records(
        /// 本次操作涉及的完整记录 ID 集合。
        ResourceTargets,
    ),
    /// 创建前检查拟写入的归属和来源。
    Candidate(
        /// 服务端确认的候选事实，版本留空以区分持久化记录。
        ObjectFacts,
    ),
    /// 同时检查原记录的操作资格和变更后的归属。
    OwnershipChange {
        /// 用于读取当前事实的原记录 ID。
        id: String,
        /// 服务端核定的变更后归属事实。
        candidate: ObjectFacts,
    },
    /// 用精确任务身份及版本检查任务操作。
    Task {
        /// 任务自身的记录 ID。
        id: String,
        /// 调用方持有的非空任务版本，须与当前任务事实一致。
        version: String,
    },
}

impl AccessTarget {
    /// 将所属业务域的真实记录 ID 封装为单条目标，沿用目标集的非空和格式校验。
    pub fn record(id: &str) -> Result<Self> {
        Ok(Self::Records(ResourceTargets::new(&[id])?))
    }
}

/// 本次取证和映射使用的固定上下文；授权结论由身份域规则另行判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessContext {
    /// 本次执行操作的账号，用于解释本人、协作及参与条件。
    pub actor: AuditActor,
    /// 当前规则使用的权限，可能是独立入口或来源动作。
    pub permission: Permission,
    /// 当前正在读取或映射的业务资源。
    pub resource: String,
    /// 当前查询模型的登记键，对象取证时为空。
    pub model: Option<QueryModelKey>,
    /// 本阶段统一的授权时点，用于解释时效事实和计算到期边界。
    pub as_of: Instant,
}

impl AccessContext {
    /// 取证时根据固定范围规格决定能否装载历史参与事实；其他已接入动作返回 `false`。
    /// 未接入权限或缺少范围规格时返回校验错误，实际授权仍由规则判定。
    pub fn allows_history(&self) -> Result<bool> {
        match scope_spec::kind(&self.permission)? {
            ActionKind::Scope => Ok(scope_spec::require_scope(&self.permission)?.allows_history()),
            _ => Ok(false),
        }
    }
}

/// 服务端确认的根对象、来源或候选事实；禁止直接从 HTTP 请求反序列化。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectFacts {
    /// 对象的业务资源标识，与 ID 共同确定身份。
    pub resource: String,
    /// 业务记录 ID；尚无记录身份的候选可以留空。
    pub id: String,
    /// 已有记录的事实版本，用于精确任务校验和对象复验；候选留空。
    pub version: String,
    /// 当前责任关系最早到期的时刻，到点后须重新取证。
    pub valid_until: Option<Instant>,
    /// 审批中冻结的单据类型或持久化任务种类；只承载业务类型事实。
    pub subject_type: FactValue<String>,
    /// 由业务强外键确认的来源资源或对象类型，供身份域选择授权规则。
    pub subject_source_resource: FactValue<String>,
    /// 单值负责人，`Known(None)` 表示确认无人负责；与 `owner_ids` 二选一。
    pub owner_id: FactValue<Option<String>>,
    /// 多负责人资源的完整当前负责人集合；与 `owner_id` 二选一。
    pub owner_ids: FactValue<BTreeSet<String>>,
    /// 当前协作账号集合，用于协作范围判断。
    pub collaborators: FactValue<BTreeSet<String>>,
    /// 历史参与账号集合，仅在规则开放历史读取时用于补充基础范围。
    pub history_participants: FactValue<BTreeSet<String>>,
    /// 单值归属组织；与 `org_unit_ids` 二选一。
    pub org_unit_id: FactValue<Option<String>>,
    /// 多负责人资源的完整归属组织集合；与 `org_unit_id` 二选一。
    pub org_unit_ids: FactValue<BTreeSet<String>>,
    /// 当前结算主体 ID，用于结算范围判断。
    pub settlement_party_id: FactValue<Option<String>>,
    /// 当前所属仓库 ID，用于仓库范围判断。
    pub warehouse_id: FactValue<Option<String>>,
    /// 任务的当前处理人账号，`Known(None)` 表示确认尚未指派。
    pub assignee_id: FactValue<Option<String>>,
    /// 任务参与账号集合，供参与条件独立匹配。
    pub participants: FactValue<BTreeSet<String>>,
    /// 持久化的任务完成标记，供授权条件判断。
    pub completed: FactValue<bool>,
    /// 按登记键装载的真实来源，缺键表示该关系尚未取证。
    pub relations: BTreeMap<RelationKey, RelatedObjects>,
}

/// 一条来源关系的取证结果，分别保留真实引用和实际读到的对象以核对覆盖。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelatedObjects {
    /// 当前对象实际引用的全部来源 ID，包含未读到对象的引用。
    pub expected_ids: BTreeSet<String>,
    /// 实际读到的来源对象及其事实，按无序集合校验。
    pub objects: Vec<ObjectFacts>,
}

/// 固定规则指定的来源覆盖要求，以实际引用和有权来源为依据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationMatch {
    /// 至少一个来源有权，仅用于受限只读视图。
    Any,
    /// 来源引用非空，且全部存在并有权。
    AllNonEmpty,
    /// 恰有一个来源引用，且该来源存在并有权。
    ExactlyOne,
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_permissions::{internal, purchase_order};

    use super::*;

    // 验证历史事实的装载许可随登记权限而定，未知权限报错。
    #[test]
    fn history_loading_follows_the_registered_permission_specification() {
        let mut context = AccessContext {
            actor: AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin),
            permission: purchase_order::LIST,
            resource: "purchase_order".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        };
        for (permission, expected) in [
            (purchase_order::LIST, true),
            (purchase_order::DETAIL, true),
            (purchase_order::SAVE_DRAFT, false),
            (internal::PAYMENT_REQUEST_CANDIDATES, false),
            (internal::PURCHASE_PORTAL_IMPACT_READ, false),
            (internal::WORK_ITEM_EXECUTE, false),
        ] {
            context.permission = permission;
            assert_eq!(context.allows_history().unwrap(), expected, "{}", context.permission);
        }
        context.permission = Permission::parse("unknown:list").unwrap();
        assert!(matches!(context.allows_history(), Err(Error::ValidationError(_))));
    }
}
