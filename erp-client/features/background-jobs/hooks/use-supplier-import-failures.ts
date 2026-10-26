"use client"

import { useMutation } from "@tanstack/react-query"
import { downloadSupplierFailures } from "@/lib/supplier-import"
import { fetchSupplierImportFailures } from "../api/supplier-import"

/** 源行只在下载动作期间读取，不保留在查询缓存中。 */
export function useSupplierImportFailuresDownload(jobId: string) {
    return useMutation({
        mutationFn: async () => {
            const data = await fetchSupplierImportFailures(jobId)
            await downloadSupplierFailures(data.rows, data.results)
        },
        retry: false,
        gcTime: 0,
        meta: { suppressErrorToast: true },
    })
}
