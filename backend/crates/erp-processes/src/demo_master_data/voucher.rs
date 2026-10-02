//! 卡券通过正式原子创建入口生成商品、唯一 SKU 和卡券扩展修订。
use application_core::AuditActor;
use erp_catalog::{CreateProductRequest, CreateVoucherCategoryRequest, ProductKind, VoucherSkuInput};
use validator::Validate;

use super::{DemoMasterDataService, spec};
use crate::{Error, Result};

impl DemoMasterDataService {
    /// 按类型选择正式创建入口，返回可登记的商品 ID。
    /// # 参数
    /// `request` 已解析字典 ID，`actor` 为初始化操作人。
    /// # 返回
    /// 商品真实 ID。
    /// # 错误
    /// 字段、岗位或领域创建失败时返回错误。
    pub(super) async fn create_demo_product(
        &self,
        request: CreateProductRequest,
        actor: &AuditActor,
    ) -> Result<String> {
        if request.product_kind != ProductKind::Voucher {
            return Ok(self.catalog().product_create(request, actor).await?.id);
        }
        let owner = self
            .role_actor(&spec::foundation_spec().product_maintainer_account)
            .await?
            .ok_or_else(|| Error::NotFound("卡券维护账号不可用".into()))?;
        let view = self.catalog().voucher_category_create(voucher_request(&request)?, &owner).await?;
        view.product_id.ok_or_else(|| Error::Internal("卡券创建后未返回商品 ID".into()))
    }
}

/// 转换卡券种子，拒绝正式入口不能保持的编号及多 SKU 输入。
/// # 参数
/// `request` 为卡券商品模板或已解析请求。
/// # 返回
/// 正式卡券原子创建请求。
/// # 错误
/// SKU 编号、数量、描述或字段不符合卡券合同时拒绝。
pub(super) fn voucher_request(request: &CreateProductRequest) -> Result<CreateVoucherCategoryRequest> {
    let [sku] = request.skus.as_slice() else {
        return Err(Error::ValidationError("演示卡券必须只有一个 SKU".into()));
    };
    if sku.sku_no != request.product_no || !sku.spec_entries.is_empty() {
        return Err(Error::ValidationError("演示卡券的商品与 SKU 编号必须相同且无规格组合".into()));
    }
    let result = CreateVoucherCategoryRequest {
        voucher_no: request.product_no.clone(),
        name: request.name.clone(),
        description: request.description.clone().unwrap_or_default(),
        specification: request.specification.clone(),
        category_id: Some(request.category_id.clone()),
        new_category: None,
        brand_id: Some(request.brand_id.clone()),
        sku: Some(VoucherSkuInput {
            base_unit_id: sku.base_unit_id.clone(),
            barcode: sku.barcode.clone(),
            weight_kg: sku.weight_kg,
            volume_m3: sku.volume_m3,
            factory_price_gross: sku.factory_price_gross,
            sales_visible_price_gross: sku.sales_visible_price_gross,
            bulk_price_gross: sku.bulk_price_gross,
            bulk_min_quantity: sku.bulk_min_quantity,
            market_price: sku.market_price,
        }),
        status: request.status,
        effective_from: Some(request.effective_from),
        effective_to: request.effective_to,
    };
    result.validate().map_err(|error| Error::ValidationError(format!("演示卡券字段无效：{error}")))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use erp_core::money::{Amount, Quantity};

    use super::*;
    use crate::demo_master_data::plan;
    use crate::demo_master_data::seed::SeedRequest;

    #[test]
    fn voucher_uses_atomic_request_and_rejects_inconsistent_sku() {
        let steps = plan::demo_steps().unwrap();
        let SeedRequest::Product(product) = &steps.last().unwrap().request else { panic!("卡券种子") };
        let request = voucher_request(product).unwrap();
        assert_eq!(request.voucher_no, product.skus[0].sku_no);
        assert_eq!(request.sku.unwrap().sales_visible_price_gross, product.skus[0].sales_visible_price_gross);
        let mut invalid = product.clone();
        invalid.skus[0].sku_no = "another".into();
        assert!(voucher_request(&invalid).is_err());
        invalid.skus.clear();
        assert!(voucher_request(&invalid).is_err());
    }

    /// 卡券原子创建保留完整公司四价和集采门槛，不用面额或供给成本覆盖参考价。
    #[test]
    fn voucher_preserves_company_prices_and_bulk_minimum() {
        let steps = plan::demo_steps().unwrap();
        let SeedRequest::Product(product) = &steps.last().unwrap().request else { panic!("卡券种子") };
        let mut product = product.clone();
        product.skus[0].factory_price_gross = Some("70.00".parse::<Amount>().unwrap());
        product.skus[0].bulk_price_gross = Some("90.00".parse::<Amount>().unwrap());
        product.skus[0].bulk_min_quantity = Some("100".parse::<Quantity>().unwrap());
        let sku = voucher_request(&product).unwrap().sku.unwrap();
        assert_eq!(sku.factory_price_gross, product.skus[0].factory_price_gross);
        assert_eq!(sku.sales_visible_price_gross, product.skus[0].sales_visible_price_gross);
        assert_eq!(sku.bulk_price_gross, product.skus[0].bulk_price_gross);
        assert_eq!(sku.bulk_min_quantity, product.skus[0].bulk_min_quantity);
        assert_eq!(sku.market_price, product.skus[0].market_price);
    }
}
