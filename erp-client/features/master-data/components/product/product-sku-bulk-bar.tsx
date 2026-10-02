"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

type SkuBulkPriceBarProps = {
    canRevise: boolean
    batchFactoryPrice: string
    batchSalePrice: string
    batchBulkPrice: string
    batchBulkMinQuantity: string
    batchMarketPrice: string
    setBatchFactoryPrice: (next: string) => void
    setBatchSalePrice: (next: string) => void
    setBatchBulkPrice: (next: string) => void
    setBatchBulkMinQuantity: (next: string) => void
    setBatchMarketPrice: (next: string) => void
    onApplyBatchReferencePrices: () => void
}

function SkuBulkPriceBar({
    canRevise,
    batchFactoryPrice,
    batchSalePrice,
    batchBulkPrice,
    batchBulkMinQuantity,
    batchMarketPrice,
    setBatchFactoryPrice,
    setBatchSalePrice,
    setBatchBulkPrice,
    setBatchBulkMinQuantity,
    setBatchMarketPrice,
    onApplyBatchReferencePrices,
}: SkuBulkPriceBarProps) {
    const fields = [
        {
            key: "factory-price",
            label: "批量出厂价",
            value: batchFactoryPrice,
            onChange: setBatchFactoryPrice,
        },
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
        <div className="grid gap-2 rounded-xl border border-border bg-surface-sunken p-3 sm:grid-cols-2 lg:grid-cols-3">
            {fields.map((field) => (
                <div key={field.key} className="space-y-1">
                    <Label
                        htmlFor={`master-data-product-sku-bulk-${field.key}`}
                        className="text-xs"
                    >
                        {field.label}
                    </Label>
                    <Input
                        id={`master-data-product-sku-bulk-${field.key}`}
                        className="h-control-sm bg-background"
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
