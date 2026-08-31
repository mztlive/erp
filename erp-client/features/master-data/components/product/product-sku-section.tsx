"use client"

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
    const prefix = idPrefix ?? "master-data-product-sku"
    return (
        <div id="product-section-sku" className="min-w-0 max-w-full space-y-8">
            <ProductSpecDraftsEditor
                idPrefix={`${prefix}-spec`}
                canRevise={canRevise}
                specDrafts={specDrafts}
                skuCount={fields.skus.length}
                syncSpecDrafts={syncSpecDrafts}
            />
            <ProductSectionFrame
                title="SKU"
                description={masterDataCopy.productSkuHint}
                extra={
                    <Badge variant="success">
                        共 {fields.skus.length} 个 SKU
                    </Badge>
                }
            >
                <SkuBulkPriceBar
                    canRevise={canRevise}
                    batchSalePrice={batchSalePrice}
                    batchMarketPrice={batchMarketPrice}
                    setBatchSalePrice={setBatchSalePrice}
                    setBatchMarketPrice={setBatchMarketPrice}
                    onApplyBatchReferencePrices={onApplyBatchReferencePrices}
                    inventoryActionHint={inventoryActionHint}
                    onOpenInventory={onOpenInventory}
                    inventoryPreviewSkuId={inventoryPreviewSkus[0]?.skuId}
                />
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
