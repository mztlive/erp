/** 商品的创建 / 修订 / 停用命令；SKU 行与媒体映射为本模块私有。 */

import { apiPost, apiPut } from "@/lib/api"
import type { ProductDto } from "@/features/master-data/api/contracts"
import { centerProduct } from "@/features/master-data/api/centers"
import { isFutureDate } from "@/features/master-data/api/list-mappers"
import { isoNow } from "@/features/master-data/api/presentation"
import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
    ProductFields,
} from "@/features/master-data/types"
import { mapMutationError } from "./shared"

/** SPU 媒体写入项：文件资产 + 展示顺序。 */
function mapProductMedia(
    names: readonly string[],
    assetIds: Readonly<Record<string, string>>,
): Array<{ file_asset_id: string; sort_order: number }> {
    return names
        .map((name, index) => ({
            file_asset_id: assetIds[name]?.trim() ?? "",
            sort_order: index,
        }))
        .filter((entry) => entry.file_asset_id)
}

function mapProductSkus(fields: ProductFields) {
    return fields.skus
        .filter((sku) => sku.lifecycleStatus === "ENABLED")
        .map((sku) => ({
            sku_id: sku.skuId || null,
            expected_sku_revision_id: sku.skuRevisionId || null,
            reenable: Boolean(sku.skuId && sku.requiresExplicitReenable),
            sku_no: sku.skuNo,
            name: sku.name.trim(),
            base_unit_id: fields.baseUnitId,
            barcode: sku.barcode || null,
            main_image_asset_id: sku.mainImageAssetId || null,
            weight_kg: null,
            volume_m3: null,
            sales_visible_price_gross: sku.salePrice || null,
            market_price: sku.marketPrice || null,
            spec_entries: fields.specs.flatMap((spec, index) => {
                const attributeCode = spec.name.trim()
                const attributeValueCode = (
                    sku.attributeValues[index] ?? ""
                ).trim()
                return attributeCode && attributeValueCode
                    ? [
                          {
                              attribute_code: attributeCode,
                              attribute_value_code: attributeValueCode,
                          },
                      ]
                    : []
            }),
        }))
}

export async function createProduct(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as ProductFields
    if (!fields.productKind) {
        return {
            outcome: "blocked",
            code: "PRODUCT_KIND_REQUIRED",
            message: "请选择商品类型后再保存。",
            detail: "商品类型决定商品业务作用，保存后不可修改。",
        }
    }
    if (!fields.categoryId || !fields.brandId || !fields.baseUnitId) {
        return {
            outcome: "blocked",
            code: "PRODUCT_REQUIRED_REFS",
            message: "请完整填写分类、品牌与基础单位。",
        }
    }
    if (fields.skus.length === 0) {
        return {
            outcome: "blocked",
            code: "SKU_REQUIRED",
            message: "至少需要一个 SKU。",
        }
    }

    try {
        const created = await apiPost<ProductDto>("/admin/products", {
            change_reason: input.changeReason || "新建商品",
            product_no: fields.productNo.trim(),
            product_kind: fields.productKind,
            name: input.name.trim(),
            description: fields.description || null,
            specification: fields.specification || null,
            category_id: fields.categoryId,
            brand_id: fields.brandId,
            status: "active",
            effective_from: input.effectiveFrom,
            effective_to: input.effectiveTo || null,
            carousel_media: mapProductMedia(
                fields.carouselImages,
                fields.carouselFileAssetIds,
            ),
            detail_media: mapProductMedia(
                fields.detailImages,
                fields.detailFileAssetIds,
            ),
            skus: mapProductSkus(fields),
        })
        if (!created.current_revision_id) {
            throw new Error("商品创建成功但未返回当前修订，禁止伪造修订身份")
        }
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.product_no,
            revisionId: created.current_revision_id,
            revisionNo: 1,
            revisionState: isFutureDate(input.effectiveFrom)
                ? "FUTURE"
                : "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.product_no}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

export async function updateProductRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    try {
        const fields = input.fields as ProductFields
        if (!fields.categoryId || !fields.brandId || !fields.baseUnitId) {
            return {
                outcome: "blocked",
                code: "PRODUCT_REQUIRED_REFS",
                message: "请完整填写分类、品牌与基础单位。",
            }
        }
        const updated = await apiPut<ProductDto>(
            `/admin/products/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                change_reason: input.changeReason,
                name: input.name.trim(),
                description: fields.description || null,
                specification: fields.specification || null,
                category_id: fields.categoryId,
                brand_id: fields.brandId,
                status:
                    fields.lifecycleStatus === "DISABLED"
                        ? "disabled"
                        : "active",
                effective_from: input.effectiveFrom,
                effective_to: input.effectiveTo || null,
                carousel_media: mapProductMedia(
                    fields.carouselImages,
                    fields.carouselFileAssetIds,
                ),
                detail_media: mapProductMedia(
                    fields.detailImages,
                    fields.detailFileAssetIds,
                ),
                skus: mapProductSkus(fields),
            },
        )
        if (!updated.current_revision_id) {
            throw new Error(
                "商品更新成功但未返回当前修订，禁止伪造修订身份",
            )
        }
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.product_no,
            revisionId: updated.current_revision_id,
            revisionNo: updated.version,
            revisionState: isFutureDate(input.effectiveFrom)
                ? "FUTURE"
                : "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-REV-${updated.product_no}-v${updated.version}`,
            nextActions: ["查看变更历史", "返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

export async function disableProduct(
    input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    try {
        // Product update requires full body; load current then set disabled.
        const center = await centerProduct(input.stableId)
        if (!center) {
            return {
                outcome: "unknown",
                message: "资料不存在或无权访问。",
                idempotencyKey: input.idempotencyKey,
            }
        }
        if (center.lifecycleStatus === "DISABLED") {
            return {
                outcome: "blocked",
                code: "ALREADY_DISABLED",
                message: "资料已停用；不是删除，历史记录仍可查看。",
            }
        }
        const detail = center.productDetail
        const updated = await apiPut<ProductDto>(
            `/admin/products/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                change_reason: input.changeReason,
                name: center.name,
                description: detail?.description || null,
                specification: detail?.specification || null,
                category_id: detail?.categoryId || "",
                brand_id: detail?.brandId || "",
                status: "disabled",
                effective_from: input.effectiveFrom,
                effective_to: center.currentRevision.effectiveTo || null,
                carousel_media: mapProductMedia(
                    detail?.carouselImages ?? [],
                    detail?.carouselFileAssetIds ?? {},
                ),
                detail_media: mapProductMedia(
                    detail?.detailImages ?? [],
                    detail?.detailFileAssetIds ?? {},
                ),
                skus: detail
                    ? mapProductSkus({
                          ...detail,
                          productKind: center.productKind ?? "",
                      })
                    : [],
            },
        )
        if (!updated.current_revision_id) {
            throw new Error(
                "商品停用成功但未返回当前修订，禁止伪造修订身份",
            )
        }
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.product_no,
            revisionId: updated.current_revision_id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-DIS-${updated.product_no}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}
