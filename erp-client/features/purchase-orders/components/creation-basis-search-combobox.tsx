"use client"

import * as React from "react"

import {
    OptionCombobox,
    type OptionComboboxProps,
} from "@/components/business/option-combobox"
import { useCreationBasesQuery } from "@/features/purchase-orders/hooks/queries"
import {
    PURCHASE_TYPE_LABEL,
    type PurchaseCreationBasis,
} from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api/errors"

export type CreationBasisSearchComboboxProps = Omit<
    OptionComboboxProps,
    "options" | "loading"
> & {
    onItemChange?: (basis?: PurchaseCreationBasis) => void
}

/** 可消费的采购创建依据；请求与缓存由组件统一持有。 */
export function CreationBasisSearchCombobox({
    value,
    onValueChange,
    onItemChange,
    emptyLabel,
    ...props
}: CreationBasisSearchComboboxProps) {
    const query = useCreationBasesQuery()
    const rows = React.useMemo(
        () => (query.data ?? []).filter((basis) => !basis.consumed),
        [query.data],
    )
    const onItemChangeRef = React.useRef(onItemChange)
    onItemChangeRef.current = onItemChange
    const hydratedValueRef = React.useRef<string | null>(null)

    React.useEffect(() => {
        const selected = rows.find((basis) => basis.basisId === value)
        if (!selected || hydratedValueRef.current === selected.basisId) return
        hydratedValueRef.current = selected.basisId
        onItemChangeRef.current?.(selected)
    }, [rows, value])

    return (
        <OptionCombobox
            {...props}
            value={value}
            onValueChange={(next) => {
                onValueChange(next)
                onItemChange?.(rows.find((basis) => basis.basisId === next))
            }}
            options={rows.map((basis) => ({
                value: basis.basisId,
                label: `${basis.salesOrderNo} · ${basis.supplierName} · ${PURCHASE_TYPE_LABEL[basis.purchaseType]} · 估 ${basis.estimatedGross}`,
                keywords: `${basis.salesOrderId} ${basis.supplierId}`,
            }))}
            loading={query.isFetching}
            emptyLabel={
                query.isError
                    ? getErrorMessage(
                          query.error,
                          "采购创建依据加载失败，请重试",
                      )
                    : emptyLabel
            }
        />
    )
}
