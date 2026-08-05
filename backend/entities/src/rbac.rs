use std::{borrow::Borrow, collections::HashSet, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::{Error, Result};

const MAX_ID_LEN: usize = 128;
const MAX_PERMISSION_PART_LEN: usize = 128;

/// 已校验的角色 ID。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RoleId(String);

impl RoleId {
    /// 解析并规范化角色 ID。
    ///
    /// # 错误
    /// 当 ID 为空、过长或包含非法字符时返回错误。
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(Error::from("角色ID不能为空"));
        }
        if value.len() > MAX_ID_LEN {
            return Err(Error::from("角色ID长度不能超过128个字符"));
        }
        if !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        {
            return Err(Error::from("角色ID包含非法字符"));
        }

        Ok(Self(value.to_string()))
    }

    /// 返回角色 ID 字符串。
    ///
    /// # 返回值
    /// 返回规范化后的角色 ID。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for RoleId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for RoleId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Serialize for RoleId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RoleId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// 已去空白、去重且保序的角色 ID 集合。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleIdSet(Vec<RoleId>);

impl RoleIdSet {
    /// 解析角色 ID 集合，允许空集合。
    ///
    /// # 错误
    /// 当任一角色 ID 非法时返回错误。
    pub fn parse<I, S>(role_ids: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut values = Vec::new();
        let mut seen = HashSet::new();
        for role_id in role_ids {
            let role_id = RoleId::parse(role_id)?;
            if seen.insert(role_id.clone()) {
                values.push(role_id);
            }
        }
        Ok(Self(values))
    }

    /// 解析非空角色 ID 集合。
    ///
    /// # 错误
    /// 当集合为空或任一角色 ID 非法时返回错误。
    pub fn parse_non_empty<I, S>(role_ids: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let role_ids = Self::parse(role_ids)?;
        if role_ids.0.is_empty() {
            return Err(Error::from("至少选择一个角色"));
        }
        Ok(role_ids)
    }

    /// 返回角色 ID 切片。
    ///
    /// # 返回值
    /// 返回内部角色 ID 的只读视图。
    pub fn as_slice(&self) -> &[RoleId] {
        &self.0
    }

    /// 转换为字符串集合。
    ///
    /// # 返回值
    /// 返回角色 ID 字符串集合。
    pub fn to_strings(&self) -> Vec<String> {
        self.0.iter().map(ToString::to_string).collect()
    }
}

/// 已规范化的 `resource:action` 权限。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Permission {
    resource: String,
    action: String,
}

impl Permission {
    /// 解析并校验权限字符串。
    ///
    /// # 错误
    /// 当权限不符合 `resource:action` 格式时返回错误。
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let normalized = value.as_ref().trim().to_ascii_lowercase();
        let Some((resource, action)) = normalized.split_once(':') else {
            return Err(Error::from("权限必须使用 resource:action 格式"));
        };
        if action.contains(':') {
            return Err(Error::from("权限只能包含一个冒号分隔符"));
        }
        Self::validate_part(resource, "权限资源", true)?;
        Self::validate_part(action, "权限动作", false)?;
        Ok(Self {
            resource: resource.to_string(),
            action: action.to_string(),
        })
    }

    /// 返回权限资源。
    ///
    /// # 返回值
    /// 返回规范化后的资源字符串。
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// 返回权限动作。
    ///
    /// # 返回值
    /// 返回规范化后的动作字符串。
    pub fn action(&self) -> &str {
        &self.action
    }

    /// 判断当前权限是否覆盖目标权限。
    ///
    /// 资源或动作上的 `*` 与 Casbin matcher 保持同一语义。
    ///
    /// # 返回值
    /// 当前权限能够授予目标权限时返回 `true`。
    pub fn covers(&self, required: &Self) -> bool {
        (self.resource == "*" || self.resource == required.resource)
            && (self.action == "*" || self.action == required.action)
    }

    fn validate_part(value: &str, label: &str, allow_slash: bool) -> Result<()> {
        if value.is_empty() {
            return Err(Error::from(format!("{label}不能为空")));
        }
        if value.len() > MAX_PERMISSION_PART_LEN {
            return Err(Error::from(format!("{label}长度不能超过128个字符")));
        }
        if value == "*" {
            return Ok(());
        }
        if !allow_slash && value.contains('/') {
            return Err(Error::from(format!("{label}不能包含斜杠")));
        }
        if value.split('/').any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
        }) {
            return Err(Error::from(format!("{label}包含非法字符")));
        }
        Ok(())
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.resource, self.action)
    }
}

impl Serialize for Permission {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Permission {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// 已去重并稳定排序的权限集合。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PermissionSet(Vec<Permission>);

impl PermissionSet {
    /// 构建规范化权限集合。
    ///
    /// # 返回值
    /// 返回去重并按 `resource:action` 排序的权限集合。
    pub fn new(permissions: impl IntoIterator<Item = Permission>) -> Self {
        let mut permissions = permissions
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        permissions.sort();
        Self(permissions)
    }

    /// 判断当前集合是否覆盖目标集合中的每项权限。
    ///
    /// # 返回值
    /// 所有目标权限都至少被当前集合中的一项覆盖时返回 `true`。
    pub fn covers(&self, required: &Self) -> bool {
        required
            .0
            .iter()
            .all(|required| self.0.iter().any(|permission| permission.covers(required)))
    }

    /// 返回规范化权限切片。
    ///
    /// # 返回值
    /// 返回内部权限的只读视图。
    pub fn as_slice(&self) -> &[Permission] {
        &self.0
    }

    /// 转换为规范化权限集合。
    ///
    /// # 返回值
    /// 返回内部权限所有权。
    pub fn into_vec(self) -> Vec<Permission> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Permission, PermissionSet, RoleIdSet};

    #[test]
    fn role_ids_should_trim_deduplicate_and_preserve_order() {
        let role_ids = RoleIdSet::parse([" role-a ", "role-a", "role-b"]).unwrap();
        assert_eq!(role_ids.to_strings(), vec!["role-a", "role-b"]);
    }

    #[test]
    fn role_ids_should_reject_empty_required_collection() {
        assert!(RoleIdSet::parse_non_empty(Vec::<String>::new()).is_err());
    }

    #[test]
    fn permission_should_normalize_valid_value() {
        let permission = Permission::parse(" Role:Read ").unwrap();
        assert_eq!(permission.to_string(), "role:read");
    }

    #[test]
    fn permission_should_reject_invalid_value() {
        assert!(Permission::parse("role").is_err());
        assert!(Permission::parse("role:read:all").is_err());
        assert!(Permission::parse("role:read/all").is_err());
    }

    #[test]
    fn wildcard_permission_should_cover_matching_scope() {
        let root = Permission::parse("*:*").unwrap();
        let customer_all = Permission::parse("customer:*").unwrap();
        let customer_read = Permission::parse("customer:read").unwrap();
        let role_read = Permission::parse("role:read").unwrap();

        assert!(root.covers(&customer_read));
        assert!(customer_all.covers(&customer_read));
        assert!(!customer_read.covers(&customer_all));
        assert!(!customer_all.covers(&role_read));
    }

    #[test]
    fn permission_set_should_normalize_and_enforce_subset() {
        let actor = PermissionSet::new([
            Permission::parse("customer:*").unwrap(),
            Permission::parse("role:read").unwrap(),
            Permission::parse("customer:*").unwrap(),
        ]);
        let allowed = PermissionSet::new([
            Permission::parse("customer:update").unwrap(),
            Permission::parse("role:read").unwrap(),
        ]);
        let elevated = PermissionSet::new([Permission::parse("role:delete").unwrap()]);

        assert_eq!(actor.as_slice().len(), 2);
        assert!(actor.covers(&allowed));
        assert!(!actor.covers(&elevated));
    }
}
