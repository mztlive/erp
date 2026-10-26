import {
    decodeTransferJson,
    readTransferContext,
    readTransferResult,
} from "@/lib/data-transfer"

/** 原任务结果读取沿用文件任务的归属、权限、版本及有效期检查。 */
export async function fetchTransferPreview<T>(jobId: string): Promise<T> {
    const current = await readTransferContext(jobId)
    if (!["ready", "invalid"].includes(current.resume_state))
        throw new Error(
            current.resume_reason ||
                "预览已确认或已失效，请刷新任务查看当前状态。",
        )
    const file = await readTransferResult(jobId)
    if (file.content_type !== "application/json")
        throw new Error("任务结果不是可继续核对的导入预览，请重新导入。")
    return decodeTransferJson<T>(file)
}

/** 确认前读取当前业务状态，避免使用已经确认或失效的预览。 */
export async function verifyTransferPreview(jobId: string) {
    const current = await readTransferContext(jobId)
    if (current.resume_state !== "ready")
        throw new Error(
            current.resume_reason || "预览状态已变化，请刷新任务后核对。",
        )
    return current
}
