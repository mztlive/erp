use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt;
use std::ops::Deref;

use erp_core::{Error, Result};
pub use erp_permissions::Permission;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const MAX_ID_LEN: usize = 128;

/// 用于角色记录与授权策略关联的规范角色 ID。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RoleId(
    // 保留原大小写的非空 ASCII 标识，构造时统一校验。
    String,
);

impl RoleId {
    /// 裁剪两端空白后解析角色 ID；空值、超过 128 字符或含非法字符时返回错误。
    /// 仅接受 ASCII 字母、数字及 `_`、`-`、`.`。
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(Error::from("角色ID不能为空"));
        }
        if value.len() > MAX_ID_LEN {
            return Err(Error::from("角色ID长度不能超过128个字符"));
        }
        if !value.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')) {
            return Err(Error::from("角色ID包含非法字符"));
        }

        Ok(Self(value.to_string()))
    }

    /// 借用规范角色 ID 文本，供角色查找和策略键构造使用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoleId {
    // 直接输出规范角色 ID，便于拼接策略主体键与展示。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for RoleId {
    // 为接受字符串借用的接口提供规范角色 ID，无需复制。
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for RoleId {
    // 允许集合按规范角色 ID 的字符串切片查找键。
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Serialize for RoleId {
    // 将角色 ID 写成规范字符串，并直接传递序列化错误。
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RoleId {
    // 反序列化时要求字符串并经 `RoleId::parse` 规范化，类型或内容非法均报错。
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// 表示角色选择或角色绑定的一组规范 ID，序列化为去重且保序的数组。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<RoleId>", into = "Vec<RoleId>")]
pub struct RoleIdSet(
    // 按构造时确定的顺序保存首次出现的角色 ID。
    Vec<RoleId>,
);

impl RoleIdSet {
    /// 逐项规范化角色 ID，去重并保留首次出现的顺序；接受空集合，任一项非法时报错。
    pub fn parse<I, S>(role_ids: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let values = role_ids.into_iter().map(RoleId::parse).collect::<Result<Vec<_>>>()?;
        Ok(Self::from(values))
    }

    /// 为至少需要一个角色的用例解析角色 ID，去重并保留首次出现的顺序。
    /// 空集合或任一角色 ID 非法时返回错误。
    pub fn parse_non_empty<I, S>(role_ids: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let role_ids = Self::parse(role_ids)?;
        role_ids.ensure_non_empty()?;
        Ok(role_ids)
    }

    /// 在业务要求选角色时检查已解析的集合，空集合报须至少选择一个角色。
    pub fn ensure_non_empty(&self) -> Result<()> {
        if self.is_empty() {
            return Err(Error::from("至少选择一个角色"));
        }
        Ok(())
    }

    /// 读取 Casbin 主体键时只提取 `role:` 后缀，按角色 ID 排序去重；没有角色键则返回空集。
    /// 持久化后缀必须已规范化且合法，否则返回错误。
    pub fn from_casbin_role_keys<I, S>(role_keys: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut role_ids = Vec::new();
        for key in role_keys {
            let Some(raw) = key.as_ref().strip_prefix("role:") else {
                continue;
            };
            let id = RoleId::parse(raw)?;
            if id.as_str() != raw {
                return Err(Error::from("持久化角色主体不是规范身份"));
            }
            role_ids.push(id);
        }

        role_ids.sort();
        Ok(Self::from(role_ids))
    }
}

impl From<Vec<RoleId>> for RoleIdSet {
    // 收拢已校验的角色 ID，保留每个 ID 首次出现的位置。
    fn from(values: Vec<RoleId>) -> Self {
        let mut seen = HashSet::new();
        Self(values.into_iter().filter(|id| seen.insert(id.clone())).collect())
    }
}

impl From<RoleIdSet> for Vec<RoleId> {
    // 交出内部有序数组，供序列化和角色列表返回使用。
    fn from(ids: RoleIdSet) -> Self {
        ids.0
    }
}

impl Deref for RoleIdSet {
    type Target = [RoleId];
    // 提供只读切片，使遍历或查询保留集合的顺序与去重约束。
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// 用于比较、合并和补齐授权策略的去重权限集合。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PermissionSet(
    // 按资源、动作依次排序的权限项，相同权限只保留一项。
    Vec<Permission>,
);

impl PermissionSet {
    /// 将已解析的权限去重，再按资源和动作排序，得到顺序确定的集合。
    pub fn new(permissions: impl IntoIterator<Item = Permission>) -> Self {
        let mut permissions = permissions.into_iter().collect::<Vec<_>>();
        permissions.sort_unstable();
        permissions.dedup();
        Self(permissions)
    }

    /// 检查每项所需权限是否都被当前集合中的某一项覆盖，沿用 [`Permission::covers`] 的通配规则。
    pub fn covers(&self, required: &Self) -> bool {
        required.0.iter().all(|required| self.covers_one(required))
    }

    /// 检查是否存在能覆盖所需权限的授予项，例如 `customer:*` 覆盖 `customer:list`。
    pub fn covers_one(&self, required: &Permission) -> bool {
        self.0.iter().any(|permission| permission.covers(required))
    }

    /// 从目标集合筛出尚未覆盖的权限，保留去重排序；精确或通配授予覆盖的项均计为已有。
    /// 只按目标项判断缺失，全部覆盖时返回空集。
    pub fn missing_from(&self, desired: &Self) -> Self {
        Self::new(desired.0.iter().filter(|required| !self.covers_one(required)).cloned())
    }

    /// 合并两侧已有权限，去重后按资源和动作排序，保留额外授予项。
    pub fn union(&self, other: &Self) -> Self {
        Self::new(self.0.iter().cloned().chain(other.0.iter().cloned()))
    }

    /// 补齐种子角色尚未覆盖的推荐权限，保留管理员额外授予的权限；无需补齐时返回 `None`。
    pub fn with_missing(&self, desired: &Self) -> Option<Self> {
        let missing = self.missing_from(desired);
        if missing.is_empty() { None } else { Some(self.union(&missing)) }
    }

    /// 判断集合是否没有任何权限项，可用于跳过空的权限增量。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 按确定顺序借用权限切片，供比较和遍历。
    pub fn as_slice(&self) -> &[Permission] {
        &self.0
    }

    /// 交出内部去重排序后的权限数组，供调用方接收或继续合并。
    pub fn into_vec(self) -> Vec<Permission> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Permission, PermissionSet, RoleId, RoleIdSet};

    // 验证角色 ID 数组读入时裁剪并去重，写出时维持规范数组，非法 ID 读入失败。
    #[test]
    fn role_set_wire_format_is_a_normalized_array() {
        let ids: RoleIdSet = serde_json::from_str(r#"[" role-a ","role-a","role-b"]"#).unwrap();
        assert_eq!(ids.iter().map(RoleId::as_str).collect::<Vec<_>>(), ["role-a", "role-b"]);
        assert_eq!(serde_json::to_string(&ids).unwrap(), r#"["role-a","role-b"]"#);
        assert!(serde_json::from_str::<RoleIdSet>(r#"["bad/id"]"#).is_err());
    }

    // 验证裁剪空白后去重，并保留角色 ID 首次出现的顺序。
    #[test]
    fn role_ids_should_trim_deduplicate_and_preserve_order() {
        let role_ids = RoleIdSet::parse([" role-a ", "role-a", "role-b"]).unwrap();
        assert_eq!(role_ids.iter().map(RoleId::as_str).collect::<Vec<_>>(), vec!["role-a", "role-b"]);
    }

    // 验证要求至少一个角色的解析入口拒绝空集合。
    #[test]
    fn role_ids_should_reject_empty_required_collection() {
        assert!(RoleIdSet::parse_non_empty(Vec::<String>::new()).is_err());
    }

    // 验证只提取角色主体并排序去重，同时拒绝空、非法或未规范化的角色键后缀。
    #[test]
    fn casbin_role_keys_are_filtered_and_sorted() {
        let role_ids =
            RoleIdSet::from_casbin_role_keys(["user:ignored", "role:role-b", "role:role-a", "role:role-b"])
                .unwrap();

        assert_eq!(role_ids.iter().map(RoleId::as_str).collect::<Vec<_>>(), vec!["role-a", "role-b"]);

        assert!(RoleIdSet::from_casbin_role_keys(["role:bad/id"]).is_err());
        assert!(RoleIdSet::from_casbin_role_keys(["role: role-a"]).is_err());
        assert!(RoleIdSet::from_casbin_role_keys(["role:role-a "]).is_err());
        assert!(RoleIdSet::from_casbin_role_keys(["role:"]).is_err());
    }

    // 验证权限集合去重，按精确和通配授权判断覆盖，并区分空集合。
    #[test]
    fn permission_set_should_normalize_and_enforce_subset() {
        let actor = PermissionSet::new([
            Permission::new("role", "read"),
            Permission::parse("customer:*").unwrap(),
            Permission::new("customer", "*"),
        ]);
        let allowed = PermissionSet::new([
            Permission::parse("customer:update").unwrap(),
            Permission::parse("role:read").unwrap(),
        ]);
        let elevated = PermissionSet::new([Permission::parse("role:delete").unwrap()]);

        assert_eq!(actor.as_slice(), [Permission::new("customer", "*"), Permission::new("role", "read")]);
        assert!(actor.covers(&allowed));
        assert!(!actor.covers(&elevated));
        assert!(actor.covers_one(&Permission::parse("customer:update").unwrap()));
        assert!(!actor.covers_one(&Permission::parse("role:delete").unwrap()));
        assert!(!actor.is_empty());
        assert!(PermissionSet::default().is_empty());
    }

    // 验证缺失清单只包含目标中未被当前权限覆盖的项，已有通配可满足具体动作。
    #[test]
    fn permission_set_should_report_uncovered_desired_permissions() {
        let current = PermissionSet::new([
            Permission::parse("customer:*").unwrap(),
            Permission::parse("custom:extra").unwrap(),
        ]);
        let desired = PermissionSet::new([
            Permission::parse("customer:list").unwrap(),
            Permission::parse("sales_order:create").unwrap(),
        ]);
        let missing = current.missing_from(&desired);

        assert_eq!(missing.as_slice(), [Permission::parse("sales_order:create").unwrap()]);
        assert!(current.missing_from(&current).is_empty());
    }

    // 验证补齐及合并所得权限有序且保留额外授权，无缺失时无需补齐。
    #[test]
    fn permission_set_should_append_missing_permissions_without_dropping_extras() {
        let current = PermissionSet::new([
            Permission::parse("customer:list").unwrap(),
            Permission::parse("custom:extra").unwrap(),
        ]);
        let desired = PermissionSet::new([
            Permission::parse("customer:list").unwrap(),
            Permission::parse("customer:create").unwrap(),
        ]);
        let merged = current.with_missing(&desired).expect("应补齐缺失权限");

        assert_eq!(
            merged.as_slice(),
            [
                Permission::parse("custom:extra").unwrap(),
                Permission::parse("customer:create").unwrap(),
                Permission::parse("customer:list").unwrap(),
            ]
        );
        assert!(current.with_missing(&current).is_none());
        assert_eq!(
            current.union(&desired).as_slice(),
            [
                Permission::parse("custom:extra").unwrap(),
                Permission::parse("customer:create").unwrap(),
                Permission::parse("customer:list").unwrap(),
            ]
        );
    }
}
