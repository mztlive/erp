"use client"

import { useCallback, useEffect, useMemo } from "react"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import type { SortingState } from "@tanstack/react-table"
import {
    requestFilterFields,
    requestListCodec,
    requestSortFields,
    type RequestFilters,
    type RequestListUrl,
} from "../lib/list-query"
import { usePaymentRequestsQuery } from "./queries"

export function useRequestList(
    canList: boolean,
    canReadCandidates: boolean,
    canFilterSupplier: boolean,
    permissionsReady: boolean,
) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const parsed = useMemo(
        () => requestListCodec.parse(searchParams),
        [searchParams],
    )
    const unavailableView = !canReadCandidates && parsed.view === "available"
    const unavailableSupplier =
        !canFilterSupplier && searchParams.has("supplier_id")
    const url = useMemo<RequestListUrl>(
        () => ({
            ...parsed,
            view: unavailableView ? "effective" : parsed.view,
            supplier_id: canFilterSupplier ? parsed.supplier_id : undefined,
            page: unavailableView || unavailableSupplier ? 1 : parsed.page,
        }),
        [canFilterSupplier, parsed, unavailableView, unavailableSupplier],
    )
    const purchaseOrderScope = useMemo(() => {
        const id = url.purchase_order_id
        if (!id) return null
        const number =
            searchParams.get("purchase_order_label")?.trim() ||
            searchParams.get("purchase_order_no")?.trim()
        return { id, number: number || undefined }
    }, [searchParams, url.purchase_order_id])
    const update = useCallback(
        (patch: Partial<RequestListUrl>) => {
            const clearsPurchaseOrder =
                "purchase_order_id" in patch && !patch.purchase_order_id
            const next = requestListCodec.buildParams({
                ...url,
                ...patch,
                ...(!canFilterSupplier && { supplier_id: undefined }),
            })
            // 保留新建入口的采购来源及其他导航上下文。
            const params = new URLSearchParams(searchParams.toString())
            for (const key of [
                "view",
                "page",
                "page_size",
                "sort_by",
                "sort_desc",
                "supplier_name",
                "payer_party_name",
                "purchase_order_id",
                ...requestFilterFields.map(([key]) => key),
            ])
                params.delete(key)
            if (clearsPurchaseOrder) params.delete("purchase_order_label")
            if (patch.view && patch.view !== parsed.view) {
                for (const key of Array.from(params.keys())) {
                    if (key.startsWith("candidate_")) params.delete(key)
                }
            }
            next.forEach((value, key) => params.set(key, value))
            const query = params.toString()
            router.replace(`${pathname}${query ? `?${query}` : ""}`, {
                scroll: false,
            })
        },
        [url, parsed.view, searchParams, router, pathname, canFilterSupplier],
    )
    useEffect(() => {
        if (permissionsReady && (unavailableView || unavailableSupplier)) {
            update({ view: url.view, page: 1 })
        }
    }, [
        permissionsReady,
        unavailableView,
        unavailableSupplier,
        update,
        url.view,
    ])
    const query = usePaymentRequestsQuery(
        {
            ...url,
            view: url.view === "draft" ? "draft" : "effective",
        },
        canList && url.view !== "available",
    )
    const applied: RequestFilters = {
        request_no: url.request_no ?? "",
        supplier_id: url.supplier_id ?? "",
        payer_party_id: url.payer_party_id ?? "",
        applicant_name: url.applicant_name ?? "",
        purchase_order_no: url.purchase_order_no ?? "",
    }
    // 空结果也要归位；非空结果的越界页由 DataTable 统一钳位。
    useEffect(() => {
        if (
            canList &&
            url.view !== "available" &&
            query.isSuccess &&
            query.data.total === 0 &&
            url.page > 1
        ) {
            update({ page: 1 })
        }
    }, [
        canList,
        url.view,
        url.page,
        query.isSuccess,
        query.data?.total,
        update,
    ])
    const sorting: SortingState = url.sort_by
        ? [
              {
                  id: url.sort_by === "request_no" ? "number" : url.sort_by,
                  desc: url.sort_desc ?? false,
              },
          ]
        : []
    const sort = (next: SortingState) => {
        const first = next[0]
        const field = first?.id === "number" ? "request_no" : first?.id
        update({
            sort_by: requestSortFields.find((value) => value === field),
            sort_desc: first?.desc ?? false,
            page: 1,
        })
    }
    return { url, update, query, applied, sorting, sort, purchaseOrderScope }
}
