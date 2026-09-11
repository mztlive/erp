//! 产品报价表异步导入：解析模板、登记后台任务并逐行创建商品。

mod add_sku;
mod backfill;
mod direct_upload;
mod execute;
mod identity;
mod images;
mod parse;
mod query;
mod resolve;
mod row;
mod row_manifest;
mod submit;
mod views;

use mongodb::Database;
use storage::S3Storage;

/// 商品报价表导入流程。
#[derive(Clone)]
pub struct ProductImportProcess {
    db: Database,
    storage: S3Storage,
    secret: Vec<u8>,
}

impl ProductImportProcess {
    /// 创建导入流程。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `storage` - 对象存储
    /// * `secret` - 文件内容指纹密钥
    ///
    /// # 返回
    /// 返回流程实例。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, storage: S3Storage, secret: impl Into<Vec<u8>>) -> Self {
        Self {
            db,
            storage,
            secret: secret.into(),
        }
    }
}
