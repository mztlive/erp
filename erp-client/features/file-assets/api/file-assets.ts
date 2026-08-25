/** 文件资产（D05）受控预览与下载适配层。 */

import {
    getApiBaseUrl,
    getToken,
    notifyUnauthorized,
    type ApiError,
} from "@/lib/api"

/** 通过受控预览接口读取文件内容；调用方负责创建并释放 Blob URL。 */
export async function fetchFileAssetPreviewBlob(
    assetId: string,
): Promise<Blob> {
    const headers: Record<string, string> = {}
    const token = getToken()
    if (token) headers.Authorization = `Bearer ${token}`
    let response: Response
    try {
        response = await fetch(
            `${getApiBaseUrl()}/admin/file-assets/${encodeURIComponent(assetId)}/preview`,
            {
                headers,
                signal: AbortSignal.timeout(30_000),
                cache: "no-store",
            },
        )
    } catch (cause) {
        const error: ApiError = {
            kind: "Network",
            message: "文件预览网络请求失败或连接超时",
            cause,
        }
        throw error
    }
    if (response.status === 401) {
        notifyUnauthorized()
        const error: ApiError = {
            kind: "Auth",
            message: "登录状态已失效，请重新登录",
            status: 401,
        }
        throw error
    }
    if (!response.ok) {
        const payload = await response.json().catch(() => null)
        const error: ApiError = {
            kind: response.status === 403 ? "Auth" : "Http",
            message:
                (payload as { errorMessage?: string; message?: string } | null)
                    ?.errorMessage ||
                (payload as { message?: string } | null)?.message ||
                `文件预览失败（HTTP ${response.status}）`,
            status: response.status,
            responseData: payload,
        }
        throw error
    }
    return response.blob()
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
