//! 客户工作簿使用稳定 SKU 编码的精确批量身份读取。

use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result};

use super::CatalogRepository;
use crate::entity::catalog::Sku;
use crate::repository::CatalogExt;

impl CatalogRepository<'_> {
    /// 按编码精确读取未删除 SKU；不执行名称猜测或模糊匹配。
    /// # 参数
    /// `codes` 为调用方已经限制数量的公司 SKU 编码，`executor` 为原读取或写事务。
    /// # 返回
    /// 返回精确命中的稳定 SKU，空输入返回空结果；可售资格仍由消费用例重验。
    /// # 错误
    /// 数据库或反序列化失败时返回存储错误。
    pub async fn skus_by_exact_codes(
        &self,
        codes: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        if codes.is_empty() {
            return Ok(Vec::new());
        }
        self.db.skus().find_many(exact_codes_filter(codes), executor).await
    }
}

fn exact_codes_filter(codes: &[String]) -> Document {
    doc! {"sku_no": {"$in": codes}}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sku_codes_remain_literal_strings_including_leading_zeroes_and_regex_characters() {
        let filter = exact_codes_filter(&["00123".into(), "00.*".into()]);
        let values = filter.get_document("sku_no").unwrap().get_array("$in").unwrap();
        assert_eq!(values[0].as_str(), Some("00123"));
        assert_eq!(values[1].as_str(), Some("00.*"));
    }
}
