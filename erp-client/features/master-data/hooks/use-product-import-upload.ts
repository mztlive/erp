"use client"

import { useCallback, useRef, useState } from "react"

import {
    abortProductImportDirectUpload,
    completeProductImportDirectUpload,
    createProductImportDirectUpload,
    fetchProductImportDirectPartUrl,
    isDirectUploadCancelled,
    putProductImportDirectPart,
    type ProductImportDirectUploadedPart,
    type ProductImportJob,
} from "@/features/master-data/api/product-import"
import { getErrorMessage } from "@/lib/api/errors"

/** 与后端导入上限对齐的前端硬拦截（700 MiB）。 */
export const MAX_PRODUCT_IMPORT_FILE_BYTES = 700 * 1024 * 1024

export type ProductImportUploadPhase = "uploading" | "committing"

export interface ProductImportUploadProgress {
    phase: ProductImportUploadPhase
    loadedBytes: number
    totalBytes: number
    partIndex: number
    totalParts: number
    percent: number
}

/** 总分片数至少为 1（小文件单片直传）。 */
export const calcTotalParts = (byteSize: number, partSize: number): number =>
    Math.max(1, Math.ceil(byteSize / partSize))

/** 上传百分比（0-100 取整）。 */
export const calcUploadPercent = (
    loadedBytes: number,
    totalBytes: number,
): number => {
    if (totalBytes <= 0) return 0
    return Math.min(100, Math.round((loadedBytes / totalBytes) * 100))
}

/** MB 文案（保留 1 位小数）。 */
export const formatUploadMegabytes = (bytes: number): string =>
    `${(bytes / (1024 * 1024)).toFixed(1)} MB`

/**
 * 浏览器直传编排：初始化 → 逐片直传对象存储 → 合并登记任务。
 *
 * 分片地址按需获取（慢速网络避免预签名过期）；失败或取消后尽力
 * 取消服务端分片上传，不残留对象存储分片。
 */
export function useProductImportDirectUpload() {
    const [progress, setProgress] =
        useState<ProductImportUploadProgress | null>(null)
    const [error, setError] = useState("")
    const [active, setActive] = useState(false)
    const abortRef = useRef<AbortController | null>(null)

    const cancel = useCallback(() => {
        abortRef.current?.abort()
    }, [])

    const reset = useCallback(() => {
        setError("")
        setProgress(null)
    }, [])

    const start = useCallback(
        async (file: File, requestId: string): Promise<ProductImportJob> => {
            setError("")
            if (file.size <= 0) {
                const empty = new Error("文件为空，请重新选择产品报价表。")
                setError(getErrorMessage(empty))
                throw empty
            }
            if (file.size > MAX_PRODUCT_IMPORT_FILE_BYTES) {
                const oversized = new Error(
                    "导入文件不能超过 700 MB，请压缩图片或拆分后重试。",
                )
                setError(getErrorMessage(oversized))
                throw oversized
            }
            const controller = new AbortController()
            abortRef.current = controller
            setActive(true)
            setProgress({
                phase: "uploading",
                loadedBytes: 0,
                totalBytes: file.size,
                partIndex: 0,
                totalParts: 1,
                percent: 0,
            })
            let uploadId = ""
            let objectKey = ""
            try {
                const init = await createProductImportDirectUpload({
                    fileName: file.name,
                    byteSize: file.size,
                    requestId,
                })
                uploadId = init.upload_id
                objectKey = init.object_key
                const parts: ProductImportDirectUploadedPart[] = []
                let loadedBase = 0
                for (let index = 0; index < init.total_parts; index += 1) {
                    const blob = file.slice(
                        index * init.part_size,
                        index * init.part_size + init.part_size,
                    )
                    const url = await fetchProductImportDirectPartUrl({
                        uploadId,
                        partNumber: index + 1,
                        objectKey,
                        requestId,
                    })
                    const etag = await putProductImportDirectPart(
                        url,
                        blob,
                        ({ loaded }) => {
                            const loadedBytes = loadedBase + loaded
                            setProgress({
                                phase: "uploading",
                                loadedBytes,
                                totalBytes: file.size,
                                partIndex: index,
                                totalParts: init.total_parts,
                                percent: calcUploadPercent(
                                    loadedBytes,
                                    file.size,
                                ),
                            })
                        },
                        controller.signal,
                    )
                    parts.push({ part_number: index + 1, etag })
                    loadedBase += blob.size
                    setProgress({
                        phase: "uploading",
                        loadedBytes: loadedBase,
                        totalBytes: file.size,
                        partIndex: Math.min(index + 1, init.total_parts - 1),
                        totalParts: init.total_parts,
                        percent: calcUploadPercent(loadedBase, file.size),
                    })
                }
                setProgress({
                    phase: "committing",
                    loadedBytes: file.size,
                    totalBytes: file.size,
                    partIndex: init.total_parts - 1,
                    totalParts: init.total_parts,
                    percent: 100,
                })
                const job = await completeProductImportDirectUpload({
                    uploadId,
                    objectKey,
                    fileName: file.name,
                    byteSize: file.size,
                    requestId,
                    parts,
                })
                setProgress(null)
                setActive(false)
                return job
            } catch (err) {
                if (isDirectUploadCancelled(err)) {
                    if (uploadId && objectKey) {
                        await abortProductImportDirectUpload({
                            uploadId,
                            objectKey,
                            requestId,
                        })
                    }
                    setProgress(null)
                    setActive(false)
                    throw err
                }
                if (uploadId && objectKey) {
                    await abortProductImportDirectUpload({
                        uploadId,
                        objectKey,
                        requestId,
                    })
                }
                setError(
                    getErrorMessage(err, "直传失败，请重试或改用普通上传。"),
                )
                setActive(false)
                throw err
            }
        },
        [],
    )

    return { progress, error, active, start, cancel, reset }
}
