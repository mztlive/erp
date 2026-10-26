"use client"
import { LoadingButton } from "@/components/ui/loading-button"
import { getErrorMessage } from "@/lib/api/errors"
import { useSupplierImportFailuresDownload } from "../hooks/use-supplier-import-failures"

/** 原提交人下载已结束任务的待处理行；敏感原文不写入查询缓存。 */
export const SupplierImportFailures = ({ jobId }: { jobId: string }) => {
    const download = useSupplierImportFailuresDownload(jobId)
    return (
        <div className="space-y-2">
            <LoadingButton
                id="supplier-import-download-failures"
                variant="outline"
                disabled={download.isPending}
                loading={download.isPending}
                onClick={() => download.mutate()}
            >
                {download.isPending ? "准备下载…" : "下载待处理行"}
            </LoadingButton>
            {download.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(download.error, "下载失败，请重试")}
                </p>
            )}
        </div>
    )
}
