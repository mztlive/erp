"use client"

import * as React from "react"
import { ChevronDownIcon } from "lucide-react"
import { Button } from "@/components/ui/button"

import { Badge } from "@/components/ui/badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { ProductSpecDraftsEditor } from "@/features/master-data/components/product/product-spec-drafts"
import { ProductSkuTable } from "@/features/master-data/components/product/product-sku-table"
import { SkuBulkPriceBar } from "@/features/master-data/components/product/product-sku-bulk-bar"
import {
    hasPendingSpecs,
    type ProductSpecDraft,
} from "@/features/master-data/lib/product-editor-model"
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
    applySpecDrafts: () => string | null
    resetSpecDrafts: () => void
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
    rememberSkuFile: (previewUrl: string, file: File) => void
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
    applySpecDrafts,
    resetSpecDrafts,
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
    const [specError, setSpecError] = React.useState<string | null>(null)
    const specsPending = hasPendingSpecs(specDrafts, fields)
    React.useEffect(() => {
        if (specsPending) setSpecOpen(true)
    }, [specsPending])
    const showInventory = fields.productKind === "PHYSICAL"
    const prefix = idPrefix ?? "master-data-product-sku"
    return (
        <section
            id="product-section-sku"
            aria-labelledby={`${prefix}-heading`}
            className="min-w-0 max-w-full scroll-mt-4 space-y-4 [&>fieldset]:min-w-0"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1.5">
                    <div className="flex flex-wrap items-center gap-2">
                        <h2
                            id={`${prefix}-heading`}
                            className="text-base font-semibold tracking-tight"
                        >
                            规格与 SKU
                        </h2>
                        <Badge variant="secondary">
                            {fields.skus.length} 个 SKU
                        </Badge>
                        {specsPending ? (
                            <Badge variant="secondary">规格待应用</Badge>
                        ) : null}
                    </div>
                    <p className="break-words text-sm text-muted-foreground">
                        {activeSpecs
                            .map(
                                (spec) =>
                                    `${spec.name}：${spec.values.join("、")}`,
                            )
                            .join(" · ") || "默认规格"}
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id={`${prefix}-spec-toggle`}
                        type="button"
                        variant="outline"
                        size="sm"
                        aria-expanded={specOpen}
                        aria-controls={`${prefix}-spec-content`}
                        onClick={() => setSpecOpen(!specOpen)}
                    >
                        {specOpen
                            ? "收起规格"
                            : canRevise
                              ? "修改规格"
                              : "查看规格"}
                        <ChevronDownIcon
                            className={
                                specOpen ? "size-4 rotate-180" : "size-4"
                            }
                            aria-hidden
                        />
                    </Button>
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
                    {showInventory ? (
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
                    ) : null}
                </div>
            </div>
            <div
                id={`${prefix}-spec-content`}
                hidden={!specOpen}
                className="rounded-lg border border-border bg-muted/20 p-3"
            >
                <ProductSpecDraftsEditor
                    idPrefix={`${prefix}-spec`}
                    canRevise={canRevise}
                    specDrafts={specDrafts}
                    skuCount={fields.skus.length}
                    syncSpecDrafts={(next) => {
                        setSpecError(null)
                        syncSpecDrafts(next)
                    }}
                />
                {specError ? (
                    <p
                        id={`${prefix}-spec-error`}
                        role="alert"
                        className="mt-3 text-sm text-destructive"
                    >
                        {specError}
                    </p>
                ) : null}
                {canRevise && specsPending ? (
                    <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border pt-3">
                        <p className="w-full text-sm text-muted-foreground sm:w-auto sm:min-w-0 sm:flex-1">
                            规格修改尚未应用，SKU 表保留已应用的内容。
                        </p>
                        <Button
                            id={`${prefix}-spec-reset`}
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                resetSpecDrafts()
                                setSpecError(null)
                            }}
                        >
                            取消规格修改
                        </Button>
                        <Button
                            id={`${prefix}-spec-apply`}
                            type="button"
                            size="sm"
                            onClick={() => setSpecError(applySpecDrafts())}
                        >
                            应用规格
                        </Button>
                    </div>
                ) : null}
            </div>
            {fields.skus.length > 1 && bulkOpen ? (
                <SkuBulkPriceBar
                    canRevise={canRevise}
                    batchSalePrice={batchSalePrice}
                    batchMarketPrice={batchMarketPrice}
                    setBatchSalePrice={setBatchSalePrice}
                    setBatchMarketPrice={setBatchMarketPrice}
                    onApplyBatchReferencePrices={onApplyBatchReferencePrices}
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
        </section>
    )
}

export { ProductSkuSection }
