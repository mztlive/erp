"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

type SkuBulkPriceBarProps = {
    canRevise: boolean
    batchSalePrice: string
    batchBulkPrice: string
    batchBulkMinQuantity: string
    batchMarketPrice: string
    setBatchSalePrice: (next: string) => void
    setBatchBulkPrice: (next: string) => void
    setBatchBulkMinQuantity: (next: string) => void
    setBatchMarketPrice: (next: string) => void
    onApplyBatchReferencePrices: () => void
}

function SkuBulkPriceBar({
    canRevise,
    batchSalePrice,
    batchBulkPrice,
    batchBulkMinQuantity,
    batchMarketPrice,
    setBatchSalePrice,
    setBatchBulkPrice,
    setBatchBulkMinQuantity,
    setBatchMarketPrice,
    onApplyBatchReferencePrices,
}: SkuBulkPriceBarProps) {
    const fields = [
        {
            key: "sale-price",
            label: "批量一件代发价",
            value: batchSalePrice,
            onChange: setBatchSalePrice,
        },
        {
            key: "bulk-price",
            label: "批量集采价",
            value: batchBulkPrice,
            onChange: setBatchBulkPrice,
        },
        {
            key: "bulk-min-quantity",
            label: "批量集采起订量",
            value: batchBulkMinQuantity,
            onChange: setBatchBulkMinQuantity,
        },
        {
            key: "market-price",
            label: "批量市场价",
            value: batchMarketPrice,
            onChange: setBatchMarketPrice,
        },
    ]
    return (
        <div className="grid gap-3 rounded-lg border border-border/70 bg-surface-sunken/70 p-3.5 sm:grid-cols-2 lg:grid-cols-5">
            {fields.map((field) => (
                <div key={field.key} className="space-y-1">
                    <Label
                        htmlFor={`master-data-product-sku-bulk-${field.key}`}
                        className="text-xs font-medium text-foreground/70"
                    >
                        {field.label}
                    </Label>
                    <Input
                        id={`master-data-product-sku-bulk-${field.key}`}
                        className="h-control-sm num bg-card text-right"
                        inputMode="decimal"
                        value={field.value}
                        disabled={!canRevise}
                        onChange={(event) => field.onChange(event.target.value)}
                        placeholder="可选"
                    />
                </div>
            ))}
            <Button
                id="master-data-product-product-sku-bulk-bar-button-1"
                type="button"
                variant="secondary"
                size="sm"
                className="self-end"
                disabled={
                    !canRevise || fields.every((field) => !field.value.trim())
                }
                onClick={onApplyBatchReferencePrices}
            >
                应用到全部 SKU
            </Button>
        </div>
    )
}

export { SkuBulkPriceBar }
