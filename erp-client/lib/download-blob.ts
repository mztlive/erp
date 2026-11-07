/**
 * 触发浏览器下载。
 *
 * `linkId` 供自动化定位下载入口；`revokeDelayMs` 保留需要延迟释放对象 URL
 * 的调用方，默认在点击后立即释放。
 */
export function downloadBlob(
    blob: Blob,
    fileName: string,
    options?: { linkId?: string; revokeDelayMs?: number },
) {
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    if (options?.linkId) link.id = options.linkId
    link.href = url
    link.download = fileName
    link.click()
    if (options?.revokeDelayMs) {
        setTimeout(() => URL.revokeObjectURL(url), options.revokeDelayMs)
    } else {
        URL.revokeObjectURL(url)
    }
}
