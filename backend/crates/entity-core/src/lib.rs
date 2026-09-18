use chrono::Utc;
use serde::{Deserialize, Serialize};

/// 未软删除标记：`deleted_at` 为该值表示实体处于活动状态。
pub const NOT_DELETED_TIMESTAMP: u64 = 0;
/// [`NOT_DELETED_TIMESTAMP`] 的 BSON 形态（MongoDB 时间戳字段为有符号整数）。
///
/// `as` 仅用于该零值常量的编译期镜像，相等性由单测锁定。
pub const NOT_DELETED_TIMESTAMP_BSON: i64 = NOT_DELETED_TIMESTAMP as i64;

/// 测试伪造实例使用的固定非零时间戳。
const FAKE_TIMESTAMP: u64 = 1_700_000_000;

/// 新建实体的起始版本号。
const INITIAL_VERSION: u64 = 1;

/// 持久化实体共用的基础元数据：主键、版本号与时间戳。
///
/// 调用方通过 `#[serde(flatten)]` 将其嵌入业务实体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BaseModel {
    /// 实体主键，由调用方生成。
    pub id: String,
    /// 乐观锁版本号，新建实例从 1 开始。
    pub version: u64,
    /// 创建时间的 Unix 秒。
    pub created_at: u64,
    /// 更新时间的 Unix 秒。
    pub updated_at: u64,
    /// 软删除时间的 Unix 秒，为 [`NOT_DELETED_TIMESTAMP`] 时表示未删除。
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
        Self::active(id, clamp_unix_timestamp(Utc::now().timestamp()))
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
        Self::active("fake".to_string(), FAKE_TIMESTAMP)
    }

    /// 以共享不变式组装活动实例：起始版本、创建与更新时间相同、未删除。
    fn active(id: String, timestamp: u64) -> Self {
        Self {
            id,
            version: INITIAL_VERSION,
            created_at: timestamp,
            updated_at: timestamp,
            deleted_at: NOT_DELETED_TIMESTAMP,
        }
    }
}

/// Unix 秒转存储时间戳，负值钳制为零值。
fn clamp_unix_timestamp(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(NOT_DELETED_TIMESTAMP)
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
    use super::{
        BaseModel, FAKE_TIMESTAMP, HasBaseModel, NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON,
        clamp_unix_timestamp,
    };

    #[test]
    fn new_builds_active_model_with_shared_invariants() {
        let model = BaseModel::new("id_1".to_string());

        assert_eq!(model.id, "id_1");
        assert_eq!(model.version, 1);
        assert_eq!(model.created_at, model.updated_at);
        assert_eq!(model.deleted_at, NOT_DELETED_TIMESTAMP);
        assert!(!model.is_deleted());
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
    fn clamp_unix_timestamp_passes_through_non_negative() {
        assert_eq!(clamp_unix_timestamp(0), 0);
        assert_eq!(clamp_unix_timestamp(1_700_000_000), 1_700_000_000);
    }

    #[test]
    fn clamp_unix_timestamp_clamps_negative_to_not_deleted() {
        assert_eq!(clamp_unix_timestamp(-1), NOT_DELETED_TIMESTAMP);
        assert_eq!(clamp_unix_timestamp(i64::MIN), NOT_DELETED_TIMESTAMP);
    }

    #[test]
    fn non_zero_deleted_at_marks_model_deleted() {
        let model = BaseModel { deleted_at: 1_700_000_001, ..BaseModel::fake() };

        assert!(model.is_deleted());
    }

    struct TestEntity {
        base: BaseModel,
    }

    impl HasBaseModel for TestEntity {
        fn base(&self) -> &BaseModel {
            &self.base
        }

        fn base_mut(&mut self) -> &mut BaseModel {
            &mut self.base
        }
    }

    #[test]
    fn has_base_model_defaults_delegate_to_base() {
        let mut entity = TestEntity { base: BaseModel::fake() };

        assert_eq!(entity.id(), "fake");
        assert_eq!(entity.version(), 1);
        assert!(!entity.is_deleted());

        entity.base_mut().deleted_at = 1_700_000_001;

        assert!(entity.is_deleted());
    }

    #[test]
    fn soft_delete_zero_constants_stay_equal() {
        assert_eq!(u64::try_from(NOT_DELETED_TIMESTAMP_BSON).expect("零值必须可转换"), NOT_DELETED_TIMESTAMP);
        assert_eq!(i64::try_from(NOT_DELETED_TIMESTAMP).expect("零值必须可转换"), NOT_DELETED_TIMESTAMP_BSON);
    }
}
