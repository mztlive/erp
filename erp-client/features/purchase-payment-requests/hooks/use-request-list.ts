"use client"

import { useCallback, useEffect, useMemo } from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { SortingState } from "@tanstack/react-table"
import {
    requestFilterFields,
    requestListCodec,
    requestSortFields,
    type RequestFilters,
    type RequestListUrl,
} from "../lib/list-query"
import { usePaymentRequestsQuery } from "./queries"

export function useRequestList() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const url = useMemo(
        () => requestListCodec.parse(searchParams),
        [searchParams],
    )
    const update = useCallback(
        (patch: Partial<RequestListUrl>) => {
            const next = requestListCodec.buildParams({ ...url, ...patch })
            // 保留新建入口的采购来源及其他导航上下文。
            const params = new URLSearchParams(searchParams.toString())
            for (const key of [
                "view",
                "page",
                "page_size",
                "sort_by",
                "sort_desc",
                ...requestFilterFields.map(([key]) => key),
            ])
                params.delete(key)
            next.forEach((value, key) => params.set(key, value))
            const query = params.toString()
            router.replace(`${pathname}${query ? `?${query}` : ""}`, {
                scroll: false,
            })
        },
        [url, searchParams, router, pathname],
    )
    const query = usePaymentRequestsQuery(
        {
            ...url,
            view: url.view === "draft" ? "draft" : "effective",
        },
        url.view !== "available",
    )
    const applied: RequestFilters = {
        request_no: url.request_no ?? "",
        supplier_name: url.supplier_name ?? "",
        payer_party_name: url.payer_party_name ?? "",
        applicant_name: url.applicant_name ?? "",
        purchase_order_no: url.purchase_order_no ?? "",
    }
    // 空结果也要归位；非空结果的越界页由 DataTable 统一钳位。
    useEffect(() => {
        if (
            url.view !== "available" &&
            query.isSuccess &&
            query.data.total === 0 &&
            url.page > 1
        ) {
            update({ page: 1 })
        }
    }, [url.view, url.page, query.isSuccess, query.data?.total, update])
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
    return { url, update, query, applied, sorting, sort }
}
