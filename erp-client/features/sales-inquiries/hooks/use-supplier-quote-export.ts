"use client"

import { useMutation, useQuery } from "@tanstack/react-query"
import {
    fetchSupplierQuoteInquiry,
    fetchSupplierQuoteSource,
    type SupplierQuoteRequest,
} from "../api/supplier-quote-export"
import {
    quoteFilePart,
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
            const [{ loadSupplierQuoteImages }, { buildSupplierQuoteArchive }] =
                await Promise.all([
                    import("../lib/supplier-quote-images"),
                    import("../lib/supplier-quote-workbook"),
                ])
            const images = await loadSupplierQuoteImages(groups)
            const archive = await buildSupplierQuoteArchive(
                fresh,
                groups,
                images,
            )
            await fetchSupplierQuoteInquiry(request)
            const url = URL.createObjectURL(archive)
            const anchor = document.createElement("a")
            anchor.href = url
            anchor.download = `${quoteFilePart(fresh.inquiryNo)}-供应商报价表.zip`
            document.body.appendChild(anchor)
            try {
                anchor.click()
            } finally {
                anchor.remove()
                window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
            }
            return groups.length
        },
        meta: { suppressErrorToast: true },
    })
}
