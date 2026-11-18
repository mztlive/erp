//! `legacy_import_confirmation`：旧数据导入业务确认事实（数据模型 §6.12）。

use std::collections::{BTreeMap, BTreeSet};

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::state::{DocumentState, ensure_transition};
use erp_core::common::time::Instant;
use erp_core::ids::{LegacyImportBatchId, WorkItemId};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 确认范围（销售、采购、运营、仓储、财务等）最大长度。
const SCOPE_MAX_LEN: usize = 64;
/// 责任角色最大长度。
const ROLE_MAX_LEN: usize = 128;
/// 导入规则版本最大长度。
const RULE_VERSION_MAX_LEN: usize = 64;
/// 退回原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 64;
/// 意见或说明最大长度。
const COMMENT_MAX_LEN: usize = 1024;
/// 确认人标识最大长度。
const DECIDER_MAX_LEN: usize = 128;

/// 确认状态（数据模型 §6.12：`PENDING`、`CONFIRMED`、`REJECTED`、`INVALIDATED`）。
///
/// 固定状态机：待确认单向推进到确认、退回或失效；已完成的
/// `CONFIRMED`/`REJECTED` 与已失效的 `INVALIDATED` 永久保留（§6.12）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfirmationStatus {
    /// 待确认。
    Pending,
    /// 已确认。
    Confirmed,
    /// 已退回。
    Rejected,
    /// 已失效（被新试算、规则或 manifest 取代）。
    Invalidated,
}

impl ConfirmationStatus {
    /// 提供确认状态的中文文案，区分等待决定、确认、退回及失效等情况。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待确认",
            Self::Confirmed => "已确认",
            Self::Rejected => "已退回",
            Self::Invalidated => "已失效",
        }
    }

    /// 提供确认状态的稳定代码，供持久化、筛选和矩阵汇总复用。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Confirmed => "CONFIRMED",
            Self::Rejected => "REJECTED",
            Self::Invalidated => "INVALIDATED",
        }
    }
}

impl DocumentState for ConfirmationStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Confirmed, Self::Rejected, Self::Invalidated],
            Self::Confirmed | Self::Rejected | Self::Invalidated => &[],
        }
    }
}

/// 确认决策（数据模型 §6.12：`CONFIRM_SCOPE` 或 `RETURN_FOR_FIX`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfirmationDecision {
    /// 确认本责任范围。
    ConfirmScope,
    /// 退回修复。
    ReturnForFix,
}

impl ConfirmationDecision {
    /// 提供业务决定的中文文案，供责任范围确认和退回修复的展示使用。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ConfirmScope => "确认本范围",
            Self::ReturnForFix => "退回修复",
        }
    }

    /// 提供业务决定的稳定代码，确保命令及持久化使用同一含义。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConfirmScope => "CONFIRM_SCOPE",
            Self::ReturnForFix => "RETURN_FOR_FIX",
        }
    }
}

/// W18 固定业务确认范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConfirmationScope {
    /// 销售责任。
    Sales,
    /// 采购责任。
    Procurement,
    /// 运营责任。
    Operations,
    /// 仓储责任。
    Warehouse,
    /// 财务责任。
    Finance,
}

/// 批次对象集分隔符（中英文半角口径，见 `required_for_object_set`）。
const OBJECT_SET_SEPARATORS: [char; 5] = [',', ';', '|', '/', '、'];

/// 来源对象类型（已大写去空白）→ 确认范围对照表（与 `required_for_object_set` 同语义）。
const OBJECT_TYPE_SCOPE_TABLE: &[(&str, ConfirmationScope)] = &[
    ("CUSTOMER", ConfirmationScope::Sales),
    ("CONTRACT", ConfirmationScope::Sales),
    ("CARD_SALES_ORDER", ConfirmationScope::Sales),
    ("客户", ConfirmationScope::Sales),
    ("合同", ConfirmationScope::Sales),
    ("卡券销售", ConfirmationScope::Sales),
    ("卡券销售单", ConfirmationScope::Sales),
    ("SUPPLIER", ConfirmationScope::Procurement),
    ("SKU", ConfirmationScope::Procurement),
    ("供应商", ConfirmationScope::Procurement),
    ("商品SKU", ConfirmationScope::Procurement),
    ("商品 SKU", ConfirmationScope::Procurement),
    ("CARD_CATEGORY", ConfirmationScope::Operations),
    ("卡券类目", ConfirmationScope::Operations),
    ("WAREHOUSE", ConfirmationScope::Warehouse),
    ("OPENING_STOCK", ConfirmationScope::Warehouse),
    ("仓库", ConfirmationScope::Warehouse),
    ("期初库存", ConfirmationScope::Warehouse),
    ("CARD_OPENING_AR", ConfirmationScope::Finance),
    ("卡券期初应收", ConfirmationScope::Finance),
    ("期初应收", ConfirmationScope::Finance),
];

// 用清理并转为大写的来源对象类型查固定对照表；未登记时返回 `None`，由调用方拒绝无法派责的批次。
fn lookup_confirmation_scope(object: &str) -> Option<ConfirmationScope> {
    OBJECT_TYPE_SCOPE_TABLE.iter().find(|(name, _)| *name == object).map(|(_, scope)| *scope)
}

impl ConfirmationScope {
    /// 将请求或存储中的范围代码解析为已登记的确认范围；未知范围返回错误，不允许进入责任矩阵。
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "SALES" => Ok(Self::Sales),
            "PROCUREMENT" => Ok(Self::Procurement),
            "OPERATIONS" => Ok(Self::Operations),
            "WAREHOUSE" => Ok(Self::Warehouse),
            "FINANCE" => Ok(Self::Finance),
            _ => Err(Error::from("确认范围未在 W18 固定注册表中")),
        }
    }

    /// 提供大写的稳定责任范围代码，供矩阵比较和持久化使用。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sales => "SALES",
            Self::Procurement => "PROCUREMENT",
            Self::Operations => "OPERATIONS",
            Self::Warehouse => "WAREHOUSE",
            Self::Finance => "FINANCE",
        }
    }

    /// 返回该范围唯一绑定的 RBAC 责任角色，供任务派责和资格核验。
    pub fn owner_role(self) -> &'static str {
        match self {
            Self::Sales => "role-sales",
            Self::Procurement => "role-procurement",
            Self::Operations => "role-operations",
            Self::Warehouse => "role-warehouse",
            Self::Finance => "role-finance",
        }
    }

    /// 将批次来源对象集映射为去重、稳定排序的必确认范围；集合为空或包含未登记对象时拒绝生成矩阵。
    pub fn required_for_object_set(source_object_set: &str) -> Result<BTreeSet<Self>> {
        let mut scopes = BTreeSet::new();
        for raw in source_object_set.split(OBJECT_SET_SEPARATORS) {
            let object = raw.trim().to_ascii_uppercase();
            if object.is_empty() {
                continue;
            }
            let scope = lookup_confirmation_scope(&object)
                .ok_or_else(|| Error::from(format!("批次对象类型 {raw} 未配置 W18 确认责任")))?;
            scopes.insert(scope);
        }

        if scopes.is_empty() {
            return Err(Error::from("无法确定这批导入资料需要哪些岗位确认，请核对批次中的业务对象"));
        }
        Ok(scopes)
    }
}

/// 确认矩阵完成一次决策后的唯一下一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationMatrixDecision {
    /// 等待其他责任范围确认。
    AwaitOtherConfirmations,
    /// 全部必要范围已确认，可进入待应用。
    StartApply,
    /// 当前试算被退回，必须修复并重新试算。
    FixAndRevalidate,
}

/// 导入确认创建数据（数据模型 §6.12）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImportConfirmationData {
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 责任范围（销售、采购、运营、仓储、财务等）。
    pub confirmation_scope: String,
    /// 责任角色。
    pub owner_role: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本。
    pub trial_version: u32,
    /// 本次确认针对的导入规则版本。
    pub import_rule_version: String,
    /// 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务。
    pub work_item_id: WorkItemId,
}

/// 旧数据导入业务确认事实（数据模型 §6.12）。
///
/// 正式确认事实：创建即 `PENDING`，只允许按固定状态机完成确认、退回或失效；
/// 已完成的确认永久保留，不设业务软删除（§4.5）。批次上的确认状态仅是该表
/// 事实的派生摘要，不保存单个确认人作为多范围事实源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct LegacyImportConfirmation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 责任范围。
    pub confirmation_scope: String,
    /// 责任角色。
    pub owner_role: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本。
    pub trial_version: u32,
    /// 本次确认针对的导入规则版本。
    pub import_rule_version: String,
    /// 确认状态。
    pub status: ConfirmationStatus,
    /// 确认决策；待确认/失效时为空。
    pub decision: Option<ConfirmationDecision>,
    /// 退回原因代码（退回时必填）。
    pub reason_code: Option<String>,
    /// 意见说明（确认意见可选）。
    pub comment: Option<String>,
    /// 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务（唯一）。
    pub work_item_id: WorkItemId,
    /// 实际确认或退回人。
    pub decided_by: Option<String>,
    /// 实际确认或退回时间。
    pub decided_at: Option<Instant>,
    /// 失效时间。
    pub invalidated_at: Option<Instant>,
    /// 替代确认事实。
    pub replacement_confirmation_id: Option<erp_core::ids::LegacyImportConfirmationId>,
}

impl LegacyImportConfirmation {
    /// 创建处于 `PENDING` 的确认事实，冻结该任务的责任范围、角色和规则版本，决定与审计字段保持空。必填文本去首尾空白后为空或超长时拒绝。
    pub fn new(
        id: erp_core::ids::LegacyImportConfirmationId,
        data: LegacyImportConfirmationData,
    ) -> Result<Self> {
        let confirmation_scope = normalize_required_text(
            data.confirmation_scope,
            "确认范围不能为空",
            SCOPE_MAX_LEN,
            "确认范围过长",
        )?;
        let owner_role =
            normalize_required_text(data.owner_role, "责任角色不能为空", ROLE_MAX_LEN, "责任角色过长")?;
        let import_rule_version = normalize_required_text(
            data.import_rule_version,
            "导入规则版本不能为空",
            RULE_VERSION_MAX_LEN,
            "导入规则版本过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            batch_id: data.batch_id,
            confirmation_scope,
            owner_role,
            batch_version: data.batch_version,
            trial_version: data.trial_version,
            import_rule_version,
            status: ConfirmationStatus::Pending,
            decision: None,
            reason_code: None,
            comment: None,
            work_item_id: data.work_item_id,
            decided_by: None,
            decided_at: None,
            invalidated_at: None,
            replacement_confirmation_id: None,
        })
    }

    /// 将批次、试算和去空白后的规则版本拼成 `batch/trial/rule` 主题版本，用于任务冻结和后续核对。
    pub fn subject_version(batch_version: u32, trial_version: u32, rule_version: &str) -> String {
        format!("batch:{batch_version};trial:{trial_version};rule:{}", rule_version.trim())
    }

    /// 按批次、试算与规则版本核对确认是否属于指定快照，且确认不能已失效；规则直接比较原文，不清理空白。
    pub fn belongs_to_snapshot(&self, batch_version: u32, trial_version: u32, rule_version: &str) -> bool {
        self.batch_version == batch_version
            && self.trial_version == trial_version
            && self.import_rule_version == rule_version
            && self.status != ConfirmationStatus::Invalidated
    }

    /// 检查确认事实仍在等待责任人决定，供完成与替换流程识别未处理记录。
    pub fn is_pending(&self) -> bool {
        self.status == ConfirmationStatus::Pending
    }

    /// 在规则版本匹配且未失效的确认中取最大试算版本，便于选中当前有效矩阵；规则先去首尾空白，无匹配返回 `None`。
    pub fn latest_active_trial(confirmations: &[Self], rule_version: &str) -> Option<u32> {
        confirmations
            .iter()
            .filter(|item| {
                item.status != ConfirmationStatus::Invalidated
                    && item.import_rule_version == rule_version.trim()
            })
            .map(|item| item.trial_version)
            .max()
    }

    /// 筛出指定批次、试算和规则版本中尚未失效的确认并返回副本，供计算当前确认矩阵。
    pub fn current_matrix(
        confirmations: &[Self],
        batch_version: u32,
        trial_version: u32,
        rule_version: &str,
    ) -> Vec<Self> {
        confirmations
            .iter()
            .filter(|item| item.belongs_to_snapshot(batch_version, trial_version, rule_version))
            .cloned()
            .collect()
    }

    /// 创建确认前校验试算单调性和快照一致性；批次与试算版本同时到上限、试算倒退、同一试算的批次或规则版本不一致，或该试算已退回时拒绝。
    pub fn ensure_trial_snapshot(
        confirmations: &[Self],
        batch_version: u32,
        trial_version: u32,
        rule_version: &str,
    ) -> Result<()> {
        if batch_version == u32::MAX && trial_version == u32::MAX {
            return Err(Error::from("批次与试算版本不能同时到达上限"));
        }
        if confirmations
            .iter()
            .map(|item| item.trial_version)
            .max()
            .is_some_and(|current| current > trial_version)
        {
            return Err(Error::from("新试算版本不得低于已有确认版本"));
        }

        let mut rejected = false;
        for item in confirmations.iter().filter(|item| item.trial_version == trial_version) {
            if item.batch_version != batch_version || item.import_rule_version != rule_version {
                return Err(Error::from("试算所用的导入批次或规则版本已变化，请重新试算"));
            }
            rejected |= item.status == ConfirmationStatus::Rejected;
        }
        if rejected {
            return Err(Error::from("当前试算已被退回，必须修复并生成新试算版本"));
        }
        Ok(())
    }

    /// 由本次决定和矩阵确定流程去向：退回则修复重验；未退回且所有必确认范围通过即可启动应用，否则等待其他确认。
    pub fn matrix_decision(
        decision: ConfirmationDecision,
        confirmations: &[Self],
        required_scopes: &BTreeSet<ConfirmationScope>,
    ) -> ConfirmationMatrixDecision {
        if decision == ConfirmationDecision::ReturnForFix {
            return ConfirmationMatrixDecision::FixAndRevalidate;
        }
        if Self::is_matrix_confirmed(confirmations, required_scopes) {
            ConfirmationMatrixDecision::StartApply
        } else {
            ConfirmationMatrixDecision::AwaitOtherConfirmations
        }
    }

    /// 只有本轮矩阵没有任何退回且每个必需范围均已有确认时才允许应用；未登记范围不计入确认覆盖。
    pub fn is_matrix_confirmed(
        confirmations: &[Self],
        required_scopes: &BTreeSet<ConfirmationScope>,
    ) -> bool {
        Self::matrix_confirmed(confirmations.iter(), required_scopes)
    }

    /// 只用该规则与试算版本下未失效的事实核对确认覆盖；规则先去首尾空白，存在退回或缺少必确认范围均不得应用。
    pub fn is_trial_confirmed(
        confirmations: &[Self],
        trial_version: u32,
        rule_version: &str,
        required_scopes: &BTreeSet<ConfirmationScope>,
    ) -> bool {
        Self::matrix_confirmed(
            confirmations.iter().filter(|item| {
                item.trial_version == trial_version
                    && item.import_rule_version == rule_version.trim()
                    && item.status != ConfirmationStatus::Invalidated
            }),
            required_scopes,
        )
    }

    // 汇总调用方已筛选的确认引用；遇到退回立即否决，否则只用合法范围的已确认记录检查是否覆盖所有责任范围。
    fn matrix_confirmed<'a>(
        confirmations: impl Iterator<Item = &'a Self>,
        required_scopes: &BTreeSet<ConfirmationScope>,
    ) -> bool {
        let mut confirmed = BTreeSet::new();
        for item in confirmations {
            if item.status == ConfirmationStatus::Rejected {
                return false;
            }
            if item.status == ConfirmationStatus::Confirmed
                && let Ok(scope) = ConfirmationScope::parse(&item.confirmation_scope)
            {
                confirmed.insert(scope);
            }
        }

        required_scopes.iter().all(|scope| confirmed.contains(scope))
    }

    /// 生成带试算版本、按范围稳定排序的状态摘要，供批次展示，不公开确认人身份。
    pub fn matrix_summary(trial_version: u32, confirmations: &[Self]) -> String {
        let states = confirmations
            .iter()
            .map(|item| (item.confirmation_scope.as_str(), item.status.as_str()))
            .collect::<BTreeMap<_, _>>();
        let encoded = states
            .into_iter()
            .map(|(scope, status)| format!("{scope}={status}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("trial={trial_version};{encoded}")
    }

    /// 判断现有记录是否应被较高试算版本替代；只替换尚待确认的旧记录，已完成决定保持原事实。
    pub fn is_replaced_by(&self, replacement_trial_version: u32) -> bool {
        self.status == ConfirmationStatus::Pending && self.trial_version < replacement_trial_version
    }

    /// 为待确认事实登记同意或退回、实际操作人和时间。`RETURN_FOR_FIX` 必须有原因代码，`CONFIRM_SCOPE` 意见可选；非待确认、退回缺原因或确认人和说明文本不合法时返回错误。
    /// 正式 `COMPLETE_IMPORT_BUSINESS_CONFIRMATION` 命令须在同一事务写入 `workflow_action` 并完成任务（`COMPLETED`），实体方法不负责联动。
    pub fn decide(
        &mut self,
        decision: ConfirmationDecision,
        decided_by: impl Into<String>,
        decided_at: Instant,
        reason_code: Option<String>,
        comment: Option<String>,
    ) -> Result<()> {
        if self.status != ConfirmationStatus::Pending {
            return Err(Error::from(format!("仅待确认事实可决策，当前状态：{}", self.status.label())));
        }

        let status = match decision {
            ConfirmationDecision::ConfirmScope => ConfirmationStatus::Confirmed,
            ConfirmationDecision::ReturnForFix => {
                let reason_code = normalize_required_text(
                    reason_code.unwrap_or_default(),
                    "退回原因代码不能为空",
                    REASON_CODE_MAX_LEN,
                    "退回原因代码过长",
                )?;
                self.reason_code = Some(reason_code);
                ConfirmationStatus::Rejected
            },
        };

        self.decision = Some(decision);
        self.comment = normalize_optional_text(comment, "意见", COMMENT_MAX_LEN)?;
        self.decided_by = Some(normalize_required_text(
            decided_by.into(),
            "确认人不能为空",
            DECIDER_MAX_LEN,
            "确认人过长",
        )?);
        self.decided_at = Some(decided_at);
        self.status = status;
        Ok(())
    }

    /// 在新试算、规则或清单替换后，使未完成确认失效并记录替代者及失效时间。退回后不转派或立即创建后继任务，须修复并生成新 `trial_version` 才建立新确认和任务。
    /// 已失效时可重复写入并覆盖替代者与时间；已确认或已退回则返回 `InvalidStateTransition`。
    pub fn invalidate(
        &mut self,
        replacement_confirmation_id: erp_core::ids::LegacyImportConfirmationId,
        invalidated_at: Instant,
    ) -> Result<()> {
        ensure_transition(self.status, ConfirmationStatus::Invalidated)?;
        self.status = ConfirmationStatus::Invalidated;
        self.invalidated_at = Some(invalidated_at);
        self.replacement_confirmation_id = Some(replacement_confirmation_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::ensure_transition;
    use erp_core::ids::{LegacyImportConfirmationId, WorkItemId};

    use super::*;

    fn confirmation_data() -> LegacyImportConfirmationData {
        LegacyImportConfirmationData {
            batch_id: LegacyImportBatchId::new("batch-1"),
            confirmation_scope: " 销售 ".to_string(),
            owner_role: " 销售领导 ".to_string(),
            batch_version: 1,
            trial_version: 2,
            import_rule_version: " v1 ".to_string(),
            work_item_id: WorkItemId::new("wi-1"),
        }
    }

    #[test]
    fn new_trims_and_starts_pending() {
        let confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-1"), confirmation_data())
                .unwrap();

        assert_eq!(confirmation.confirmation_scope, "销售");
        assert_eq!(confirmation.owner_role, "销售领导");
        assert_eq!(confirmation.import_rule_version, "v1");
        assert_eq!(confirmation.status, ConfirmationStatus::Pending);
        assert!(confirmation.decision.is_none());
        assert_eq!(confirmation.work_item_id, WorkItemId::new("wi-1"));
    }

    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_scope =
            LegacyImportConfirmationData { confirmation_scope: "   ".to_string(), ..confirmation_data() };
        assert!(LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-2"), empty_scope).is_err());

        let overlong_role =
            LegacyImportConfirmationData { owner_role: "r".repeat(ROLE_MAX_LEN + 1), ..confirmation_data() };
        assert!(
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-3"), overlong_role).is_err()
        );
    }

    #[test]
    fn confirm_scope_records_decision_and_audit() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-4"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);

        confirmation
            .decide(
                ConfirmationDecision::ConfirmScope,
                " 运营-张三 ".to_string(),
                at,
                None,
                Some(" 范围无误 ".to_string()),
            )
            .unwrap();

        assert_eq!(confirmation.status, ConfirmationStatus::Confirmed);
        assert_eq!(confirmation.decision, Some(ConfirmationDecision::ConfirmScope));
        assert_eq!(confirmation.decided_by.as_deref(), Some("运营-张三"));
        assert_eq!(confirmation.decided_at, Some(at));
        assert_eq!(confirmation.comment.as_deref(), Some("范围无误"));
        assert!(confirmation.reason_code.is_none());
    }

    #[test]
    fn reject_requires_reason_code() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-5"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);

        assert!(
            confirmation
                .decide(ConfirmationDecision::ReturnForFix, "财务".to_string(), at, None, None)
                .is_err(),
            "退回原因必填"
        );

        confirmation
            .decide(
                ConfirmationDecision::ReturnForFix,
                "财务".to_string(),
                at,
                Some(" 客户资料缺失 ".to_string()),
                None,
            )
            .unwrap();
        assert_eq!(confirmation.status, ConfirmationStatus::Rejected);
        assert_eq!(confirmation.decision, Some(ConfirmationDecision::ReturnForFix));
        assert_eq!(confirmation.reason_code.as_deref(), Some("客户资料缺失"));
    }

    #[test]
    fn completed_confirmation_cannot_be_decided_again() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-6"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);
        confirmation.decide(ConfirmationDecision::ConfirmScope, "运营".to_string(), at, None, None).unwrap();

        assert!(
            confirmation
                .decide(ConfirmationDecision::ConfirmScope, "运营".to_string(), at, None, None)
                .is_err(),
            "已完成确认不可重复决策"
        );
        assert!(
            confirmation.invalidate(LegacyImportConfirmationId::new("c-7"), at).is_err(),
            "已完成确认不可失效"
        );
    }

    #[test]
    fn pending_confirmation_invalidates_with_replacement() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-8"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);

        confirmation.invalidate(LegacyImportConfirmationId::new("c-9"), at).unwrap();
        assert_eq!(confirmation.status, ConfirmationStatus::Invalidated);
        assert_eq!(confirmation.invalidated_at, Some(at));
        assert_eq!(confirmation.replacement_confirmation_id, Some(LegacyImportConfirmationId::new("c-9")));
    }

    #[test]
    fn status_machine_is_directed() {
        assert!(ensure_transition(ConfirmationStatus::Pending, ConfirmationStatus::Confirmed).is_ok());
        assert!(ensure_transition(ConfirmationStatus::Pending, ConfirmationStatus::Rejected).is_ok());
        assert!(ensure_transition(ConfirmationStatus::Pending, ConfirmationStatus::Invalidated).is_ok());
        assert!(
            ensure_transition(ConfirmationStatus::Rejected, ConfirmationStatus::Pending).is_err(),
            "退回为终态，修复后创建新试算的确认事实"
        );
        assert!(ensure_transition(ConfirmationStatus::Invalidated, ConfirmationStatus::Confirmed).is_err());
    }

    #[test]
    fn status_and_decision_serde_use_stable_codes() {
        assert_eq!(serde_json::to_string(&ConfirmationStatus::Pending).unwrap(), "\"PENDING\"");
        assert_eq!(serde_json::to_string(&ConfirmationStatus::Invalidated).unwrap(), "\"INVALIDATED\"");
        assert_eq!(serde_json::to_string(&ConfirmationDecision::ReturnForFix).unwrap(), "\"RETURN_FOR_FIX\"");
        assert_eq!(ConfirmationStatus::Rejected.label(), "已退回");
        assert_eq!(ConfirmationDecision::ConfirmScope.label(), "确认本范围");
    }

    #[test]
    fn scope_registry_derives_roles_and_required_matrix() {
        assert_eq!(ConfirmationScope::parse(" sales ").unwrap(), ConfirmationScope::Sales);
        assert_eq!(ConfirmationScope::Finance.owner_role(), "role-finance");
        let scopes = ConfirmationScope::required_for_object_set(
            "CUSTOMER;SUPPLIER|CARD_CATEGORY/期初库存、CARD_OPENING_AR",
        )
        .unwrap();
        assert_eq!(
            scopes,
            BTreeSet::from([
                ConfirmationScope::Sales,
                ConfirmationScope::Procurement,
                ConfirmationScope::Operations,
                ConfirmationScope::Warehouse,
                ConfirmationScope::Finance,
            ])
        );
        assert!(ConfirmationScope::required_for_object_set("UNKNOWN").is_err());
    }

    #[test]
    fn matrix_rules_own_snapshot_version_and_next_decision() {
        let mut sales = LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("c-sales"),
            LegacyImportConfirmationData { confirmation_scope: "SALES".to_string(), ..confirmation_data() },
        )
        .unwrap();
        assert!(sales.is_pending());
        sales
            .decide(
                ConfirmationDecision::ConfirmScope,
                "sales-user",
                Instant::from_unix_secs(1_700_000_000),
                None,
                None,
            )
            .unwrap();

        let required = BTreeSet::from([ConfirmationScope::Sales]);
        assert_eq!(
            LegacyImportConfirmation::matrix_decision(
                ConfirmationDecision::ConfirmScope,
                &[sales.clone()],
                &required,
            ),
            ConfirmationMatrixDecision::StartApply
        );

        assert_eq!(
            LegacyImportConfirmation::subject_version(1, 2, " rule-1 "),
            "batch:1;trial:2;rule:rule-1"
        );
        assert!(sales.belongs_to_snapshot(1, 2, "v1"));
        assert_eq!(LegacyImportConfirmation::latest_active_trial(&[sales.clone()], " v1 "), Some(2));
        assert!(LegacyImportConfirmation::is_trial_confirmed(&[sales.clone()], 2, "v1", &required,));
        assert!(LegacyImportConfirmation::matrix_summary(2, &[sales]).contains("SALES=CONFIRMED"));

        assert!(LegacyImportConfirmation::ensure_trial_snapshot(&[], u32::MAX, u32::MAX, "v1").is_err());
    }

    #[test]
    fn trial_snapshot_preserves_consistency_error_before_rejection() {
        let pending =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-snapshot"), confirmation_data())
                .unwrap();
        LegacyImportConfirmation::ensure_trial_snapshot(&[], 1, 2, "v1").unwrap();
        LegacyImportConfirmation::ensure_trial_snapshot(std::slice::from_ref(&pending), 1, 2, "v1").unwrap();

        let mut rejected = pending.clone();
        rejected
            .decide(
                ConfirmationDecision::ReturnForFix,
                "sales-user",
                Instant::from_unix_secs(1_700_000_000),
                Some("DATA_MISMATCH".into()),
                None,
            )
            .unwrap();
        assert!(matches!(
            LegacyImportConfirmation::ensure_trial_snapshot(std::slice::from_ref(&rejected), 1, 2, "v1"),
            Err(Error::LogicError(message)) if message == "当前试算已被退回，必须修复并生成新试算版本"
        ));

        for inconsistent in [
            LegacyImportConfirmation { batch_version: 2, ..pending.clone() },
            LegacyImportConfirmation { import_rule_version: "v2".into(), ..pending },
        ] {
            for items in [[rejected.clone(), inconsistent.clone()], [inconsistent.clone(), rejected.clone()]]
            {
                assert!(matches!(
                    LegacyImportConfirmation::ensure_trial_snapshot(&items, 1, 2, "v1"),
                    Err(Error::LogicError(message)) if message == "试算所用的导入批次或规则版本已变化，请重新试算"
                ));
            }
        }
    }

    #[test]
    fn matrix_confirmation_rejects_incomplete_or_returned_scopes() {
        let mut sales = LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("c-matrix-sales"),
            LegacyImportConfirmationData { confirmation_scope: "SALES".to_string(), ..confirmation_data() },
        )
        .unwrap();
        sales
            .decide(
                ConfirmationDecision::ConfirmScope,
                "sales-user",
                Instant::from_unix_secs(1_700_000_000),
                None,
                None,
            )
            .unwrap();
        let required = BTreeSet::from([ConfirmationScope::Sales, ConfirmationScope::Finance]);
        assert!(!LegacyImportConfirmation::is_matrix_confirmed(&[sales.clone()], &required,));

        let mut finance = LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("c-matrix-finance"),
            LegacyImportConfirmationData {
                confirmation_scope: "FINANCE".to_string(),
                work_item_id: WorkItemId::new("wi-finance"),
                ..confirmation_data()
            },
        )
        .unwrap();
        finance
            .decide(
                ConfirmationDecision::ReturnForFix,
                "finance-user",
                Instant::from_unix_secs(1_700_000_001),
                Some("AMOUNT_MISMATCH".to_string()),
                None,
            )
            .unwrap();
        assert!(
            !LegacyImportConfirmation::is_matrix_confirmed(&[sales.clone(), finance.clone()], &required,)
        );
        assert!(!LegacyImportConfirmation::is_trial_confirmed(&[sales, finance], 2, "v1", &required,));
    }
}
