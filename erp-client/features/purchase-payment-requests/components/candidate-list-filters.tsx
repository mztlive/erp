"use client"

import { PaymentTextFilterBar } from "./payment-text-filter-bar"
import {
    emptyCandidateFilters,
    candidateFilterFields,
    type CandidateFilters,
} from "../lib/candidate-list-query"

export function CandidateListFilters({
    applied,
    onApply,
}: {
    applied: CandidateFilters
    onApply: (filters: CandidateFilters) => void
}) {
    return (
        <PaymentTextFilterBar
            idPrefix="purchase-payment-candidates-filters"
            formAriaLabel="可申请采购单查询条件"
            fields={candidateFilterFields}
            applied={applied}
            empty={emptyCandidateFilters}
            onApply={onApply}
            idleHint="仅显示已生效、有可申请余额且你有申请资格的采购单。支持跨页勾选，按供应商和我方主体分别生成申请。"
        />
    )
}
