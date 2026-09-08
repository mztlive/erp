import type { ChangeConfirmation } from "../hooks/use-change-confirmation"
import * as React from "react"

import {
    applySpecsFromDrafts,
    createSpecDraft,
    validateSpecDrafts,
} from "@/features/master-data/lib/product-editor-model"
import type {
    ProductEditorFormValues,
    ProductSpecDraft,
} from "@/features/master-data/lib/product-editor-model"
import type { ProductInventoryPreviewSku } from "@/features/master-data/components/product/product-inventory-preview-sheet"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { ProductEditor } from "@/features/master-data/hooks/use-product-editor"
import type {
    ProductFields,
    ProductSkuFields,
} from "@/features/master-data/types"

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
    confirmChange: (request: ChangeConfirmation) => void,
    offerUndo?: (undo: () => void) => void,
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
    }
    const resetSpecDrafts = () =>
        setSpecDrafts(
            fields.specs.map((spec) => createSpecDraft(spec.name, spec.values)),
        )
    const applySpecDrafts = (): string | null => {
        const error = validateSpecDrafts(values.specDrafts)
        if (error) return error
        const next = applySpecsFromDrafts(
            values.specDrafts,
            fields,
            values.name,
        )
        const signatures = new Set(
            next.skus.map((sku) => sku.specificationSignature),
        )
        const removed = fields.skus.filter(
            (sku) => !signatures.has(sku.specificationSignature ?? ""),
        )
        if (removed.length) {
            confirmChange({
                title: `移除 ${removed.length} 个 SKU？`,
                description: `应用后将得到 ${next.skus.length} 个 SKU。被移除 SKU 的价格、主图、条码和供给关联无法继承；保存商品后生效。`,
                details: removed.map(
                    (sku) => `${sku.skuNo || "未编码"} · ${sku.specLabel}`,
                ),
                confirmLabel: "应用规格",
                destructive: true,
                onConfirm: () => setFields(next),
            })
            return null
        }
        setFields(next)
        return null
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
        (spec) => spec.name.trim() && spec.values.some((value) => value.trim()),
    )

    const applyBatchReferencePrices = () => {
        const hasAny =
            values.batchSalePrice.trim() || values.batchMarketPrice.trim()
        if (!hasAny) return
        const sale = values.batchSalePrice.trim()
        const market = values.batchMarketPrice.trim()
        const overwritten = values.fields.skus.filter(
            (sku) =>
                (sale && sku.salePrice?.trim() && sku.salePrice !== sale) ||
                (market &&
                    sku.marketPrice?.trim() &&
                    sku.marketPrice !== market),
        )
        const fieldsChanged = [sale && "销售价", market && "市场价"]
            .filter(Boolean)
            .join("、")
        const previous = fields.skus
        const apply = () => {
            setFields((current) => ({
                ...current,
                skus: current.skus.map((sku) => ({
                    ...sku,
                    salePrice: sale || sku.salePrice,
                    marketPrice: market || sku.marketPrice,
                })),
            }))
            offerUndo?.(() =>
                setFields((current) => ({
                    ...current,
                    skus: current.skus.map((sku) => {
                        const old = previous.find(
                            (before) =>
                                before.specificationSignature ===
                                    sku.specificationSignature &&
                                before.skuNo === sku.skuNo,
                        )
                        if (!old) return sku
                        return {
                            ...sku,
                            salePrice:
                                sale && sku.salePrice === sale
                                    ? old.salePrice
                                    : sku.salePrice,
                            marketPrice:
                                market && sku.marketPrice === market
                                    ? old.marketPrice
                                    : sku.marketPrice,
                        }
                    }),
                })),
            )
        }
        if (overwritten.length) {
            confirmChange({
                title: `覆盖 ${overwritten.length} 个 SKU 的价格？`,
                description: `本次应用${fieldsChanged}，保存商品后生效。`,
                details: overwritten.map(
                    (sku) => `${sku.skuNo || "未编码"} · ${sku.specLabel}`,
                ),
                confirmLabel: "应用价格",
                onConfirm: apply,
            })
        } else apply()
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
        applySpecDrafts,
        resetSpecDrafts,
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
