/** 基础资料 multipart 原子命令的临时文件引用与请求组装。 */

import { apiPostForm, apiPutForm } from "@/lib/api"
import type { PendingAssetUpload } from "@/features/master-data/types"

const PENDING_FILE_REFERENCE_PREFIX = "pending-file:"

/** 生成一次请求内稳定且不会与正式文件资产 ID 混淆的临时引用。 */
export function pendingFileReference(...segments: Array<string | number>) {
    return `${PENDING_FILE_REFERENCE_PREFIX}${segments
        .map((segment) => encodeURIComponent(String(segment)))
        .join(":")}`
}

function assetCommandForm(
    command: unknown,
    uploads: readonly PendingAssetUpload[],
): FormData {
    const form = new FormData()
    form.append("command", JSON.stringify(command))
    for (const upload of uploads) {
        if (!upload.reference.startsWith(PENDING_FILE_REFERENCE_PREFIX)) {
            throw new Error("临时文件引用格式无效")
        }
        form.append(upload.reference, upload.file, upload.file.name)
    }
    return form
}

/** 用一个 POST multipart 请求提交业务命令及全部新增文件。 */
export function postAssetCommand<T>(
    path: string,
    command: unknown,
    uploads: readonly PendingAssetUpload[],
): Promise<T> {
    return apiPostForm<T>(path, assetCommandForm(command, uploads), {
        timeoutMs: 60_000,
    })
}

/** 用一个 PUT multipart 请求提交业务命令及全部新增文件。 */
export function putAssetCommand<T>(
    path: string,
    command: unknown,
    uploads: readonly PendingAssetUpload[],
): Promise<T> {
    return apiPutForm<T>(path, assetCommandForm(command, uploads), {
        timeoutMs: 60_000,
    })
}
