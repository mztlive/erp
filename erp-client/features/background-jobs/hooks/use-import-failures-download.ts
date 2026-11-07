"use client"

import { useMutation } from "@tanstack/react-query"

/** 源行只在下载动作期间读取，不进入查询缓存，失败也不弹全局提示。 */
export function useImportFailuresDownload(download: () => Promise<void>) {
    return useMutation({
        mutationFn: download,
        retry: false,
        gcTime: 0,
        meta: { suppressErrorToast: true },
    })
}
