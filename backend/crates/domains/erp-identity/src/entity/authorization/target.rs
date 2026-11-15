use std::collections::{BTreeMap, BTreeSet};

use application_core::AuditActor;
use erp_core::common::time::Instant;

use super::{FactValue, ResourceTargets};
use crate::{Error, Permission, Result};

macro_rules! access_key {
    ($name:ident) => {
        #[doc = "服务端登记的授权标识，不接受集合名或字段表达式。"]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(String);
        impl $name {
            /// 解析登记标识。
            /// # 参数
            /// value 为小写字母、数字和下划线组成的标识。
            /// # 返回
            /// 已校验标识。
            /// # 错误
            /// 空值、超长或非法字符拒绝。
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
            /// 读取登记值。
            /// # 参数
            /// 无。
            /// # 返回
            /// 不可变标识。
            /// # 错误
            /// 不返回错误。
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
access_key!(RelationKey);
access_key!(QueryModelKey);

/// 本次完整操作目标；候选事实只能由业务模块从已验证输入构造。
#[derive(Debug, Clone)]
pub enum AccessTarget {
    Operation,
    Records(ResourceTargets),
    Candidate(ObjectFacts),
    OwnershipChange { id: String, candidate: ObjectFacts },
    Task { id: String, version: String },
}

impl AccessTarget {
    /// 构造单条已有记录目标。
    /// # 参数
    /// id 为拥有领域的真实记录 ID。
    /// # 返回
    /// 包含完整单条 ID 集合的目标。
    /// # 错误
    /// 非法或空 ID 返回校验错误。
    pub fn record(id: &str) -> Result<Self> {
        Ok(Self::Records(ResourceTargets::new(&[id])?))
    }
}

/// 同一授权阶段使用的事实解释上下文；构造上下文本身不产生授权。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessContext {
    pub actor: AuditActor,
    pub permission: Permission,
    pub resource: String,
    pub model: Option<QueryModelKey>,
    pub as_of: Instant,
}

/// 根对象或来源对象的可信事实；不能从 HTTP 请求直接反序列化。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectFacts {
    pub resource: String,
    pub id: String,
    pub version: String,
    pub valid_until: Option<Instant>,
    /// 审批冻结的单据类型或持久化任务种类，不接收权限或政策结论。
    pub subject_type: FactValue<String>,
    /// 由业务强外键核定的来源资源或对象类型；权限仍由 identity 选择。
    pub subject_source_resource: FactValue<String>,
    pub owner_id: FactValue<Option<String>>,
    /// 多负责人资源的完整当前负责人集合；与 owner_id 二选一。
    pub owner_ids: FactValue<BTreeSet<String>>,
    pub collaborators: FactValue<BTreeSet<String>>,
    pub history_participants: FactValue<BTreeSet<String>>,
    pub org_unit_id: FactValue<Option<String>>,
    /// 多负责人资源的完整归属组织集合；与 org_unit_id 二选一。
    pub org_unit_ids: FactValue<BTreeSet<String>>,
    pub settlement_party_id: FactValue<Option<String>>,
    pub warehouse_id: FactValue<Option<String>>,
    pub assignee_id: FactValue<Option<String>>,
    pub participants: FactValue<BTreeSet<String>>,
    pub completed: FactValue<bool>,
    pub relations: BTreeMap<RelationKey, RelatedObjects>,
}

/// 实际引用集合与成功读取的来源分别保留，避免缺失来源被过滤后放行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelatedObjects {
    pub expected_ids: BTreeSet<String>,
    pub objects: Vec<ObjectFacts>,
}

/// 来源集合的判定方式由登记规则固定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationMatch {
    Any,
    AllNonEmpty,
    ExactlyOne,
}
