"use client"

import { PaymentTextFilterBar } from "./payment-text-filter-bar"
import {
    emptyCandidateFilters,
    candidateFilterFields,
    type CandidateFilters,
} from "../lib/candidate-list-query"

export function CandidateListFilters({
    applied,
    canFilterSupplier,
    onApply,
    onReset,
    resultStatus,
}: {
    canFilterSupplier: boolean
    applied: CandidateFilters
    onApply: (filters: CandidateFilters) => void
    onReset: () => void
    resultStatus?: string | null
}) {
    return (
        <PaymentTextFilterBar
            idPrefix="purchase-payment-candidates-filters"
            formAriaLabel="可申请采购单查询条件"
            fields={candidateFilterFields.filter(
                ([key]) => key !== "supplier_id" || canFilterSupplier,
            )}
            moreLegend="采购内容"
            searchKey="purchase_order_no"
            applied={applied}
            empty={emptyCandidateFilters}
            onApply={onApply}
            onReset={onReset}
            resultStatus={resultStatus}
        />
    )
}
