"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import type { SourcingSupplierOption } from "@/features/purchase-orders/lib/purchase-order-create-model"

export type PurchaseOrderCreateBatchBarProps = {
    selectedCount: number
    options: readonly SourcingSupplierOption[]
    disabled?: boolean
    onApply: (supplierId: string) => void
}

/**
 * 把勾选明细批量指定为同一家共同可选供应商。
 */
export function PurchaseOrderCreateBatchBar({
    selectedCount,
    options,
    disabled,
    onApply,
}: PurchaseOrderCreateBatchBarProps) {
    const [supplierId, setSupplierId] = React.useState<string | null>(null)
    React.useEffect(() => {
        if (!options.some((option) => option.supplierId === supplierId)) {
            setSupplierId(options[0]?.supplierId ?? null)
        }
    }, [options, supplierId])

    return (
        <div className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-3 py-2">
            <p className="text-sm text-muted-foreground">
                已选 {selectedCount} 行
            </p>
            <OptionCombobox
                className="w-64"
                value={supplierId}
                onValueChange={setSupplierId}
                allowClear={false}
                disabled={disabled || options.length === 0}
                placeholder={
                    options.length === 0
                        ? "选中行没有共同可选供应商"
                        : "批量指定供应商"
                }
                aria-label="批量指定供应商"
                options={options.map((option) => ({
                    value: option.supplierId,
                    label: option.supplierName,
                    keywords: option.supplierId,
                }))}
            />
            <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={disabled || !supplierId || options.length === 0}
                onClick={() => {
                    if (supplierId) onApply(supplierId)
                }}
                data-testid="purchase-create-batch-apply"
            >
                应用到选中行
            </Button>
        </div>
    )
}
