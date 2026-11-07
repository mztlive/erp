"use client"

import { ImportFailuresDownload } from "./import-failures-download"
import { useBrandImportFailuresDownload } from "../hooks/use-brand-import-failures"

/** 原提交人下载已结束任务的待处理行；敏感原文不写入查询缓存。 */
export const BrandImportFailures = ({ jobId }: { jobId: string }) => {
    const download = useBrandImportFailuresDownload(jobId)
    return (
        <ImportFailuresDownload
            id="brand-import-download-failures"
            pending={download.isPending}
            failed={download.isError}
            error={download.error}
            onDownload={() => download.mutate()}
        />
    )
}
