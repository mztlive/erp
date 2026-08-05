use chrono::Local;
use serde::{Deserialize, Serialize};

pub const NOT_DELETED_TIMESTAMP: u64 = 0;
pub const NOT_DELETED_TIMESTAMP_BSON: i64 = 0;

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
    pub fn new(id: String) -> Self {
        let now = Local::now().timestamp();
        Self {
            id,
            version: 1,
            created_at: now as u64,
            updated_at: now as u64,
            deleted_at: NOT_DELETED_TIMESTAMP,
        }
    }

    /// 判断对象是否已被软删除。
    ///
    /// # 返回
    /// 已删除返回 `true`，否则返回 `false`。
    pub fn is_deleted(&self) -> bool {
        self.deleted_at != NOT_DELETED_TIMESTAMP
    }

    /// 构造用于测试的伪造实例。
    ///
    /// # 返回
    /// 返回创建的实例。
    pub fn fake() -> Self {
        Self {
            id: "fake".to_string(),
            ..Default::default()
        }
    }
}

/// 提供实体持久化元数据的读写访问。
pub trait HasBaseModel {
    /// 返回实体持久化元数据。
    ///
    /// # 返回
    /// 返回引用，生命周期与持有者一致。
    fn base(&self) -> &BaseModel;

    /// 返回实体持久化元数据的可变引用。
    ///
    /// # 返回
    /// 返回可变引用，生命周期与持有者一致。
    fn base_mut(&mut self) -> &mut BaseModel;
}

#[cfg(test)]
mod tests {
    use super::BaseModel;

    #[test]
    fn new_model_is_active() {
        let model = BaseModel::new("id_1".to_string());

        assert!(!model.is_deleted());
    }
}
