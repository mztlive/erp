use chrono::Utc;
use serde::{Deserialize, Serialize};

/// 未软删除标记：`deleted_at` 为该值表示实体处于活动状态。
pub const NOT_DELETED_TIMESTAMP: u64 = 0;
/// [`NOT_DELETED_TIMESTAMP`] 的 BSON 形态（MongoDB 时间戳字段为有符号整数）。
///
/// `as` 仅用于该零值常量的编译期镜像，相等性由单测锁定。
pub const NOT_DELETED_TIMESTAMP_BSON: i64 = NOT_DELETED_TIMESTAMP as i64;

/// 测试伪造实例使用的固定非零时间戳。
const FAKE_TIMESTAMP: i64 = 1_700_000_000;

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
    /// 在新建业务实体时，用调用方已分配的主键初始化持久化元数据。
    /// 版本从 1 开始，创建和更新时间取同一当前 Unix 秒（负值按 0 处理），删除时间设为
    /// [`NOT_DELETED_TIMESTAMP`]。
    pub fn new(id: String) -> Self {
        Self::active_at(id, Utc::now().timestamp())
    }

    /// 读取实体元数据时，用它判断记录是否已被软删除；只有 `deleted_at` 等于
    /// [`NOT_DELETED_TIMESTAMP`] 才视为未删除。
    pub fn is_deleted(&self) -> bool {
        self.deleted_at != NOT_DELETED_TIMESTAMP
    }

    /// 为不应依赖系统时钟的测试提供固定主键和时间的活动元数据，初始版本与 [`Self::new`] 一致。
    pub fn fake() -> Self {
        Self::active_at("fake".to_string(), FAKE_TIMESTAMP)
    }

    // 正式构造和固定时间的测试构造共用此入口，保持起始版本、创建和更新时间相同以及未删除的约定。
    // 早于 Unix 纪元的时间按 0 存储，避免负秒数转换成异常大的无符号值。
    fn active_at(id: String, unix_secs: i64) -> Self {
        let timestamp = u64::try_from(unix_secs).unwrap_or(NOT_DELETED_TIMESTAMP);
        Self {
            id,
            version: INITIAL_VERSION,
            created_at: timestamp,
            updated_at: timestamp,
            deleted_at: NOT_DELETED_TIMESTAMP,
        }
    }
}

/// 提供实体持久化元数据的读写访问。
pub trait HasBaseModel {
    /// 暴露实体共用的持久化元数据，供仓储读取主键、版本和时间字段，无需了解实体自身的字段布局。
    fn base(&self) -> &BaseModel;

    /// 提供元数据的可变引用，供持久化写入在遵守版本和软删除规则的前提下更新公共字段。
    fn base_mut(&mut self) -> &mut BaseModel;

    /// 按公共元数据判断实体是否已软删除，供只认识本 trait 的调用方排除已删除记录。
    fn is_deleted(&self) -> bool {
        self.base().is_deleted()
    }

    /// 借用持久化主键，供仓储定位记录或建立实体间引用；它不是业务展示编号。
    fn id(&self) -> &str {
        &self.base().id
    }

    /// 读取当前持久化版本，供写入方进行乐观锁比较，避免覆盖其他操作已保存的修改。
    fn version(&self) -> u64 {
        self.base().version
    }
}

#[cfg(test)]
mod tests {
    use super::{BaseModel, HasBaseModel, NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON};

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
        assert_eq!(model.created_at, 1_700_000_000);
        assert_eq!(model.updated_at, model.created_at);
        assert_eq!(model.deleted_at, NOT_DELETED_TIMESTAMP);
        assert!(!model.is_deleted());
    }

    #[test]
    fn active_model_clamps_pre_epoch_timestamps_without_marking_deleted() {
        for (unix_secs, expected) in [(i64::MIN, 0), (-1, 0), (0, 0), (1_700_000_000, 1_700_000_000)] {
            let model = BaseModel::active_at("id_1".to_string(), unix_secs);
            assert_eq!((model.created_at, model.updated_at), (expected, expected));
            assert!(!model.is_deleted());
        }
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
