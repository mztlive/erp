//! 工作项构造与字段校验。

use entity_core::BaseModel;
use serde::{Deserialize, Serialize};

use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};

use super::{AssignmentSource, WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType};

const OBJECT_TYPE_MAX_LEN: usize = 64;
const OBJECT_ID_MAX_LEN: usize = 128;
const RESPONSIBILITY_KEY_MAX_LEN: usize = 128;
const RESPONSIBILITY_SCOPE_MAX_ITEMS: usize = 200;
const SUBJECT_VERSION_MAX_LEN: usize = 128;
const ROLE_MAX_LEN: usize = 128;
const ORGANIZATION_ID_MAX_LEN: usize = 128;
pub(super) const USER_ID_MAX_LEN: usize = 128;
const REASON_CODE_MAX_LEN: usize = 64;
const IMPACT_SUMMARY_MAX_LEN: usize = 512;
pub(super) const CLOSE_REASON_MAX_LEN: usize = 512;

/// 业务对象允许工作项引用的权威版本集合。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkItemSubjectVersions {
    values: Vec<String>,
}

impl WorkItemSubjectVersions {
    /// 创建不限制对象版本的关系。
    ///
    /// # 返回
    /// 返回接受任意任务对象版本的空约束。
    pub fn unrestricted() -> Self {
        Self::default()
    }

    /// 创建只接受指定权威版本的关系。
    ///
    /// # 参数
    /// * `values` - 业务对象生产者允许的版本集合
    ///
    /// # 返回
    /// 返回已规范化、排序并去重的版本约束。
    ///
    /// # 错误
    /// 集合为空、任一版本为空或过长时返回错误。
    pub fn constrained(values: Vec<String>) -> Result<Self> {
        if values.is_empty() {
            return Err(Error::from("受约束的工作项对象版本不能为空"));
        }
        let mut normalized = values
            .into_iter()
            .map(|value| {
                normalize_required_text(
                    value,
                    "工作项对象版本不能为空",
                    SUBJECT_VERSION_MAX_LEN,
                    "工作项对象版本过长",
                )
            })
            .collect::<Result<Vec<_>>>()?;
        normalized.sort();
        normalized.dedup();
        Ok(Self { values: normalized })
    }

    /// 判断任务对象版本是否被当前关系接受。
    ///
    /// # 参数
    /// * `actual` - 工作项冻结的对象版本
    ///
    /// # 返回
    /// 无约束或命中权威版本集合时返回 `true`。
    pub fn accepts(&self, actual: &str) -> bool {
        self.values.is_empty() || self.values.iter().any(|expected| expected == actual)
    }

    /// 返回已规范化的权威版本集合。
    ///
    /// # 返回
    /// 无约束时返回空切片，否则返回排序去重后的版本。
    pub fn as_slice(&self) -> &[String] {
        &self.values
    }
}

/// 创建任务所需的责任与业务对象快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemData {
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；创建开放任务时必填。
    pub owner_user_id: String,
    /// 初始责任来源。
    pub assignment_source: AssignmentSource,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
}

impl WorkItem {
    /// 创建任务并建立初始责任事实。
    ///
    /// 开放任务必须给出唯一责任人，并立即形成 `assigned_at` 与
    /// `current_assignment_at`。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或缺少个人责任人时返回错误。
    pub fn new(id: WorkItemId, data: WorkItemData) -> Result<Self> {
        Self::new_at_with_optional_responsibility(id, data, None, Vec::new(), Instant::now())
    }

    /// 创建带服务端责任维度的任务。
    ///
    /// 责任维度在创建时规范化并冻结；后续转交不得修改。
    /// 客户端输入不得直接调用本入口，应用服务只能传入已注册的固定维度。
    ///
    /// # 错误
    /// 责任维度为空、字段超长，或任务基础数据无效时返回错误。
    pub fn new_with_responsibility_key(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: impl Into<String>,
    ) -> Result<Self> {
        let responsibility_key = normalize_responsibility_key(responsibility_key.into())?;
        Self::new_at_with_optional_responsibility(
            id,
            data,
            Some(responsibility_key),
            Vec::new(),
            Instant::now(),
        )
    }

    /// 创建带服务端责任维度与稳定业务行范围的任务。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 任务责任与业务对象快照
    /// * `responsibility_key` - 参与开放唯一性的服务端稳定键
    /// * `scope_ids` - 已由服务端解析、跨版本稳定的业务行 ID
    ///
    /// # 返回
    /// 返回责任键和责任行集合均已规范化、排序并冻结的开放任务。
    ///
    /// # 错误
    /// 责任键或行 ID 为空、字段过长、行数超过上限，或任务基础数据无效时返回错误。
    pub fn new_with_responsibility_scope(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: impl Into<String>,
        scope_ids: Vec<String>,
    ) -> Result<Self> {
        let responsibility_key = normalize_responsibility_key(responsibility_key.into())?;
        let scope_ids = normalize_responsibility_scope(scope_ids)?;
        Self::new_at_with_optional_responsibility(
            id,
            data,
            Some(responsibility_key),
            scope_ids,
            Instant::now(),
        )
    }

    /// 使用确定时间创建任务，供事务编排和确定性测试使用。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或分派模式与个人责任不匹配时返回错误。
    pub fn new_at(id: WorkItemId, data: WorkItemData, at: Instant) -> Result<Self> {
        Self::new_at_with_optional_responsibility(id, data, None, Vec::new(), at)
    }

    /// 使用已规范化的可选责任键和责任范围创建任务。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 任务基础数据
    /// * `responsibility_key` - 可选服务端责任键
    /// * `responsibility_scope_ids` - 已规范化的稳定业务行集合
    /// * `at` - 责任形成时间
    ///
    /// # 返回
    /// 返回初始状态为开放且已指定到人的任务。
    ///
    /// # 错误
    /// 任务基础数据不合法或误用通用路径创建审批任务时返回错误。
    pub(super) fn new_at_with_optional_responsibility(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: Option<String>,
        responsibility_scope_ids: Vec<String>,
        at: Instant,
    ) -> Result<Self> {
        let normalized = NormalizedWorkItemData::try_from(data)?;
        if normalized.work_item_type == WorkItemType::DocumentApproval {
            return Err(Error::from("单据审批任务必须使用专用构造路径"));
        }
        if normalized.work_item_type == WorkItemType::ProcurementOrderCreation
            && (responsibility_key.is_none() || responsibility_scope_ids.is_empty())
        {
            return Err(Error::from("供给分配任务必须冻结责任键和责任行范围"));
        }
        if matches!(
            normalized.work_item_type,
            WorkItemType::FulfillmentOperation
                | WorkItemType::CustomerAcceptanceRegistration
                | WorkItemType::SupplierPaymentExecution
                | WorkItemType::SalesInvoiceExecution
        ) && responsibility_key.is_none()
        {
            return Err(Error::from("执行任务必须冻结责任键"));
        }
        if normalized.work_item_type != WorkItemType::ProcurementOrderCreation
            && !responsibility_scope_ids.is_empty()
        {
            return Err(Error::from("只有供给分配任务可以冻结责任行范围"));
        }
        let responsibility_actor_ids = vec![normalized.owner_user_id.clone()];
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            work_item_type: normalized.work_item_type,
            approval_node_execution_id: None,
            business_object_type: normalized.business_object_type,
            business_object_id: normalized.business_object_id,
            responsibility_key,
            responsibility_scope_ids,
            subject_version: normalized.subject_version,
            status: WorkItemStatus::Open,
            owner_role: normalized.owner_role,
            owner_organization_id: normalized.owner_organization_id,
            owner_user_id: Some(normalized.owner_user_id),
            responsibility_actor_ids,
            assignment_source: normalized.assignment_source,
            assigned_at: Some(at),
            started_at: None,
            current_assignment_at: Some(at),
            last_activity_at: None,
            priority: normalized.priority,
            due_at: normalized.due_at,
            reason_code: normalized.reason_code,
            impact_summary: normalized.impact_summary,
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
        })
    }

    /// 更新开放任务的业务影响摘要。
    ///
    /// # 参数
    /// * `impact_summary` - 面向用户的最新业务影响；空白值会规范化为空
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务非开放或摘要超过长度上限时返回错误。
    pub fn update_impact_summary(&mut self, impact_summary: Option<String>) -> Result<()> {
        self.ensure_open()?;
        self.impact_summary = normalize_optional_text(impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?;
        Ok(())
    }

    /// 为重新释放的采购需求创建一条新的开放任务。
    ///
    /// # 参数
    /// * `id` - 新任务主键
    /// * `subject_version` - 重新释放时的销售当前版本
    /// * `impact_summary` - 重新释放后的剩余数量摘要
    ///
    /// # 返回
    /// 返回复制当前责任人、责任来源和冻结行范围的新开放任务。
    ///
    /// # 错误
    /// 当前任务不是采购建单终态、缺少责任人或冻结责任事实时返回错误。
    ///
    /// # 关键业务约束
    /// 历史终态任务保持不变；重新释放必须形成新任务身份。
    pub fn successor_for_released_requirement(
        &self,
        id: WorkItemId,
        subject_version: String,
        impact_summary: Option<String>,
    ) -> Result<Self> {
        if self.work_item_type != WorkItemType::ProcurementOrderCreation {
            return Err(Error::from("只有供给分配任务可以创建释放后继任务"));
        }
        if self.status == WorkItemStatus::Open {
            return Err(Error::from("开放供给分配任务不能创建释放后继任务"));
        }
        let responsibility_key = self
            .responsibility_key()
            .ok_or_else(|| Error::from("历史供给分配任务缺少责任键"))?
            .to_string();
        let owner_user_id = self
            .owner_user_id
            .clone()
            .ok_or_else(|| Error::from("历史供给分配任务缺少具体责任人"))?;
        Self::new_with_responsibility_scope(
            id,
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: self.business_object_type.clone(),
                business_object_id: self.business_object_id.clone(),
                subject_version,
                owner_role: self.owner_role.clone(),
                owner_organization_id: self.owner_organization_id.clone(),
                owner_user_id,
                assignment_source: self.assignment_source,
                priority: self.priority,
                due_at: self.due_at,
                reason_code: Some("PROCUREMENT_QUANTITY_RELEASED".to_string()),
                impact_summary,
            },
            responsibility_key,
            self.responsibility_scope_ids.clone(),
        )
    }
}

/// 规范化参与开放任务唯一性的服务端责任键。
///
/// # 参数
/// * `responsibility_key` - 原始责任键
///
/// # 返回
/// 返回去除首尾空白后的非空责任键。
///
/// # 错误
/// 责任键为空或超过长度上限时返回错误。
fn normalize_responsibility_key(responsibility_key: String) -> Result<String> {
    normalize_required_text(
        responsibility_key,
        "责任维度不能为空",
        RESPONSIBILITY_KEY_MAX_LEN,
        "责任维度过长",
    )
}

/// 规范化服务端冻结的稳定业务行范围。
///
/// # 参数
/// * `scope_ids` - 原始稳定业务行 ID 集合
///
/// # 返回
/// 返回逐项去空白、排序并去重后的非空集合。
///
/// # 错误
/// 集合为空、超过最大行数，或任一 ID 为空或过长时返回错误。
fn normalize_responsibility_scope(scope_ids: Vec<String>) -> Result<Vec<String>> {
    if scope_ids.is_empty() || scope_ids.len() > RESPONSIBILITY_SCOPE_MAX_ITEMS {
        return Err(Error::from("责任范围行数必须在1-200之间"));
    }
    let mut normalized = scope_ids
        .into_iter()
        .map(|scope_id| {
            normalize_required_text(
                scope_id,
                "责任范围行不能为空",
                OBJECT_ID_MAX_LEN,
                "责任范围行过长",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

struct NormalizedWorkItemData {
    work_item_type: WorkItemType,
    business_object_type: String,
    business_object_id: String,
    subject_version: String,
    owner_role: String,
    owner_organization_id: String,
    owner_user_id: String,
    assignment_source: AssignmentSource,
    priority: WorkItemPriority,
    due_at: Option<Instant>,
    reason_code: Option<String>,
    impact_summary: Option<String>,
}

impl TryFrom<WorkItemData> for NormalizedWorkItemData {
    type Error = Error;

    fn try_from(data: WorkItemData) -> Result<Self> {
        Ok(Self {
            work_item_type: data.work_item_type,
            business_object_type: normalize_required_text(
                data.business_object_type,
                "业务对象类型不能为空",
                OBJECT_TYPE_MAX_LEN,
                "业务对象类型过长",
            )?,
            business_object_id: normalize_required_text(
                data.business_object_id,
                "业务对象ID不能为空",
                OBJECT_ID_MAX_LEN,
                "业务对象ID过长",
            )?,
            subject_version: normalize_required_text(
                data.subject_version,
                "对象版本不能为空",
                SUBJECT_VERSION_MAX_LEN,
                "对象版本过长",
            )?,
            owner_role: normalize_required_text(
                data.owner_role,
                "责任角色不能为空",
                ROLE_MAX_LEN,
                "责任角色过长",
            )?,
            owner_organization_id: normalize_required_text(
                data.owner_organization_id,
                "责任组织不能为空",
                ORGANIZATION_ID_MAX_LEN,
                "责任组织过长",
            )?,
            owner_user_id: normalize_required_text(
                data.owner_user_id,
                "责任人不能为空",
                USER_ID_MAX_LEN,
                "责任人过长",
            )?,
            assignment_source: data.assignment_source,
            priority: data.priority,
            due_at: data.due_at,
            reason_code: normalize_optional_text(data.reason_code, "原因代码", REASON_CODE_MAX_LEN)?,
            impact_summary: normalize_optional_text(data.impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        account, direct_data, AssignmentSource, AvailableWorkItemAccount, WorkItem, WorkItemData,
        WorkItemStatus, WorkItemSubjectVersions, WorkItemType,
    };
    use crate::AccountStatus;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_core::AccountKind;

    #[test]
    fn account_and_subject_version_value_objects_fail_closed() {
        let active = account(AccountStatus::Active);
        let available = AvailableWorkItemAccount::from_account_kind(&active, AccountKind::Admin).unwrap();
        assert_eq!(available.account_id(), "account-1");
        assert_eq!(available.kind(), AccountKind::Admin);
        assert!(AvailableWorkItemAccount::from_account(&account(AccountStatus::Suspended)).is_err());

        let versions = WorkItemSubjectVersions::constrained(vec![
            "v2".to_string(),
            " v1 ".to_string(),
            "v2".to_string(),
        ])
        .unwrap();
        assert_eq!(versions.as_slice(), &["v1".to_string(), "v2".to_string()]);
        assert!(versions.accepts("v1"));
        assert!(!versions.accepts("v3"));
        assert!(WorkItemSubjectVersions::constrained(vec![" ".to_string()]).is_err());
        assert!(WorkItemSubjectVersions::unrestricted().accepts("any"));
    }

    #[test]
    fn open_task_requires_personal_owner() {
        let item = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(item.responsibility_actor_ids, vec!["alice".to_string()]);
        let missing = WorkItemData {
            owner_user_id: "   ".to_string(),
            ..direct_data()
        };
        assert!(WorkItem::new_at(WorkItemId::new("wi-2"), missing, Instant::from_unix_secs(100)).is_err());
    }

    #[test]
    fn procurement_task_cannot_bypass_frozen_scope_constructor() {
        let data = WorkItemData {
            work_item_type: WorkItemType::ProcurementOrderCreation,
            ..direct_data()
        };
        assert!(WorkItem::new_at(
            WorkItemId::new("wi-procurement"),
            data.clone(),
            Instant::from_unix_secs(100),
        )
        .is_err());
        assert!(WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-procurement-key"),
            data,
            "sales-lines:key",
        )
        .is_err());
    }

    #[test]
    fn responsibility_scope_is_normalized_and_system_completion_preserves_history() {
        let mut item = WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-procurement"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                ..direct_data()
            },
            " sales-lines:key ",
            vec![" line-b ".to_string(), "line-a".to_string(), "line-a".to_string()],
        )
        .unwrap();
        assert_eq!(item.responsibility_key(), Some("sales-lines:key"));
        assert_eq!(
            item.responsibility_scope_ids(),
            &["line-a".to_string(), "line-b".to_string()]
        );
        item.update_impact_summary(Some(" 剩余 6 件待采购 ".to_string()))
            .unwrap();
        assert_eq!(item.impact_summary.as_deref(), Some("剩余 6 件待采购"));
        item.complete_when_requirement_satisfied(Instant::from_unix_secs(120))
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.completed_by.as_deref(), Some("__system__"));
        assert_eq!(item.owner_user_id.as_deref(), Some("alice"));
        assert!(item
            .complete_when_requirement_satisfied(Instant::from_unix_secs(130))
            .is_err());

        let successor = item
            .successor_for_released_requirement(
                WorkItemId::new("wi-procurement-released"),
                "sales-revision-2".to_string(),
                Some("剩余 4 件待采购".to_string()),
            )
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(successor.base.id, "wi-procurement-released");
        assert_eq!(successor.status, WorkItemStatus::Open);
        assert_eq!(successor.subject_version, "sales-revision-2");
        assert_eq!(successor.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(successor.assignment_source, AssignmentSource::SystemRule);
        assert_eq!(successor.responsibility_key(), Some("sales-lines:key"));
        assert_eq!(
            successor.responsibility_scope_ids(),
            item.responsibility_scope_ids()
        );
        assert_eq!(
            successor.reason_code.as_deref(),
            Some("PROCUREMENT_QUANTITY_RELEASED")
        );
        assert_eq!(successor.impact_summary.as_deref(), Some("剩余 4 件待采购"));
    }

    #[test]
    fn responsibility_scope_rejects_empty_lines() {
        assert!(WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-empty"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                ..direct_data()
            },
            "sales-lines:key",
            Vec::new(),
        )
        .is_err());
    }
}
