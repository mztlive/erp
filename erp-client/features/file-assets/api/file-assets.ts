/** 文件资产（D05）受控预览与下载适配层。 */

import { apiGetBlob } from "@/lib/api"

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
