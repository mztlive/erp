"use client"

import * as React from "react"
import { useMutation } from "@tanstack/react-query"

import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
import { downloadTransfer, type TransferFile } from "@/lib/data-transfer"
import { downloadListCsv } from "@/lib/list-export"
import { excelExportText } from "@/lib/ui-text"

type ScopeCsvExport = {
    content: string
    fileName: string
    rowCount: number
}

/**
 * 客户往来与供应商往来范围列表共用的 CSV 导出。
 * 成功与失败提示、防重复点击与文件下载保持原页面行为。
 */
export function useScopeListExport(exportRows: () => Promise<ScopeCsvExport>) {
    const [exporting, setExporting] = React.useState(false)

    async function handleExport() {
        if (exporting) return
        setExporting(true)
        try {
            const result = await exportRows()
            downloadListCsv(result.content, result.fileName)
            toast.add({
                title: "导出已完成",
                description: `已按当前筛选条件导出 ${result.fileName}（共 ${result.rowCount} 条）。`,
                type: "success",
            })
        } catch (error) {
            toast.add({
                title: "导出失败",
                description: getErrorMessage(error, "请重新查询后重试"),
                type: "error",
            })
        } finally {
            setExporting(false)
        }
    }

    return { exporting, handleExport }
}

export function useScopeExcelExport<T>(
    exportFile: (query: T) => Promise<TransferFile>,
) {
    return useMutation({
        gcTime: 0,
        mutationFn: async (query: T) => {
            const file = await exportFile(query)
            downloadTransfer(file)
            return {
                fileName: file.file_name,
                rowCount:
                    typeof file.metadata?.row_count === "number"
                        ? file.metadata.row_count
                        : undefined,
            }
        },
        onSuccess: ({ fileName, rowCount }) =>
            toast.add({
                title: excelExportText.success,
                description:
                    rowCount === undefined
                        ? fileName
                        : `${fileName}（共 ${rowCount} 条）`,
                type: "success",
            }),
        onError: (error) =>
            toast.add({
                title: excelExportText.failure,
                description: getErrorMessage(error, "请重新查询后重试"),
                type: "error",
            }),
    })
}
