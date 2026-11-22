"use client"

import * as React from "react"
import { BoxesIcon, ChevronDownIcon, SlidersHorizontalIcon } from "lucide-react"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { ProductSpecDraftsEditor } from "@/features/master-data/components/product/product-spec-drafts"
import {
    ProductSkuTable,
    type SkuSupplySummary,
} from "@/features/master-data/components/product/product-sku-table"
import { SkuBulkPriceBar } from "@/features/master-data/components/product/product-sku-bulk-bar"
import type { BatchReferencePriceDraft } from "@/features/master-data/lib/product-form-bindings"
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
import { cn } from "@/lib/utils"

type ProductSkuSectionProps = {
    idPrefix?: string
    isCreate: boolean
    canRevise: boolean
    canEditLifecycle: boolean
    name: string
    fields: ProductFields
    specDrafts: readonly ProductSpecDraft[]
    activeSpecs: readonly ProductSpecDimension[]
    inventoryPreviewSkus: readonly ProductInventoryPreviewSku[]
    syncSpecDrafts: (next: readonly ProductSpecDraft[]) => void
    applySpecDrafts: () => string | null
    resetSpecDrafts: () => void
    updateSku: (index: number, patch: Partial<ProductSkuFields>) => void
    onApplyBatchReferencePrices: (draft: BatchReferencePriceDraft) => void
    inventoryActionHint: string | undefined
    showSupplyColumn?: boolean
    supplyBySku?: ReadonlyMap<string, SkuSupplySummary>
    supplyLoading?: boolean
    supplyError?: boolean
    canViewCosts?: boolean
    onOpenSupply?: (skuId: string) => void
    onOpenInventory?: (
        skuId: string | undefined,
        trigger: HTMLButtonElement,
    ) => void
    rememberSkuFile: (previewUrl: string, file: File) => void
}

function ProductSkuSection({
    idPrefix,
    isCreate,
    canRevise,
    canEditLifecycle,
    name,
    fields,
    specDrafts,
    activeSpecs,
    inventoryPreviewSkus,
    syncSpecDrafts,
    applySpecDrafts,
    resetSpecDrafts,
    updateSku,
    onApplyBatchReferencePrices,
    inventoryActionHint,
    showSupplyColumn,
    supplyBySku,
    supplyLoading,
    supplyError,
    canViewCosts,
    onOpenSupply,
    onOpenInventory,
    rememberSkuFile,
}: ProductSkuSectionProps) {
    const [specOpen, setSpecOpen] = React.useState(isCreate)
    const [bulkOpen, setBulkOpen] = React.useState(false)
    const [specError, setSpecError] = React.useState<string | null>(null)
    const [batchSalePrice, setBatchSalePrice] = React.useState("")
    const [batchBulkPrice, setBatchBulkPrice] = React.useState("")
    const [batchBulkMinQuantity, setBatchBulkMinQuantity] = React.useState("")
    const [batchMarketPrice, setBatchMarketPrice] = React.useState("")
    const specsPending = hasPendingSpecs(specDrafts, fields)
    React.useEffect(() => {
        if (specsPending) setSpecOpen(true)
    }, [specsPending])
    const showInventory = fields.productKind === "PHYSICAL"
    const prefix = idPrefix ?? "master-data-product-sku"
    const specSummary =
        activeSpecs
            .map((spec) => `${spec.name}：${spec.values.join("、")}`)
            .join(" · ") || "默认规格"

    return (
        <DetailWorkspaceSection
            id="product-section-sku"
            title="规格与 SKU"
            headingId={`${prefix}-heading`}
            width="full"
            badge={
                <div className="flex flex-wrap items-center gap-1.5">
                    <span className="num rounded-sm bg-muted/80 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                        {fields.skus.length} 个 SKU
                    </span>
                    {specsPending ? (
                        <StatusBadge tone="warning" label="规格待应用" />
                    ) : null}
                </div>
            }
            subtitle={specSummary}
            actions={
                <>
                    <Button
                        id={`${prefix}-spec-toggle`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        aria-expanded={specOpen}
                        aria-controls={`${prefix}-spec-content`}
                        onClick={() => setSpecOpen(!specOpen)}
                    >
                        <SlidersHorizontalIcon
                            data-icon="inline-start"
                            className="size-3.5"
                            aria-hidden
                        />
                        {specOpen
                            ? "收起规格"
                            : canRevise
                              ? "修改规格"
                              : "查看规格"}
                        <ChevronDownIcon
                            className={cn(
                                "size-3.5 transition-transform duration-150",
                                specOpen && "rotate-180",
                            )}
                            aria-hidden
                        />
                    </Button>
                    {fields.skus.length > 1 && canRevise ? (
                        <Button
                            id={`${prefix}-bulk-toggle`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.subtleButton}
                            aria-expanded={bulkOpen}
                            onClick={() => setBulkOpen(!bulkOpen)}
                        >
                            批量设置价格
                        </Button>
                    ) : null}
                    {showInventory && onOpenInventory ? (
                        <Button
                            id={`${prefix}-inventory`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.subtleButton}
                            disabled={Boolean(inventoryActionHint)}
                            title={inventoryActionHint}
                            onClick={(event) =>
                                onOpenInventory(
                                    inventoryPreviewSkus[0]?.skuId,
                                    event.currentTarget,
                                )
                            }
                        >
                            <BoxesIcon
                                data-icon="inline-start"
                                className="size-3.5"
                                aria-hidden
                            />
                            查看商品库存
                        </Button>
                    ) : null}
                </>
            }
        >
            <div
                id={`${prefix}-spec-content`}
                hidden={!specOpen}
                className="rounded-lg border border-border/70 bg-surface-sunken/70 p-4"
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
                        className="mt-3 text-xs text-destructive"
                    >
                        {specError}
                    </p>
                ) : null}
                {canRevise && specsPending ? (
                    <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border/60 pt-3">
                        <p className="w-full text-xs text-muted-foreground sm:w-auto sm:min-w-0 sm:flex-1">
                            规格修改尚未应用，SKU 表保留已应用的内容。
                        </p>
                        <Button
                            id={`${prefix}-spec-reset`}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
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
                            variant="secondary"
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
                    batchBulkPrice={batchBulkPrice}
                    batchBulkMinQuantity={batchBulkMinQuantity}
                    batchMarketPrice={batchMarketPrice}
                    setBatchSalePrice={setBatchSalePrice}
                    setBatchBulkPrice={setBatchBulkPrice}
                    setBatchBulkMinQuantity={setBatchBulkMinQuantity}
                    setBatchMarketPrice={setBatchMarketPrice}
                    onApplyBatchReferencePrices={() =>
                        onApplyBatchReferencePrices({
                            salePrice: batchSalePrice,
                            bulkPriceGross: batchBulkPrice,
                            bulkMinQuantity: batchBulkMinQuantity,
                            marketPrice: batchMarketPrice,
                        })
                    }
                />
            ) : null}
            {fields.skus.length === 0 ? (
                <p className="py-4 text-body-compact text-muted-foreground">
                    {masterDataCopy.productNoSkus}
                </p>
            ) : (
                <ProductSkuTable
                    canEditLifecycle={canEditLifecycle}
                    fields={fields}
                    activeSpecs={activeSpecs}
                    canRevise={canRevise}
                    name={name}
                    showSupplyColumn={showSupplyColumn}
                    supplyBySku={supplyBySku}
                    supplyLoading={supplyLoading}
                    supplyError={supplyError}
                    canViewCosts={canViewCosts}
                    onOpenSupply={onOpenSupply}
                    updateSku={updateSku}
                    rememberSkuFile={rememberSkuFile}
                    onOpenInventory={onOpenInventory}
                />
            )}
        </DetailWorkspaceSection>
    )
}

export { ProductSkuSection }
