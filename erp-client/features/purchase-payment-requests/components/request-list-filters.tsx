"use client"

import { PaymentTextFilterBar } from "./payment-text-filter-bar"
import {
    emptyRequestFilters,
    requestFilterFields,
    type RequestFilters,
} from "../lib/list-query"

export function RequestListFilters({
    applied,
    canFilterSupplier,
    onApply,
    onReset,
    scopeChip,
    resultStatus,
}: {
    canFilterSupplier: boolean
    applied: RequestFilters
    onApply: (filters: RequestFilters, clearScope?: boolean) => void
    onReset: () => void
    scopeChip?: { label: string; onClear: () => void }
    resultStatus?: string | null
}) {
    return (
        <PaymentTextFilterBar
            idPrefix="purchase-payment-requests-filters"
            formAriaLabel="付款申请查询条件"
            fields={requestFilterFields.filter(
                ([key]) => key !== "supplier_id" || canFilterSupplier,
            )}
            moreLegend="申请来源"
            searchKey="request_no"
            applied={applied}
            empty={emptyRequestFilters}
            onApply={onApply}
            onReset={onReset}
            scopeChip={
                scopeChip
                    ? { key: "purchase_order_id", ...scopeChip }
                    : undefined
            }
            resultStatus={resultStatus}
        />
    )
}
