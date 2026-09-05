"use client"

import * as React from "react"
import { ChevronDownIcon, PackageIcon } from "lucide-react"
import { Button } from "@/components/ui/button"

import { Badge } from "@/components/ui/badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { ProductSectionFrame } from "@/features/master-data/components/product/product-section-frame"
import { ProductSpecDraftsEditor } from "@/features/master-data/components/product/product-spec-drafts"
import { ProductSkuTable } from "@/features/master-data/components/product/product-sku-table"
import { SkuBulkPriceBar } from "@/features/master-data/components/product/product-sku-bulk-bar"
import type { ProductSpecDraft } from "@/features/master-data/lib/product-editor-model"
import type { ProductInventoryPreviewSku } from "@/features/master-data/components/product/product-inventory-preview-sheet"
import type {
    ProductFields,
    ProductSkuFields,
    ProductSpecDimension,
} from "@/features/master-data/types"
import type { FixedSku } from "@/features/supplier-offerings/types"

type ProductSkuSectionProps = {
    idPrefix?: string
    isCreate: boolean
    canRevise: boolean
    name: string
    fields: ProductFields
    specDrafts: readonly ProductSpecDraft[]
    activeSpecs: readonly ProductSpecDimension[]
    inventoryPreviewSkus: readonly ProductInventoryPreviewSku[]
    syncSpecDrafts: (next: readonly ProductSpecDraft[]) => void
    updateSku: (index: number, patch: Partial<ProductSkuFields>) => void
    batchSalePrice: string
    batchMarketPrice: string
    setBatchSalePrice: (next: string) => void
    setBatchMarketPrice: (next: string) => void
    onApplyBatchReferencePrices: () => void
    inventoryActionHint: string | undefined
    onOpenInventory: (
        skuId: string | undefined,
        trigger: HTMLButtonElement,
    ) => void
    rememberSkuFile: (index: number, file?: File) => void
    supplierCounts: Map<string, number> | undefined
    supplierCountsPending: boolean
    supplierCountsError: unknown
    onRegisterSupply: (sku: FixedSku) => void
    stableId: string
}

function ProductSkuSection({
    idPrefix,
    isCreate,
    canRevise,
    name,
    fields,
    specDrafts,
    activeSpecs,
    inventoryPreviewSkus,
    syncSpecDrafts,
    updateSku,
    batchSalePrice,
    batchMarketPrice,
    setBatchSalePrice,
    setBatchMarketPrice,
    onApplyBatchReferencePrices,
    inventoryActionHint,
    onOpenInventory,
    rememberSkuFile,
    supplierCounts,
    supplierCountsPending,
    supplierCountsError,
    onRegisterSupply,
    stableId,
}: ProductSkuSectionProps) {
    const [specOpen, setSpecOpen] = React.useState(isCreate)
    const [bulkOpen, setBulkOpen] = React.useState(false)
    const prefix = idPrefix ?? "master-data-product-sku"
    return (
        <div
            id="product-section-sku"
            className="min-w-0 max-w-full space-y-5 [&>fieldset]:min-w-0"
        >
            <div className="rounded-lg border border-border bg-muted/25">
                <button
                    id={`${prefix}-spec-toggle`}
                    type="button"
                    className="flex w-full items-center gap-3 px-4 py-3 text-left text-sm"
                    aria-expanded={specOpen}
                    aria-controls={`${prefix}-spec-content`}
                    onClick={() => setSpecOpen(!specOpen)}
                >
                    <PackageIcon
                        className="size-4 shrink-0 text-muted-foreground"
                        aria-hidden
                    />
                    <span className="min-w-0 flex-1 break-words">
                        {activeSpecs
                            .map(
                                (spec) =>
                                    `${spec.name}：${spec.values.join("、")}`,
                            )
                            .join(" · ") || "默认规格"}
                    </span>
                    <span className="shrink-0 text-muted-foreground">
                        {specOpen ? "收起规格" : "修改规格"}
                    </span>
                    <ChevronDownIcon
                        className={specOpen ? "size-4 rotate-180" : "size-4"}
                        aria-hidden
                    />
                </button>
                <div
                    id={`${prefix}-spec-content`}
                    hidden={!specOpen}
                    className="border-t border-border bg-card p-3"
                >
                    <ProductSpecDraftsEditor
                        idPrefix={`${prefix}-spec`}
                        canRevise={canRevise}
                        specDrafts={specDrafts}
                        skuCount={fields.skus.length}
                        syncSpecDrafts={syncSpecDrafts}
                    />
                </div>
            </div>
            <ProductSectionFrame
                title="规格与 SKU"
                description="主图和价格可直接修改；编码、名称和条码在 SKU 详情中维护。"
                extra={
                    <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="secondary">
                            {fields.skus.length} 个 SKU
                        </Badge>
                        {fields.skus.length > 1 && canRevise ? (
                            <Button
                                id={`${prefix}-bulk-toggle`}
                                type="button"
                                variant="outline"
                                size="sm"
                                aria-expanded={bulkOpen}
                                onClick={() => setBulkOpen(!bulkOpen)}
                            >
                                批量设置价格
                            </Button>
                        ) : null}
                        <Button
                            id={`${prefix}-inventory`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={Boolean(inventoryActionHint)}
                            title={inventoryActionHint}
                            onClick={(event) =>
                                onOpenInventory(
                                    inventoryPreviewSkus[0]?.skuId,
                                    event.currentTarget,
                                )
                            }
                        >
                            查看商品库存
                        </Button>
                    </div>
                }
            >
                {fields.skus.length > 1 && bulkOpen ? (
                    <SkuBulkPriceBar
                        canRevise={canRevise}
                        batchSalePrice={batchSalePrice}
                        batchMarketPrice={batchMarketPrice}
                        setBatchSalePrice={setBatchSalePrice}
                        setBatchMarketPrice={setBatchMarketPrice}
                        onApplyBatchReferencePrices={
                            onApplyBatchReferencePrices
                        }
                    />
                ) : null}
                {fields.skus.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        {masterDataCopy.productNoSkus}
                    </p>
                ) : (
                    <ProductSkuTable
                        fields={fields}
                        activeSpecs={activeSpecs}
                        isCreate={isCreate}
                        canRevise={canRevise}
                        name={name}
                        updateSku={updateSku}
                        rememberSkuFile={rememberSkuFile}
                        onOpenInventory={onOpenInventory}
                        supplierCounts={supplierCounts}
                        supplierCountsPending={supplierCountsPending}
                        supplierCountsError={supplierCountsError}
                        onRegisterSupply={onRegisterSupply}
                        stableId={stableId}
                    />
                )}
            </ProductSectionFrame>
        </div>
    )
}

export { ProductSkuSection }
