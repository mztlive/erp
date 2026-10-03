/** 文件资产（D05）受控预览与下载适配层。 */

import { apiGetBlob, apiPostForm } from "@/lib/api"

/** 通过受控预览接口读取文件内容；调用方负责创建并释放 Blob URL。 */
export function fetchFileAssetPreviewBlob(assetId: string): Promise<Blob> {
    return apiGetBlob(
        `/admin/file-assets/${encodeURIComponent(assetId)}/preview`,
        { timeoutMs: 30_000, cache: "no-store" },
    )
}

/**
 * 通过受控预览接口拉取文件并触发浏览器下载。
 *
 * @param assetId 文件资产 ID
 * @param fileName 下载时使用的文件名
 */
export async function downloadFileAsset(
    assetId: string,
    fileName: string,
): Promise<void> {
    const blob = await fetchFileAssetPreviewBlob(assetId)
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = fileName
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
    URL.revokeObjectURL(url)
}

const evidenceUploads = new WeakMap<
    File,
    Promise<{ id: string; file_name: string }>
>()

/** 上传受控图片或 PDF；同一文件重试复用已登记身份。 */
export function uploadEvidenceFileAsset(
    file: File,
): Promise<{ id: string; file_name: string }> {
    if (
        !["application/pdf", "image/jpeg", "image/png", "image/webp"].includes(
            file.type,
        )
    ) {
        return Promise.reject(new Error("凭证仅支持 PDF、JPG、PNG 或 WebP"))
    }
    if (file.size === 0 || file.size > 5 * 1024 * 1024) {
        return Promise.reject(new Error("请选择不超过 5 MB 的非空凭证文件"))
    }
    const uploaded = evidenceUploads.get(file)
    if (uploaded) return uploaded
    const data = new FormData()
    data.append("file", file)
    data.append("sensitivity_class", "sensitive")
    data.append("retention_class", "long_term")
    const request = apiPostForm<{ id: string; file_name: string }>(
        "/admin/file-assets/upload",
        data,
    ).catch((error: unknown) => {
        evidenceUploads.delete(file)
        throw error
    })
    evidenceUploads.set(file, request)
    return request
}
