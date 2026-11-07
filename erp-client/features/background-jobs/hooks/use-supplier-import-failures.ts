"use client"

import { downloadSupplierFailures } from "@/lib/supplier-import"
import { fetchSupplierImportFailures } from "../api/supplier-import"
import { useImportFailuresDownload } from "./use-import-failures-download"

/** 源行只在下载动作期间读取，不保留在查询缓存中。 */
export function useSupplierImportFailuresDownload(jobId: string) {
    return useImportFailuresDownload(async () => {
        const data = await fetchSupplierImportFailures(jobId)
        await downloadSupplierFailures(data.rows, data.results)
    })
}
