//! 导入身份和行内规则；稳定实体 ID 由既有唯一索引保护同次提交重试。
use super::ProductBrandId;
use super::product_brand::{ProductBrand, ProductBrandData};
use crate::error::{Error, Result};

/// 已验证的浏览器生成 UUID；不允许任意路径或任意实体 ID。
#[derive(Debug)]
pub struct BrandImportKey(String);

impl BrandImportKey {
    /// 解析稳定导入身份。
    /// # 参数
    /// * `value` - UUID 格式的请求身份
    /// # 返回
    /// 已验证的导入身份。
    /// # 错误
    /// 格式不合法。
    pub fn new(value: &str) -> Result<Self> {
        let valid = value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| {
                if [8, 13, 18, 23].contains(&index) { byte == b'-' } else { byte.is_ascii_hexdigit() }
            });
        if !valid {
            return Err(Error::ValidationError("导入标识无效，请重新选择文件".into()));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    /// 生成仅用于该导入行的品牌身份。
    /// # 参数
    /// * `row_number` - 原始 Excel 行号
    /// # 返回
    /// 受品牌 ID 唯一索引保护的稳定身份。
    /// # 错误
    /// 无。
    pub fn brand_id(&self, row_number: u32) -> ProductBrandId {
        ProductBrandId::new(format!("brand-import-{}-{row_number}", self.0))
    }
}

/// 已复用品牌实体完成名称与代码校验的导入内容。
pub(crate) struct PreparedBrandImport {
    pub brand: ProductBrand,
    pub automatic_code: bool,
}

impl PreparedBrandImport {
    /// 规范化行内容，空代码暂用占位值，写入前由取号器替换。
    pub(crate) fn new(id: ProductBrandId, name: String, code: String, actor: &str) -> Result<Self> {
        let automatic_code = code.trim().is_empty();
        let data = ProductBrandData::new(if automatic_code { "AUTO".into() } else { code }, name);
        Ok(Self {
            brand: ProductBrand::new(id, data, actor)
                .map_err(|error| Error::ValidationError(error.to_string()))?,
            automatic_code,
        })
    }

    /// 判断已存在记录能否作为跳过结果；停用记录保持停用。
    pub(crate) fn ensure_matches(&self, existing: &ProductBrand) -> Result<()> {
        if existing.base.is_deleted() {
            return Err(Error::ConflictError("原品牌已删除，请核对品牌资料后使用新的文件重新导入".into()));
        }
        if existing.name != self.brand.name
            || (!self.automatic_code && existing.brand_code != self.brand.brand_code)
        {
            return Err(Error::ConflictError("品牌名称或代码与已有记录不一致，请核对后重试".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entity_core::NOT_DELETED_TIMESTAMP;

    use super::*;
    use crate::entity::catalog::EnableStatus;

    #[test]
    fn import_identity_is_stable_and_rejects_invalid_input() {
        let key = BrandImportKey::new("12345678-1234-4234-8234-123456789ABC").unwrap();
        assert_eq!(key.brand_id(2), key.brand_id(2));
        assert_ne!(key.brand_id(2), key.brand_id(3));
        for value in ["", "../../brand", "zzzzzzzz-1234-4234-8234-123456789abc"] {
            assert!(BrandImportKey::new(value).is_err());
        }
    }

    #[test]
    fn import_normalizes_and_checks_replay_content() {
        let row = PreparedBrandImport::new(ProductBrandId::new("test"), " 品牌 ".into(), " ".into(), "user")
            .unwrap();
        assert!(row.automatic_code);
        assert_eq!(row.brand.name, "品牌");

        let mut existing = row.brand.clone();
        existing.brand_code = "BR-001".into();
        assert!(row.ensure_matches(&existing).is_ok());
        existing.name = "其他品牌".into();
        assert!(row.ensure_matches(&existing).is_err());
        existing.name = "品牌".into();
        existing.stable.status = EnableStatus::Disabled;
        assert!(row.ensure_matches(&existing).is_ok());
        existing.base.deleted_at = 1;
        assert!(row.ensure_matches(&existing).is_err());

        existing.base.deleted_at = NOT_DELETED_TIMESTAMP;
        existing.name = "其他品牌".into();
        for name in [String::new(), " ".into(), "字".repeat(129)] {
            assert!(PreparedBrandImport::new(ProductBrandId::new("test"), name, "B".into(), "user").is_err());
        }
        let coded =
            PreparedBrandImport::new(ProductBrandId::new("test"), "其他品牌".into(), " B ".into(), "user")
                .unwrap();
        assert_eq!(coded.brand.brand_code, "B");
        assert!(coded.ensure_matches(&existing).is_err());

        assert!(
            PreparedBrandImport::new(ProductBrandId::new("test"), "品牌".into(), "x".repeat(65), "user")
                .is_err()
        );
    }
}
