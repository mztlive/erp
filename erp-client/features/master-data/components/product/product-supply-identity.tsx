import type {
    ProductFields,
    ProductSkuFields,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"

/** 按商品维度顺序展示完整规格，突出不同 SKU 之间有差异的值。 */
export function ProductSupplyIdentity({
    sku,
    fields,
    productName,
    titleId,
    differingDimensions,
}: {
    sku: ProductSkuFields
    fields: ProductFields
    productName: string
    titleId: string
    differingDimensions: readonly boolean[]
}) {
    const dimensions = fields.specs.map((spec, index) => ({
        name: spec.name,
        value: sku.attributeValues[index]?.trim() || "未填写",
        differs: differingDimensions[index],
    }))
    const showName = Boolean(
        sku.name.trim() &&
        (sku.name.trim() !== productName.trim() || dimensions.length === 0),
    )
    const hasDistinctImage = Boolean(
        sku.mainImagePreviewUrl &&
        !fields.carouselImages.includes(sku.mainImage) &&
        !Object.values(fields.carouselFileAssetIds).includes(
            sku.mainImageAssetId ?? "",
        ) &&
        !Object.values(fields.carouselPreviewUrls).includes(
            sku.mainImagePreviewUrl,
        ) &&
        fields.skus.some(
            (other) => other.mainImagePreviewUrl !== sku.mainImagePreviewUrl,
        ),
    )
    const hasDifferences = differingDimensions.some(Boolean)

    return (
        <div className="min-w-0 flex-1 space-y-3">
            <h3 id={titleId} className="flex flex-wrap gap-x-8 gap-y-3">
                {dimensions.length > 0 ? (
                    dimensions.map((dimension) => (
                        <span
                            key={dimension.name}
                            className="min-w-0 max-w-full"
                        >
                            <span className="block text-xs font-normal text-muted-foreground wrap-anywhere">
                                {dimension.name}
                            </span>
                            <span
                                className={cn(
                                    "mt-1 block text-2xl leading-snug text-foreground wrap-anywhere",
                                    dimension.differs || !hasDifferences
                                        ? "font-semibold"
                                        : "font-normal",
                                )}
                            >
                                {dimension.value}
                            </span>
                        </span>
                    ))
                ) : (
                    <span className="min-w-0">
                        <span className="block text-xs font-normal text-muted-foreground">
                            规格
                        </span>
                        <span className="mt-1 block text-2xl leading-snug font-semibold wrap-anywhere">
                            {sku.specLabel.trim() || "默认规格"}
                        </span>
                    </span>
                )}
            </h3>
            <div className="flex min-w-0 items-center gap-3">
                {hasDistinctImage ? (
                    // eslint-disable-next-line @next/next/no-img-element -- 复用商品页已解析的 SKU 图片地址。
                    <img
                        src={sku.mainImagePreviewUrl}
                        alt={sku.name || sku.specLabel}
                        className="size-10 shrink-0 rounded-md object-cover"
                    />
                ) : null}
                <div className="min-w-0 space-y-1">
                    {showName ? (
                        <p className="text-sm wrap-anywhere">{sku.name}</p>
                    ) : null}
                    <p className="num text-xs text-muted-foreground wrap-anywhere select-text">
                        SKU 编号：{sku.skuNo}
                    </p>
                </div>
            </div>
        </div>
    )
}
