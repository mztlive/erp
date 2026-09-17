use chrono::Utc;
use serde::{Deserialize, Serialize};

pub const NOT_DELETED_TIMESTAMP: u64 = 0;
pub const NOT_DELETED_TIMESTAMP_BSON: i64 = NOT_DELETED_TIMESTAMP as i64;

/// 测试伪造实例使用的固定非零时间戳。
const FAKE_TIMESTAMP: u64 = 1_700_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BaseModel {
    pub id: String,
    pub version: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub deleted_at: u64,
}

impl BaseModel {
    /// 创建 BaseModel 实例。
    ///
    /// # 参数
    /// * `id` - 标识符
    ///
    /// # 返回
    /// 返回创建的实例。
    ///
    /// # 错误
    /// 无；时间戳钳制到零值后仍能构造。
    pub fn new(id: String) -> Self {
        let now = Utc::now().timestamp();
        let now_u64 = u64::try_from(now).unwrap_or(NOT_DELETED_TIMESTAMP);
        Self { id, version: 1, created_at: now_u64, updated_at: now_u64, deleted_at: NOT_DELETED_TIMESTAMP }
    }

    /// 判断对象是否已被软删除。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 已删除返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 无。
    pub fn is_deleted(&self) -> bool {
        self.deleted_at != NOT_DELETED_TIMESTAMP
    }

    /// 构造用于测试的伪造实例。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与 `new` 相同不变式的固定测试实例。
    ///
    /// # 错误
    /// 无。
    pub fn fake() -> Self {
        Self {
            id: "fake".to_string(),
            version: 1,
            created_at: FAKE_TIMESTAMP,
            updated_at: FAKE_TIMESTAMP,
            deleted_at: NOT_DELETED_TIMESTAMP,
        }
    }
}

/// 提供实体持久化元数据的读写访问。
pub trait HasBaseModel {
    /// 返回实体持久化元数据。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回引用，生命周期与持有者一致。
    ///
    /// # 错误
    /// 无。
    fn base(&self) -> &BaseModel;

    /// 返回实体持久化元数据的可变引用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可变引用，生命周期与持有者一致。
    ///
    /// # 错误
    /// 无。
    fn base_mut(&mut self) -> &mut BaseModel;

    /// 判断实体是否已被软删除。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 已删除返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 无。
    fn is_deleted(&self) -> bool {
        self.base().is_deleted()
    }

    /// 返回实体持久化主键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回主键字符串引用。
    ///
    /// # 错误
    /// 无。
    fn id(&self) -> &str {
        &self.base().id
    }

    /// 返回实体版本号。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前版本号。
    ///
    /// # 错误
    /// 无。
    fn version(&self) -> u64 {
        self.base().version
    }
}

#[cfg(test)]
mod tests {
    use super::{BaseModel, FAKE_TIMESTAMP, NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON};

    #[test]
    fn new_model_is_active() {
        let model = BaseModel::new("id_1".to_string());

        assert!(!model.is_deleted());
    }

    #[test]
    fn new_accepts_string_without_explicit_conversion() {
        let model = BaseModel::new("id_1".to_string());

        assert_eq!(model.id, "id_1");
        assert_eq!(model.version, 1);
    }

    #[test]
    fn fake_matches_new_invariants_with_fixed_timestamp() {
        let model = BaseModel::fake();

        assert_eq!(model.id, "fake");
        assert_eq!(model.version, 1);
        assert_eq!(model.created_at, FAKE_TIMESTAMP);
        assert_eq!(model.updated_at, FAKE_TIMESTAMP);
        assert_eq!(model.deleted_at, NOT_DELETED_TIMESTAMP);
        assert!(!model.is_deleted());
    }

    #[test]
    fn soft_delete_zero_constants_stay_equal() {
        assert_eq!(u64::from(NOT_DELETED_TIMESTAMP_BSON as u8), NOT_DELETED_TIMESTAMP);
        assert_eq!(NOT_DELETED_TIMESTAMP_BSON, NOT_DELETED_TIMESTAMP as i64);
    }
}
