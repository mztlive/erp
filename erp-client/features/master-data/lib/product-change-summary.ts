import type { ProductEditorFormValues } from "./product-editor-model"

/** Compare business fields only; batch-input drafts and local preview URLs are not saved changes. */
export function productChangeSummary(
    previous: ProductEditorFormValues,
    next: ProductEditorFormValues,
): string[] {
    const changes: string[] = []
    const old = previous.fields
    const fields = next.fields
    if (previous.name !== next.name) changes.push("商品名称")
    for (const [key, label] of [
        ["productNo", "商品编号"],
        ["description", "商品描述"],
        ["brandId", "品牌"],
        ["categoryId", "商品分类"],
        ["baseUnitId", "基础单位"],
        ["productKind", "商品类型"],
    ] as const) {
        if (old[key] !== fields[key]) changes.push(label)
    }
    if (JSON.stringify(previous.specDrafts) !== JSON.stringify(next.specDrafts))
        changes.push("商品规格")
    for (const [key, label] of [
        ["carouselImages", "轮播图"],
        ["detailImages", "详情图"],
    ] as const) {
        const assetKey =
            key === "carouselImages"
                ? "carouselFileAssetIds"
                : "detailFileAssetIds"
        if (
            JSON.stringify(old[key]) !== JSON.stringify(fields[key]) ||
            JSON.stringify(old[assetKey]) !== JSON.stringify(fields[assetKey])
        )
            changes.push(label)
    }
    if (old.skus.length !== fields.skus.length) {
        changes.push(`SKU 数量：${old.skus.length} → ${fields.skus.length}`)
    }
    for (const sku of fields.skus) {
        const before = old.skus.find((item) =>
            sku.skuId
                ? item.skuId === sku.skuId
                : sku.specificationSignature
                  ? item.specificationSignature === sku.specificationSignature
                  : item.skuNo === sku.skuNo,
        )
        if (!before) continue
        const label = sku.name || sku.specLabel || sku.skuNo
        for (const [key, title] of [
            ["salePrice", "销售价"],
            ["marketPrice", "市场价"],
        ] as const) {
            if ((before[key] ?? "") !== (sku[key] ?? "")) {
                changes.push(
                    `${label} · ${title}：${before[key] || "未设置"} → ${sku[key] || "未设置"}`,
                )
            }
        }
        if (
            [
                "name",
                "skuNo",
                "barcode",
                "mainImage",
                "mainImageAssetId",
                "lifecycleStatus",
            ].some(
                (key) =>
                    before[key as keyof typeof before] !==
                    sku[key as keyof typeof sku],
            )
        )
            changes.push(`${label} · SKU 资料`)
    }
    if (
        previous.effectiveFrom !== next.effectiveFrom ||
        previous.effectiveTo !== next.effectiveTo
    )
        changes.push("生效时间")
    if (!changes.length && previous.changeReason !== next.changeReason)
        changes.push("变更原因")
    return changes
}
