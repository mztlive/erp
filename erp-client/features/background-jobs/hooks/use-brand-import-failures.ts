"use client"
import { useMutation } from "@tanstack/react-query"
import { fetchBrandImportFailures } from "@/features/master-data/api/brand-import"
import { downloadBrandFailures } from "@/features/master-data/lib/brand-import"

/** 当前权限和原提交人由后端重验，源行只在下载期间读取。 */
export function useBrandImportFailuresDownload(jobId: string) {
    return useMutation({
        mutationFn: async () => {
            const data = await fetchBrandImportFailures(jobId)
            await downloadBrandFailures(data.rows, data.results)
        },
        retry: false,
        gcTime: 0,
        meta: { suppressErrorToast: true },
    })
}
