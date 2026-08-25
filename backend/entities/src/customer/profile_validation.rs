//! 客户资料根命令的纯结构规则。
//!
//! 本模块只校验不依赖数据库与外部 I/O 的命令形状；DTO 协议字段格式仍由
//! 上层 DTO 负责，实体与仓储存在性、唯一索引和事务一致性由各自层处理。

use std::collections::HashSet;

use crate::errors::{Error, Result};

/// 客户资料根命令操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerProfileOperation {
    /// 创建完整客户资料。
    Create,
    /// 修订既有客户资料。
    Update,
}

/// 客户资料根命令的版本与负责人字段形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomerProfileRequestShape {
    operation: CustomerProfileOperation,
    expected_party_version: Option<u64>,
    expected_customer_version: Option<u64>,
    owner_user_id_present: bool,
}

impl CustomerProfileRequestShape {
    /// 构造并校验客户资料根命令的结构字段。
    ///
    /// 创建命令不得携带既有版本；修订命令必须携带大于零的 Party 与客户
    /// 版本，且负责人变化必须改走客户归属命令。创建场景兼容旧客户端的
    /// 负责人字段，但领域写入仍以服务端创建人为准。
    ///
    /// # 参数
    /// * `operation` - 创建或修订操作
    /// * `expected_party_version` - 客户端期望的 Party 版本
    /// * `expected_customer_version` - 客户端期望的客户版本
    /// * `owner_user_id_present` - 请求是否携带负责人字段
    ///
    /// # 返回
    /// 结构合法时返回已校验的值对象。
    ///
    /// # 错误
    /// 创建携带版本，或修订缺少正版本、携带负责人字段时返回错误。
    pub fn new(
        operation: CustomerProfileOperation,
        expected_party_version: Option<u64>,
        expected_customer_version: Option<u64>,
        owner_user_id_present: bool,
    ) -> Result<Self> {
        let shape = Self {
            operation,
            expected_party_version,
            expected_customer_version,
            owner_user_id_present,
        };
        shape.validate()?;
        Ok(shape)
    }

    /// 返回根命令操作类型。
    ///
    /// # 返回
    /// 返回创建或修订操作。
    pub fn operation(&self) -> CustomerProfileOperation {
        self.operation
    }

    /// 校验根命令的版本与负责人字段组合。
    ///
    /// # 返回
    /// 字段组合合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 字段组合不符合创建或修订约束时返回错误。
    fn validate(&self) -> Result<()> {
        match self.operation {
            CustomerProfileOperation::Create => {
                if self.expected_party_version.is_some() || self.expected_customer_version.is_some() {
                    return Err(Error::from("创建客户时不能提交既有版本"));
                }
            }
            CustomerProfileOperation::Update => {
                if self.owner_user_id_present {
                    return Err(Error::from("负责人变更必须通过客户归属操作提交"));
                }
                ensure_positive_version(self.expected_party_version, "主体")?;
                ensure_positive_version(self.expected_customer_version, "客户")?;
            }
        }
        Ok(())
    }
}

/// 客户资料从属事实类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerProfileFactKind {
    /// 联系人事实。
    Contact,
    /// 地址事实。
    Address,
    /// 银行账户事实。
    BankAccount,
}

impl CustomerProfileFactKind {
    /// 返回事实类型的中文名称。
    ///
    /// # 返回
    /// 返回用于领域错误的稳定中文名称。
    fn label(self) -> &'static str {
        match self {
            Self::Contact => "联系人",
            Self::Address => "地址",
            Self::BankAccount => "银行账户",
        }
    }

    /// 返回新事实必填敏感值的中文名称。
    ///
    /// # 返回
    /// 返回手机号、地址或银行账号标签。
    fn required_value_label(self) -> &'static str {
        match self {
            Self::Contact => "手机号",
            Self::Address => "地址",
            Self::BankAccount => "银行账号",
        }
    }
}

/// 客户资料事实输入的最小结构契约。
pub trait CustomerProfileFactInput {
    /// 返回既有事实 ID；`None` 表示新增事实。
    fn existing_id(&self) -> Option<&str>;

    /// 返回当前输入是否被标记为默认项。
    fn is_default(&self) -> bool;

    /// 返回新增事实所需的敏感明文；既有事实允许省略。
    fn required_value(&self) -> Option<&str>;
}

/// 一类客户资料事实的可选输入集合。
#[derive(Debug, Clone, Copy)]
pub struct CustomerProfileFactSet<'a, T> {
    kind: CustomerProfileFactKind,
    items: Option<&'a [T]>,
}

impl<'a, T> CustomerProfileFactSet<'a, T>
where
    T: CustomerProfileFactInput,
{
    /// 构造事实集合结构校验值对象。
    ///
    /// # 参数
    /// * `kind` - 联系人、地址或银行账户类型
    /// * `items` - 请求显式提交的事实集合；`None` 表示修订时保留
    ///
    /// # 返回
    /// 返回待校验的事实集合值对象。
    pub fn new(kind: CustomerProfileFactKind, items: Option<&'a [T]>) -> Self {
        Self { kind, items }
    }

    /// 校验默认项、既有 ID 与新增事实必填值。
    ///
    /// 每类事实最多一个默认项；既有 ID 去空白后必须非空且不得重复；
    /// 创建命令不得引用既有事实；新增事实必须携带对应敏感明文。
    ///
    /// # 参数
    /// * `operation` - 创建或修订操作
    ///
    /// # 返回
    /// 结构合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 默认项重复、既有 ID 非法/重复、创建引用既有事实或新增事实缺少
    /// 必填敏感值时返回错误。
    pub fn validate(&self, operation: CustomerProfileOperation) -> Result<()> {
        let Some(items) = self.items else {
            return Ok(());
        };
        if items.iter().filter(|item| item.is_default()).count() > 1 {
            return Err(Error::from(format!(
                "同一时间只能有一个默认{}",
                self.kind.label()
            )));
        }

        let mut existing_ids = HashSet::with_capacity(items.len());
        for item in items {
            if let Some(existing_id) = item.existing_id() {
                validate_existing_id(existing_id, operation, self.kind, &mut existing_ids)?;
                continue;
            }
            let has_required_value = item
                .required_value()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
            if !has_required_value {
                return Err(Error::from(format!(
                    "{}不能为空",
                    self.kind.required_value_label()
                )));
            }
        }
        Ok(())
    }
}

/// 校验修订命令版本存在且大于零。
fn ensure_positive_version(value: Option<u64>, label: &str) -> Result<()> {
    if value.is_some_and(|version| version > 0) {
        return Ok(());
    }
    Err(Error::from(format!("修订时必须提供{label}版本")))
}

/// 校验并登记既有事实 ID。
fn validate_existing_id(
    existing_id: &str,
    operation: CustomerProfileOperation,
    kind: CustomerProfileFactKind,
    existing_ids: &mut HashSet<String>,
) -> Result<()> {
    let existing_id = existing_id.trim();
    if existing_id.is_empty() {
        return Err(Error::from(format!("既有{} ID 不能为空", kind.label())));
    }
    if operation == CustomerProfileOperation::Create {
        return Err(Error::from(format!("创建客户时不能引用既有{}", kind.label())));
    }
    if !existing_ids.insert(existing_id.to_string()) {
        return Err(Error::from(format!("同一{}不能重复提交", kind.label())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CustomerProfileFactInput, CustomerProfileFactKind, CustomerProfileFactSet, CustomerProfileOperation,
        CustomerProfileRequestShape,
    };

    #[derive(Debug, Clone, Copy)]
    struct Fact<'a> {
        existing_id: Option<&'a str>,
        is_default: bool,
        required_value: Option<&'a str>,
    }

    impl CustomerProfileFactInput for Fact<'_> {
        fn existing_id(&self) -> Option<&str> {
            self.existing_id
        }

        fn is_default(&self) -> bool {
            self.is_default
        }

        fn required_value(&self) -> Option<&str> {
            self.required_value
        }
    }

    #[test]
    fn request_shape_accepts_valid_create_and_update() {
        assert!(
            CustomerProfileRequestShape::new(CustomerProfileOperation::Create, None, None, true,).is_ok()
        );
        assert!(
            CustomerProfileRequestShape::new(CustomerProfileOperation::Update, Some(1), Some(1), false,)
                .is_ok()
        );
    }

    #[test]
    fn request_shape_rejects_invalid_versions_and_update_owner() {
        assert!(
            CustomerProfileRequestShape::new(CustomerProfileOperation::Create, Some(1), None, false,)
                .is_err()
        );
        assert!(
            CustomerProfileRequestShape::new(CustomerProfileOperation::Update, Some(0), Some(1), false,)
                .is_err()
        );
        assert!(
            CustomerProfileRequestShape::new(CustomerProfileOperation::Update, Some(1), Some(1), true,)
                .is_err()
        );
    }

    #[test]
    fn fact_set_accepts_single_default_and_existing_without_plaintext() {
        let facts = [
            Fact {
                existing_id: Some("contact-1"),
                is_default: true,
                required_value: None,
            },
            Fact {
                existing_id: None,
                is_default: false,
                required_value: Some("13800138000"),
            },
        ];
        let set = CustomerProfileFactSet::new(CustomerProfileFactKind::Contact, Some(&facts));
        assert!(set.validate(CustomerProfileOperation::Update).is_ok());
    }

    #[test]
    fn fact_set_rejects_multiple_defaults_and_duplicate_trimmed_ids() {
        let defaults = [
            Fact {
                existing_id: Some("contact-1"),
                is_default: true,
                required_value: None,
            },
            Fact {
                existing_id: Some("contact-2"),
                is_default: true,
                required_value: None,
            },
        ];
        let set = CustomerProfileFactSet::new(CustomerProfileFactKind::Contact, Some(&defaults));
        assert!(set.validate(CustomerProfileOperation::Update).is_err());

        let duplicates = [
            Fact {
                existing_id: Some(" contact-1 "),
                is_default: false,
                required_value: None,
            },
            Fact {
                existing_id: Some("contact-1"),
                is_default: false,
                required_value: None,
            },
        ];
        let set = CustomerProfileFactSet::new(CustomerProfileFactKind::Contact, Some(&duplicates));
        assert!(set.validate(CustomerProfileOperation::Update).is_err());
    }

    #[test]
    fn fact_set_rejects_create_existing_id_and_missing_new_value() {
        let existing = [Fact {
            existing_id: Some("address-1"),
            is_default: false,
            required_value: None,
        }];
        let set = CustomerProfileFactSet::new(CustomerProfileFactKind::Address, Some(&existing));
        assert!(set.validate(CustomerProfileOperation::Create).is_err());

        let missing = [Fact {
            existing_id: None,
            is_default: false,
            required_value: Some("   "),
        }];
        let set = CustomerProfileFactSet::new(CustomerProfileFactKind::BankAccount, Some(&missing));
        assert!(set.validate(CustomerProfileOperation::Create).is_err());
    }

    #[test]
    fn fact_set_accepts_absent_and_empty_collections() {
        let absent: CustomerProfileFactSet<'_, Fact<'_>> =
            CustomerProfileFactSet::new(CustomerProfileFactKind::Address, None);
        assert!(absent.validate(CustomerProfileOperation::Update).is_ok());
        let empty: [Fact<'_>; 0] = [];
        let empty = CustomerProfileFactSet::new(CustomerProfileFactKind::Address, Some(&empty));
        assert!(empty.validate(CustomerProfileOperation::Update).is_ok());
    }
}
