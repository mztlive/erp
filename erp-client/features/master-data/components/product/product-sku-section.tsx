"use client"

import { Badge } from "@/components/ui/badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
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
import { cn } from "@/lib/utils"

type ProductSkuSectionProps = {
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
    return (
        <>
            <ProductSpecDraftsEditor
                canRevise={canRevise}
                specDrafts={specDrafts}
                skuCount={fields.skus.length}
                syncSpecDrafts={syncSpecDrafts}
            />
            <fieldset
                className={cn(
                    "min-w-0 max-w-full space-y-4 overflow-hidden border-b border-grid p-5 last:border-b-0",
                )}
            >
                <legend className="sr-only">SKU</legend>
                <div className="text-base font-semibold">SKU</div>
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="min-w-0 space-y-1">
                        <p className="text-xs text-muted-foreground">
                            {masterDataCopy.productSkuHint}
                        </p>
                    </div>
                    <Badge variant="success">
                        共 {fields.skus.length} 个 SKU
                    </Badge>
                </div>
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
            </fieldset>
        </>
    )
}

export { ProductSkuSection }
