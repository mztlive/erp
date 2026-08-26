"use client"

import * as React from "react"
import { SparklesIcon } from "lucide-react"

import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import type { SourcingSupplierOption } from "@/features/purchase-orders/lib/purchase-order-create-model"
import { FULFILLMENT_RESPONSIBILITY_LABEL } from "@/features/purchase-orders/types"

export type PurchaseOrderCreateBatchBarProps = {
    selectedCount: number
    options: readonly SourcingSupplierOption[]
    disabled?: boolean
    matchDisabled?: boolean
    onApply: (basisId: string) => void
    onMatchBest: () => void
}

/**
 * 批量指定共同履约方案，或一键为全部明细匹配最优方案。
 */
export function PurchaseOrderCreateBatchBar({
    selectedCount,
    options,
    disabled,
    matchDisabled,
    onApply,
    onMatchBest,
}: PurchaseOrderCreateBatchBarProps) {
    const [basisId, setBasisId] = React.useState<string | null>(null)
    React.useEffect(() => {
        if (!options.some((option) => option.basisId === basisId)) {
            setBasisId(options[0]?.basisId ?? null)
        }
    }, [basisId, options])

    return (
        <div className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-3 py-2">
            <p className="text-sm text-muted-foreground">
                已选 {selectedCount} 行
            </p>
            <OptionCombobox
                className="w-64"
                value={basisId}
                onValueChange={setBasisId}
                allowClear={false}
                disabled={disabled || options.length === 0}
                placeholder={
                    selectedCount === 0 && options.length === 0
                        ? "请先勾选明细"
                        : options.length === 0
                          ? "选中行没有可指定的履约方案"
                          : "批量指定履约方案"
                }
                aria-label="批量指定履约方案"
                options={options.map((option) => ({
                    value: option.basisId,
                    label:
                        option.sourceType === "EXISTING_STOCK"
                            ? `${option.supplierName} · 现货`
                            : `${option.supplierName} · ${FULFILLMENT_RESPONSIBILITY_LABEL[option.fulfillmentResponsibility]}`,
                    keywords: `${option.sourceType} ${option.supplierId} ${option.warehouseName ?? ""} ${option.fulfillmentResponsibility}`,
                }))}
            />
            <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={disabled || !basisId || options.length === 0}
                onClick={() => {
                    if (basisId) onApply(basisId)
                }}
                data-testid="purchase-create-batch-apply"
            >
                应用到选中行
            </Button>
            <Button
                type="button"
                size="sm"
                variant="outline"
                className="sm:ml-auto"
                disabled={disabled || matchDisabled}
                onClick={onMatchBest}
                data-testid="purchase-create-match-best"
            >
                <SparklesIcon data-icon="inline-start" />
                重新自动分配
            </Button>
        </div>
    )
}
