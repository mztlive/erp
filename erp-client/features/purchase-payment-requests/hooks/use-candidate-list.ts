"use client"

import { useCallback, useEffect, useMemo } from "react"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import type { SortingState } from "@tanstack/react-table"
import {
    candidateListCodec,
    candidateSortFields,
    type CandidateFilters,
    type CandidateListQuery,
} from "../lib/candidate-list-query"
import { usePaymentCandidatePageQuery } from "./queries"

export function useCandidateList(canRead: boolean) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const url = useMemo(
        () => candidateListCodec.parse(searchParams),
        [searchParams],
    )
    const update = useCallback(
        (patch: Partial<CandidateListQuery>) => {
            const params = new URLSearchParams(searchParams.toString())
            for (const key of Array.from(params.keys())) {
                if (key.startsWith("candidate_")) params.delete(key)
            }
            candidateListCodec
                .buildParams({ ...url, ...patch })
                .forEach((value, key) => params.set(key, value))
            params.set("view", "available")
            router.replace(`${pathname}?${params.toString()}`, {
                scroll: false,
            })
        },
        [url, searchParams, pathname, router],
    )
    const query = usePaymentCandidatePageQuery(url, canRead)
    const applied: CandidateFilters = {
        purchase_order_no: url.purchase_order_no ?? "",
        purchase_summary: url.purchase_summary ?? "",
        supplier_id: url.supplier_id ?? "",
        payer_party_id: url.payer_party_id ?? "",
    }
    useEffect(() => {
        if (
            canRead &&
            query.isSuccess &&
            query.data.total === 0 &&
            url.page > 1
        )
            update({ page: 1 })
    }, [canRead, query.isSuccess, query.data?.total, url.page, update])
    const sorting: SortingState = url.sort_by
        ? [{ id: url.sort_by, desc: url.sort_desc ?? false }]
        : []
    const sort = (next: SortingState) =>
        update({
            sort_by: candidateSortFields.find((key) => key === next[0]?.id),
            sort_desc: next[0]?.desc ?? false,
            page: 1,
        })
    return { url, query, applied, update, sorting, sort }
}
