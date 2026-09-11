"use client"
import { useMutation } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { apiGet } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import {
    downloadSupplierFailures,
    type SupplierImportRow,
    type SupplierImportResult,
} from "@/lib/supplier-import"

/** 原提交人下载已结束任务的待处理行；敏感原文不写入查询缓存。 */
export const SupplierImportFailures = ({ jobId }: { jobId: string }) => {
    const download = useMutation({
        mutationFn: async () => {
            const data = await apiGet<{
                rows: SupplierImportRow[]
                results: SupplierImportResult[]
            }>(`/admin/supplier-profiles/import/jobs/${jobId}/failures`)
            await downloadSupplierFailures(data.rows, data.results)
        },
        retry: false,
    })
    return (
        <div className="space-y-2">
            <Button
                id="supplier-import-download-failures"
                variant="outline"
                disabled={download.isPending}
                onClick={() => download.mutate()}
            >
                {download.isPending ? "准备下载…" : "下载待处理行"}
            </Button>
            {download.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(download.error, "下载失败，请重试")}
                </p>
            )}
        </div>
    )
}
