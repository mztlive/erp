"use client"

import { useMutation } from "@tanstack/react-query"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
import { downloadBlob } from "@/lib/download-blob"
import { fetchPaymentExport } from "../api/payment-export"
import type { WorkspaceUrlState } from "../lib/url-state"

export function usePaymentExport() {
    return useMutation({
        gcTime: 0,
        mutationFn: async (state: WorkspaceUrlState) => {
            const rows = await fetchPaymentExport(state)
            const { buildPaymentWorkbook } =
                await import("../lib/payment-export")
            const blob = await buildPaymentWorkbook(rows)
            const fileName = `待执行采购付款-${new Intl.DateTimeFormat("sv-SE", { timeZone: "Asia/Shanghai" }).format(new Date())}.xlsx`
            downloadBlob(blob, fileName, { revokeDelayMs: 30_000 })
            return rows.length
        },
        onSuccess: (count) =>
            toast.add({
                title: `已导出 ${count} 笔待付款记录`,
                type: "success",
            }),
        onError: (error) =>
            toast.add({
                title: "导出失败",
                description: getErrorMessage(error, "请刷新列表后重试"),
                type: "error",
            }),
    })
}
