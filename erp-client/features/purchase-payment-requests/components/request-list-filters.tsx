"use client"

import { PaymentTextFilterBar } from "./payment-text-filter-bar"
import {
    emptyRequestFilters,
    requestFilterFields,
    type RequestFilters,
} from "../lib/list-query"

export function RequestListFilters({
    applied,
    onApply,
    onReset,
    scopeChip,
    view,
}: {
    applied: RequestFilters
    onApply: (filters: RequestFilters) => void
    onReset?: () => void
    scopeChip?: { label: string; onClear: () => void }
    view: "effective" | "draft"
}) {
    return (
        <PaymentTextFilterBar
            idPrefix="purchase-payment-requests-filters"
            formAriaLabel="付款申请查询条件"
            fields={requestFilterFields}
            applied={applied}
            empty={emptyRequestFilters}
            onApply={onApply}
            onReset={onReset}
            chips={
                scopeChip
                    ? [
                          {
                              key: "purchase_order_id",
                              label: scopeChip.label,
                              onClear: scopeChip.onClear,
                          },
                      ]
                    : []
            }
            idleHint={
                view === "draft"
                    ? "未提交、已驳回和已撤回的申请，可修改后提交。"
                    : "已提交的申请及付款记录，包含已关闭申请。"
            }
        />
    )
}
