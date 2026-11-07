"use client"

import { ImportFailuresDownload } from "./import-failures-download"
import { useSupplierImportFailuresDownload } from "../hooks/use-supplier-import-failures"

/** 原提交人下载已结束任务的待处理行；敏感原文不写入查询缓存。 */
export const SupplierImportFailures = ({ jobId }: { jobId: string }) => {
    const download = useSupplierImportFailuresDownload(jobId)
    return (
        <ImportFailuresDownload
            id="supplier-import-download-failures"
            pending={download.isPending}
            failed={download.isError}
            error={download.error}
            onDownload={() => download.mutate()}
        />
    )
}
