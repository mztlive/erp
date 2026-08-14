/**
 * 商品详情页 SKU 行 → 供给登记对话框的固定 SKU 身份。
 * 原逻辑内嵌在 SKU 表格单元格中，抽为纯函数便于复用与测试。
 */

import type {
    ProductFields,
    ProductSkuFields,
} from "@/features/master-data/types"
import type { FixedSku } from "@/features/supplier-offerings/types"

export function toFixedSku(
    fields: ProductFields,
    sku: ProductSkuFields,
    productName: string,
): FixedSku {
    return {
        skuId: sku.skuId ?? "",
        skuCode: sku.skuNo,
        skuName: sku.name.trim() || productName,
        productKind: fields.productKind,
        specification: sku.specLabel,
        baseUnit: sku.baseUnit ?? fields.baseUnit,
        category: fields.category || undefined,
        brand: fields.brand || undefined,
        barcode: sku.barcode,
        description: fields.description || undefined,
        carouselImages: fields.carouselImages,
        detailImages: fields.detailImages,
        carouselFileAssetIds: fields.carouselFileAssetIds,
        detailFileAssetIds: fields.detailFileAssetIds,
        carouselPreviewUrls: fields.carouselPreviewUrls,
        detailPreviewUrls: fields.detailPreviewUrls,
        mainImage: sku.mainImage || undefined,
        mainImageAssetId: sku.mainImageAssetId,
        mainImagePreviewUrl: sku.mainImagePreviewUrl,
    }
}
