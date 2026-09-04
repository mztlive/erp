//! 系统管理员能力配置变更集（INT-E29：`UpdateSupplierCapabilitiesCommand` 的纯领域形态）。
//!
//! `UpdateSupplierCapabilitiesCommand` 携带的 raw vector（`capability_changes`）与
//! raw map（`expected_capability_versions`）原先由 Service 逐项解释：重复 code 静默
//! 依赖 `ensure_unique`、版本 map 缺 key 才报错、多余 key 被忽略、新能力的
//! `version=0` 与 `disabled` 起始状态散落在持久化循环里。本 VO 独占这组集合规则，
//! Service 只把已校验变更集、实时版本与采购确认送入编排。

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use super::capability::{SupplierApiCapability, SupplierApiCapabilityCode};
use super::governance::ensure_unique_capability_codes;

/// 单次能力变更允许的最大条数（与 wire DTO 的 `1..=10` 上限一致）。
const MAX_CHANGES: usize = 10;

/// 到达 VO 之前的单条能力变更输入（Service 由 wire DTO 逐条映射，不解释规则）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityChangeInput {
    /// 固定能力代码。
    pub code: SupplierApiCapabilityCode,
    /// 目标启停状态。
    pub enabled: bool,
    /// 供应商能力限制快照。
    pub constraint_snapshot: Option<String>,
}

/// 已校验的单条能力变更（携带期望版本；新增/更新分类待领域快照确认）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingCapabilityChange {
    /// 固定能力代码。
    pub code: SupplierApiCapabilityCode,
    /// 目标启停状态。
    pub enabled: bool,
    /// 供应商能力限制快照。
    pub constraint_snapshot: Option<String>,
    /// 该能力声明的期望版本。
    pub expected_version: u64,
}

/// 已分类的单条能力变更（新增/更新已按连接下既有能力确认）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatedCapabilityChange {
    /// 固定能力代码。
    pub code: SupplierApiCapabilityCode,
    /// 目标启停状态。
    pub enabled: bool,
    /// 供应商能力限制快照。
    pub constraint_snapshot: Option<String>,
    /// 该能力声明的期望版本；新增能力固定为 `0`。
    pub expected_version: u64,
    /// 是否为连接下新增的能力声明。
    pub is_new: bool,
}

/// 能力变更集合校验拒绝原因（强类型，Service 按变体映射为既有错误语义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityChangeSetRejection {
    /// 变更清单为空或超过上限。
    EmptyOrTooMany,
    /// 变更 code 出现重复。
    DuplicateCodes,
    /// 版本 map 缺少某条变更的期望版本。
    MissingExpectedVersion(&'static str),
    /// 版本 map 存在变更清单之外的多余 key。
    UnexpectedExpectedVersion(String),
    /// 新能力的期望版本不为 `0`。
    NewCapabilityVersionMustBeZero(&'static str),
    /// 新能力要求以启用状态登记。
    NewCapabilityMustStartDisabled(&'static str),
}

impl std::fmt::Display for CapabilityChangeSetRejection {
    /// 返回与历史 Service 内联校验一致的中文说明。
    ///
    /// # 参数
    /// * `f` - 格式化目标
    ///
    /// # 返回
    /// 写入用户可读的拒绝说明。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyOrTooMany => write!(f, "能力变更必须为1到10条"),
            Self::DuplicateCodes => write!(f, "能力变更代码不能重复"),
            Self::MissingExpectedVersion(key) => write!(f, "缺少能力 {key} 的期望版本"),
            Self::UnexpectedExpectedVersion(key) => write!(f, "能力版本映射存在多余能力 {key}"),
            Self::NewCapabilityVersionMustBeZero(key) => {
                write!(f, "新能力 {key} 的期望版本必须为0")
            }
            Self::NewCapabilityMustStartDisabled(_) => {
                write!(f, "新能力必须先以停用状态登记，再由采购确认后启用")
            }
        }
    }
}

impl std::error::Error for CapabilityChangeSetRejection {}

impl From<CapabilityChangeSetRejection> for crate::errors::Error {
    /// 将集合拒绝转换为实体层通用错误（保留展示文本）。
    ///
    /// # 参数
    /// * `rejection` - 集合校验拒绝原因
    ///
    /// # 返回
    /// 携带同文本的实体层错误。
    fn from(rejection: CapabilityChangeSetRejection) -> Self {
        Self::from(rejection.to_string())
    }
}

/// 形态已校验的能力变更集（保持调用方传入顺序，逐条携带期望版本）。
///
/// 集合形态（非空上限、code 唯一、版本 map keys 精确相等）在构造时固定；
/// 新增/更新分类依赖连接下既有能力快照，由 [`CapabilityChangeSet::classify`]
/// 在调用方事务视图内确认。两步拆分保证集合形态错误先于幂等回放判定，
/// 与历史 Service 校验顺序一致。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityChangeSet {
    /// 形态已校验变更（与输入顺序一致）。
    changes: Vec<PendingCapabilityChange>,
}

/// 新增/更新已分类的能力变更集（保持调用方传入顺序）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassifiedCapabilityChangeSet {
    /// 已分类变更（与输入顺序一致）。
    changes: Vec<ValidatedCapabilityChange>,
}

impl CapabilityChangeSet {
    /// 校验变更清单形态与版本 map keys 精确相等。
    ///
    /// 依次固定：清单非空且不超过上限、code 唯一、版本 map keys 与变更 code
    /// 精确相等（缺 key 与多余 key 均拒绝）。新能力规则与实时版本冲突仍待
    /// [`CapabilityChangeSet::classify`] 与 Service 判定，本方法不读取数据库。
    ///
    /// # 参数
    /// * `inputs` - 待校验的单条变更输入（保持调用方顺序）
    /// * `expected_versions` - 以能力代码字符串为 key 的期望版本映射
    ///
    /// # 返回
    /// 返回保持输入顺序的形态已校验变更集。
    ///
    /// # 错误
    /// 当清单为空/超限、code 重复、版本 map 缺 key 或多余 key 时返回
    /// [`CapabilityChangeSetRejection`]。
    ///
    /// # 约束
    /// 纯内存校验；不访问 MongoDB、时钟、ID 生成器或密钥。
    pub fn new(
        inputs: Vec<CapabilityChangeInput>,
        expected_versions: &BTreeMap<String, u64>,
    ) -> Result<Self, CapabilityChangeSetRejection> {
        if inputs.is_empty() || inputs.len() > MAX_CHANGES {
            return Err(CapabilityChangeSetRejection::EmptyOrTooMany);
        }
        ensure_unique_capability_codes(inputs.iter().map(|input| input.code))
            .map_err(|_| CapabilityChangeSetRejection::DuplicateCodes)?;
        let mut changes = Vec::with_capacity(inputs.len());
        for input in inputs {
            let key = input.code.as_str();
            let expected = expected_versions
                .get(key)
                .copied()
                .ok_or(CapabilityChangeSetRejection::MissingExpectedVersion(key))?;
            changes.push(PendingCapabilityChange {
                code: input.code,
                enabled: input.enabled,
                constraint_snapshot: input.constraint_snapshot,
                expected_version: expected,
            });
        }
        let change_codes: BTreeSet<String> = changes
            .iter()
            .map(|change| change.code.as_str().to_string())
            .collect();
        for key in expected_versions.keys() {
            if !change_codes.contains(key) {
                return Err(CapabilityChangeSetRejection::UnexpectedExpectedVersion(
                    key.clone(),
                ));
            }
        }
        Ok(Self { changes })
    }

    /// 按连接下既有能力确认新增/更新分类并校验新能力规则。
    ///
    /// 新能力要求期望版本为 `0` 且以停用状态登记；既有能力的实时版本冲突与
    /// 采购确认覆盖仍由 Service 在调用方事务内判定。
    ///
    /// # 参数
    /// * `existing` - 连接下既有能力声明（调用方事务视图，仅读取代码集合）
    ///
    /// # 返回
    /// 返回保持输入顺序的已分类变更集。
    ///
    /// # 错误
    /// 当新能力版本不为 `0` 或新能力要求启用时返回
    /// [`CapabilityChangeSetRejection`]。
    ///
    /// # 约束
    /// 纯内存校验；不访问 MongoDB、时钟、ID 生成器或密钥。
    pub fn classify(
        self,
        existing: &[SupplierApiCapability],
    ) -> Result<ClassifiedCapabilityChangeSet, CapabilityChangeSetRejection> {
        let existing_codes: HashSet<SupplierApiCapabilityCode> = existing
            .iter()
            .map(|capability| capability.capability_code)
            .collect();
        let mut changes = Vec::with_capacity(self.changes.len());
        for pending in self.changes {
            let key = pending.code.as_str();
            let is_new = !existing_codes.contains(&pending.code);
            if is_new {
                if pending.expected_version != 0 {
                    return Err(CapabilityChangeSetRejection::NewCapabilityVersionMustBeZero(key));
                }
                if pending.enabled {
                    return Err(CapabilityChangeSetRejection::NewCapabilityMustStartDisabled(key));
                }
            }
            changes.push(ValidatedCapabilityChange {
                code: pending.code,
                enabled: pending.enabled,
                constraint_snapshot: pending.constraint_snapshot,
                expected_version: pending.expected_version,
                is_new,
            });
        }
        Ok(ClassifiedCapabilityChangeSet { changes })
    }

    /// 返回形态已校验变更（与输入顺序一致）。
    ///
    /// # 返回
    /// 返回逐条形态已校验变更的只读视图。
    pub fn changes(&self) -> &[PendingCapabilityChange] {
        &self.changes
    }

    /// 返回变更条数。
    ///
    /// # 返回
    /// 返回已校验变更的数量。
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// 判断变更集是否为空（构造器保证恒为 `false`）。
    ///
    /// # 返回
    /// 变更集为空时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

impl ClassifiedCapabilityChangeSet {
    /// 返回已分类变更（与输入顺序一致）。
    ///
    /// # 返回
    /// 返回逐条已分类变更的只读视图。
    pub fn changes(&self) -> &[ValidatedCapabilityChange] {
        &self.changes
    }

    /// 返回变更条数。
    ///
    /// # 返回
    /// 返回已分类变更的数量。
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// 判断变更集是否为空（分类器保证恒为 `false`）。
    ///
    /// # 返回
    /// 变更集为空时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{CapabilityChangeInput, CapabilityChangeSet, CapabilityChangeSetRejection, MAX_CHANGES};
    use crate::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};
    use crate::supplier_api::{
        SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiCapabilityData,
        SupplierApiCapabilityStatus,
    };
    use std::collections::BTreeMap;

    /// 构造既有能力声明测试夹具。
    fn existing_capability(code: SupplierApiCapabilityCode) -> SupplierApiCapability {
        SupplierApiCapability::new(
            SupplierApiCapabilityId::new(format!("cap-{}", code.as_str())),
            SupplierApiCapabilityData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_code: code,
                status: SupplierApiCapabilityStatus::Disabled,
                constraint_snapshot: None,
            },
        )
        .unwrap()
    }

    /// 构造单条变更输入测试夹具。
    fn change(code: SupplierApiCapabilityCode, enabled: bool) -> CapabilityChangeInput {
        CapabilityChangeInput {
            code,
            enabled,
            constraint_snapshot: None,
        }
    }

    /// 构造与变更清单精确匹配的版本映射测试夹具。
    fn versions(pairs: Vec<(&'static str, u64)>) -> BTreeMap<String, u64> {
        pairs
            .into_iter()
            .map(|(key, version)| (key.to_string(), version))
            .collect()
    }

    #[test]
    fn change_set_classifies_updates_and_creates_in_input_order() {
        let existing = vec![existing_capability(SupplierApiCapabilityCode::Order)];
        let set = CapabilityChangeSet::new(
            vec![
                change(SupplierApiCapabilityCode::Order, true),
                change(SupplierApiCapabilityCode::Product, false),
            ],
            &versions(vec![("order", 1), ("product", 0)]),
        )
        .unwrap()
        .classify(&existing)
        .unwrap();

        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
        assert!(!set.changes()[0].is_new);
        assert_eq!(set.changes()[0].expected_version, 1);
        assert!(set.changes()[0].enabled);
        assert!(set.changes()[1].is_new);
        assert_eq!(set.changes()[1].expected_version, 0);
    }

    #[test]
    fn change_set_rejects_duplicate_codes() {
        let rejection = CapabilityChangeSet::new(
            vec![
                change(SupplierApiCapabilityCode::Order, false),
                change(SupplierApiCapabilityCode::Order, true),
            ],
            &versions(vec![("order", 1)]),
        )
        .expect_err("重复 code 必须拒绝");
        assert_eq!(rejection, CapabilityChangeSetRejection::DuplicateCodes);
        assert_eq!(rejection.to_string(), "能力变更代码不能重复");
    }

    #[test]
    fn change_set_rejects_missing_and_unexpected_version_keys() {
        let missing = CapabilityChangeSet::new(
            vec![change(SupplierApiCapabilityCode::Order, false)],
            &versions(vec![]),
        )
        .expect_err("缺 key 必须拒绝");
        assert_eq!(
            missing,
            CapabilityChangeSetRejection::MissingExpectedVersion("order")
        );
        assert_eq!(missing.to_string(), "缺少能力 order 的期望版本");

        let unexpected = CapabilityChangeSet::new(
            vec![change(SupplierApiCapabilityCode::Order, false)],
            &versions(vec![("order", 1), ("product", 0)]),
        )
        .expect_err("多余 key 必须拒绝");
        assert_eq!(
            unexpected,
            CapabilityChangeSetRejection::UnexpectedExpectedVersion("product".to_string())
        );
    }

    #[test]
    fn change_set_rejects_new_capability_with_nonzero_version_or_enabled() {
        let version = CapabilityChangeSet::new(
            vec![change(SupplierApiCapabilityCode::Product, false)],
            &versions(vec![("product", 2)]),
        )
        .unwrap()
        .classify(&[])
        .expect_err("新能力期望版本不为 0 必须拒绝");
        assert_eq!(
            version,
            CapabilityChangeSetRejection::NewCapabilityVersionMustBeZero("product")
        );

        let enabled = CapabilityChangeSet::new(
            vec![change(SupplierApiCapabilityCode::Product, true)],
            &versions(vec![("product", 0)]),
        )
        .unwrap()
        .classify(&[])
        .expect_err("新能力启用必须拒绝");
        assert_eq!(
            enabled,
            CapabilityChangeSetRejection::NewCapabilityMustStartDisabled("product")
        );
    }

    #[test]
    fn change_set_rejects_empty_and_overlong_inputs() {
        assert_eq!(
            CapabilityChangeSet::new(vec![], &versions(vec![])).expect_err("空清单必须拒绝"),
            CapabilityChangeSetRejection::EmptyOrTooMany
        );
        let overlong: Vec<CapabilityChangeInput> = (0..=MAX_CHANGES)
            .map(|_| change(SupplierApiCapabilityCode::Order, false))
            .collect();
        assert_eq!(
            CapabilityChangeSet::new(overlong, &versions(vec![("order", 1)])).expect_err("超限清单必须拒绝"),
            CapabilityChangeSetRejection::EmptyOrTooMany
        );
    }

    #[test]
    fn change_set_rejection_converts_to_entity_error_without_io() {
        let error: crate::errors::Error = CapabilityChangeSetRejection::DuplicateCodes.into();
        assert_eq!(error.to_string(), "能力变更代码不能重复");
    }
}
