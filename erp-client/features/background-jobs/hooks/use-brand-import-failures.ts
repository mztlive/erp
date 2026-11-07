"use client"

import { fetchBrandImportFailures } from "@/features/master-data/api/brand-import"
import { downloadBrandFailures } from "@/features/master-data/lib/brand-import"
import { useImportFailuresDownload } from "./use-import-failures-download"

/** 当前权限和原提交人由后端重验，源行只在下载期间读取。 */
export function useBrandImportFailuresDownload(jobId: string) {
    return useImportFailuresDownload(async () => {
        const data = await fetchBrandImportFailures(jobId)
        await downloadBrandFailures(data.rows, data.results)
    })
}
