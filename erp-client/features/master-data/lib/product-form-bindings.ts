import * as React from "react"

import { applySpecsFromDrafts } from "@/features/master-data/lib/product-editor-model"
import type {
    ProductEditorFormValues,
    ProductSpecDraft,
} from "@/features/master-data/lib/product-editor-model"
import type { ProductInventoryPreviewSku } from "@/features/master-data/components/product/product-inventory-preview-sheet"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { ProductEditor } from "@/features/master-data/hooks/use-product-editor"
import type { ProductFields, ProductSkuFields } from "@/features/master-data/types"

/**
 * 商品详情编辑表单的值绑定：标题、库存预览 SKU、字段/规格/价格的
 * setFieldValue 封装与批量参考价格应用，供 ProductDetailPage 的
 * form.Subscribe 渲染函数使用。纯派生函数，无自身状态。
 */
export function createProductFormBindings(
    form: ProductEditor["form"],
    values: ProductEditorFormValues,
    isCreate: boolean,
    fallbackName: string | undefined,
) {
    const fields = values.fields
    const title = isCreate
        ? masterDataCopy.productCreateTitle
        : values.name || fallbackName || "商品详情"

    const inventoryPreviewSkus: ProductInventoryPreviewSku[] =
        fields.productKind === "PHYSICAL"
            ? fields.skus.flatMap((sku) =>
                  sku.skuId
                      ? [
                            {
                                skuId: sku.skuId,
                                skuNo: sku.skuNo,
                                specLabel: sku.specLabel,
                                baseUnit: sku.baseUnit || fields.baseUnit,
                            },
                        ]
                      : [],
              )
            : []
    const inventoryActionHint =
        fields.productKind && fields.productKind !== "PHYSICAL"
            ? "仅实物商品适用公司自有库存台账"
            : inventoryPreviewSkus.length === 0
              ? "选择实物商品类型并保存 SKU 后可查看正式库存"
              : undefined

    const setName = (next: string) => form.setFieldValue("name", next)
    const setEffectiveFrom = (next: string) =>
        form.setFieldValue("effectiveFrom", next)
    const setEffectiveTo = (next: string) =>
        form.setFieldValue("effectiveTo", next)
    const setChangeReason = (next: string) =>
        form.setFieldValue("changeReason", next)
    const setFields = (next: React.SetStateAction<ProductFields>) =>
        form.setFieldValue("fields", (previous) =>
            typeof next === "function" ? next(previous) : next,
        )
    const setSpecDrafts = (
        next: React.SetStateAction<readonly ProductSpecDraft[]>,
    ) =>
        form.setFieldValue("specDrafts", (previous) =>
            typeof next === "function" ? next(previous) : next,
        )
    const syncSpecDrafts = (next: readonly ProductSpecDraft[]) => {
        setSpecDrafts(next)
        setFields((previous) =>
            applySpecsFromDrafts(next, previous, values.name),
        )
    }
    const updateSku = (index: number, patch: Partial<ProductSkuFields>) => {
        setFields((previous) => ({
            ...previous,
            skus: previous.skus.map((sku, skuIndex) =>
                skuIndex === index ? { ...sku, ...patch } : sku,
            ),
        }))
    }
    const handleSubmit = (event?: React.FormEvent) => {
        event?.preventDefault()
        void form.handleSubmit()
    }

    const name = values.name
    const effectiveFrom = values.effectiveFrom
    const effectiveTo = values.effectiveTo
    const changeReason = values.changeReason
    const specDrafts = values.specDrafts
    const activeSpecs = fields.specs.filter(
        (spec) =>
            spec.name.trim() &&
            spec.values.some((value) => value.trim()),
    )

    const applyBatchReferencePrices = () => {
        const hasAny =
            values.batchSalePrice.trim() || values.batchMarketPrice.trim()
        if (!hasAny) return
        const hasFilled = values.fields.skus.some(
            (sku) => sku.salePrice?.trim() || sku.marketPrice?.trim(),
        )
        const message = hasFilled
            ? `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU，并覆盖已填写的销售价/市场价。确定继续？`
            : `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU。确定继续？`
        if (!window.confirm(message)) return
        setFields((previous) => ({
            ...previous,
            skus: previous.skus.map((sku) => ({
                ...sku,
                salePrice:
                    values.batchSalePrice.trim() || sku.salePrice || undefined,
                marketPrice:
                    values.batchMarketPrice.trim() ||
                    sku.marketPrice ||
                    undefined,
            })),
        }))
    }

    return {
        title,
        fields,
        inventoryPreviewSkus,
        inventoryActionHint,
        setName,
        setEffectiveFrom,
        setEffectiveTo,
        setChangeReason,
        setFields,
        syncSpecDrafts,
        updateSku,
        handleSubmit,
        name,
        effectiveFrom,
        effectiveTo,
        changeReason,
        specDrafts,
        activeSpecs,
        applyBatchReferencePrices,
    }
}
