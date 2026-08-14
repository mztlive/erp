"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

type SkuBulkPriceBarProps = {
    canRevise: boolean
    batchSalePrice: string
    batchMarketPrice: string
    setBatchSalePrice: (next: string) => void
    setBatchMarketPrice: (next: string) => void
    onApplyBatchReferencePrices: () => void
    inventoryActionHint: string | undefined
    onOpenInventory: (skuId: string | undefined, trigger: HTMLButtonElement) => void
    inventoryPreviewSkuId: string | undefined
}

function SkuBulkPriceBar({
    canRevise,
    batchSalePrice,
    batchMarketPrice,
    setBatchSalePrice,
    setBatchMarketPrice,
    onApplyBatchReferencePrices,
    inventoryActionHint,
    onOpenInventory,
    inventoryPreviewSkuId,
}: SkuBulkPriceBarProps) {
    return (
        <div className="grid gap-2 rounded-xl border border-border bg-surface-sunken p-3 sm:grid-cols-2 lg:grid-cols-[repeat(2,minmax(0,1fr))_auto_auto]">
            <div className="space-y-1">
                <Label htmlFor="bulk-sale-price" className="text-xs">
                    批量销售价
                </Label>
                <Input
                    id="bulk-sale-price"
                    className="h-8 bg-background"
                    value={batchSalePrice}
                    disabled={!canRevise}
                    onChange={(event) => setBatchSalePrice(event.target.value)}
                    placeholder="可选"
                />
            </div>
            <div className="space-y-1">
                <Label htmlFor="bulk-market-price" className="text-xs">
                    批量市场价
                </Label>
                <Input
                    id="bulk-market-price"
                    className="h-8 bg-background"
                    value={batchMarketPrice}
                    disabled={!canRevise}
                    onChange={(event) => setBatchMarketPrice(event.target.value)}
                    placeholder="可选"
                />
            </div>
            <Button
                type="button"
                variant="secondary"
                size="sm"
                className="self-end"
                disabled={
                    !canRevise ||
                    (!batchSalePrice.trim() && !batchMarketPrice.trim())
                }
                onClick={onApplyBatchReferencePrices}
            >
                批量设置
            </Button>
            <Button
                type="button"
                variant="outline"
                size="sm"
                className="self-end"
                disabled={Boolean(inventoryActionHint)}
                title={inventoryActionHint}
                onClick={(event) =>
                    onOpenInventory(inventoryPreviewSkuId, event.currentTarget)
                }
            >
                查看商品库存
            </Button>
        </div>
    )
}

export { SkuBulkPriceBar }
