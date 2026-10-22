"use client"

import { runDataTransfer, downloadTransfer } from "@/lib/data-transfer"

import { useMutation, useQuery } from "@tanstack/react-query"
import {
    fetchSupplierQuoteSource,
    type SupplierQuoteRequest,
} from "../api/supplier-quote-export"
import {
    selectedSupplierQuotes,
    type SupplierQuoteSource,
} from "../lib/supplier-quote-export"
import { inquiryKeys } from "./queries"

export function useSupplierQuoteSource(
    request: SupplierQuoteRequest,
    userId: string,
) {
    return useQuery({
        queryKey: [
            ...inquiryKeys.all,
            "supplier-quote-export",
            userId,
            request,
        ],
        queryFn: () => fetchSupplierQuoteSource(request),
        staleTime: 0,
        refetchOnWindowFocus: false,
        retry: false,
    })
}

export function useDownloadSupplierQuotes(request: SupplierQuoteRequest) {
    return useMutation({
        mutationFn: async ({
            source,
            lineIds,
            supplierIds,
        }: {
            source: SupplierQuoteSource
            lineIds: string[]
            supplierIds: string[]
        }) => {
            const fresh = await fetchSupplierQuoteSource(request)
            if (JSON.stringify(fresh) !== JSON.stringify(source))
                throw new Error(
                    "询价或供给资料已变化，请刷新预览并重新核对后下载",
                )
            const groups = selectedSupplierQuotes(fresh, lineIds, supplierIds)
            const covered = new Set(
                groups.flatMap((group) =>
                    group.lines.map((line) => line.lineId),
                ),
            )
            if (!lineIds.length || lineIds.some((id) => !covered.has(id)))
                throw new Error(
                    "所选商品中有未匹配供应商的商品，请补齐供给关系或取消勾选后下载",
                )
            if (groups.some((group) => group.lines.some((line) => line.issue)))
                throw new Error("所选商品资料不完整，请处理商品提示后重新下载")
            if (groups.length > 200)
                throw new Error(
                    "一次最多生成 200 家供应商的报价表，请减少所选供应商",
                )
            const result = await runDataTransfer("supplier_quote_archive", {
                inquiry_id: request.inquiryId,
                task_id: request.taskId,
                expected_version: request.version,
                task_version: request.taskVersion,
                line_ids: lineIds,
                supplier_ids: supplierIds,
            })
            downloadTransfer(result)
            return groups.length
        },
        meta: { suppressErrorToast: true },
    })
}
