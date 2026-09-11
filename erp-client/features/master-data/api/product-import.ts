import { apiDelete, apiGet, apiPost, apiPostForm } from "@/lib/api"
import { toQueryString } from "@/lib/api/paging"

export type ProductImportJob = {
    id: string
    job_no: string
    status: string
    file_name: string | null
    total_count: number
    processed_count: number
    success_count: number
    skipped_count: number
    failed_count: number
    started_at: number | null
    finished_at: number | null
    error_summary: string | null
    version: number
    created_at: number
}

export async function submitProductImport(input: {
    file: File
    requestId: string
}): Promise<ProductImportJob> {
    const body = new FormData()
    body.set("file", input.file)
    body.set("request_id", input.requestId)
    return apiPostForm<ProductImportJob>("/admin/products/import", body, {
        timeoutMs: 15 * 60 * 1000,
    })
}

/** 浏览器直传初始化视图（与后端 DTO 字段一一对应）。 */
export type ProductImportDirectUploadInit = {
    upload_id: string
    object_key: string
    part_size: number
    total_parts: number
    part_url_ttl_secs: number
}

/** 已直传分片（序号与对象存储返回的 ETag）。 */
export type ProductImportDirectUploadedPart = {
    part_number: number
    etag: string
}

/** 初始化浏览器直传：只发小 JSON，不经过网关传文件。 */
export async function createProductImportDirectUpload(input: {
    fileName: string
    byteSize: number
    requestId: string
}): Promise<ProductImportDirectUploadInit> {
    return apiPost<ProductImportDirectUploadInit>(
        "/admin/products/import-uploads",
        {
            file_name: input.fileName,
            byte_size: input.byteSize,
            request_id: input.requestId,
        },
    )
}

/** 按需获取单个分片的预签名地址（慢速网络避免地址过期）。 */
export async function fetchProductImportDirectPartUrl(input: {
    uploadId: string
    partNumber: number
    objectKey: string
    requestId: string
}): Promise<string> {
    const view = await apiGet<{ url: string; expires_in_secs: number }>(
        `/admin/products/import-uploads/${encodeURIComponent(input.uploadId)}/parts/${input.partNumber}`,
        {
            object_key: input.objectKey,
            request_id: input.requestId,
        },
    )
    return view.url
}

/** 合并已直传分片并登记导入任务（小 JSON，解析仍在服务端同步完成）。 */
export async function completeProductImportDirectUpload(input: {
    uploadId: string
    objectKey: string
    fileName: string
    byteSize: number
    requestId: string
    parts: ProductImportDirectUploadedPart[]
}): Promise<ProductImportJob> {
    return apiPost<ProductImportJob>(
        `/admin/products/import-uploads/${encodeURIComponent(input.uploadId)}/complete`,
        {
            object_key: input.objectKey,
            file_name: input.fileName,
            byte_size: input.byteSize,
            request_id: input.requestId,
            parts: input.parts,
        },
        { timeoutMs: 15 * 60 * 1000 },
    )
}

/** 取消分片上传并清理对象存储侧已上传的分片；失败静默忽略。 */
export async function abortProductImportDirectUpload(input: {
    uploadId: string
    objectKey: string
    requestId: string
}): Promise<void> {
    const query = toQueryString({
        object_key: input.objectKey,
        request_id: input.requestId,
    })
    await apiDelete<void>(
        `/admin/products/import-uploads/${encodeURIComponent(input.uploadId)}?${query}`,
    ).catch(() => undefined)
}

export type DirectPartProgress = {
    loaded: number
    total: number
}

/** 用户取消上传的标记错误（调用方可据此静默收尾）。 */
export const isDirectUploadCancelled = (error: unknown): boolean =>
    error instanceof DOMException && error.name === "AbortError"

/**
 * 把单个分片 PUT 到预签名地址。
 *
 * 必须用裸请求直调对象存储：不得附加应用鉴权头，也不受统一信封解析约束；
 * 用 XMLHttpRequest 以支持上传进度与取消。
 *
 * @param url 预签名 PUT 地址。
 * @param blob 分片内容。
 * @param onProgress 分片级进度回调。
 * @param signal 取消信号。
 * @returns 对象存储返回的 ETag（合并分片的凭证）。
 * @throws {Error} 网络失败、非 2xx 或缺 ETag（多为存储跨域未暴露该头）时抛出。
 */
export const putProductImportDirectPart = (
    url: string,
    blob: Blob,
    onProgress?: (progress: DirectPartProgress) => void,
    signal?: AbortSignal,
): Promise<string> =>
    new Promise((resolve, reject) => {
        if (signal?.aborted) {
            reject(new DOMException("已取消上传", "AbortError"))
            return
        }
        const xhr = new XMLHttpRequest()
        xhr.open("PUT", url)
        const onAbort = () => xhr.abort()
        signal?.addEventListener("abort", onAbort, { once: true })
        const cleanup = () => signal?.removeEventListener("abort", onAbort)
        xhr.upload.onprogress = (event) => {
            if (event.lengthComputable) {
                onProgress?.({ loaded: event.loaded, total: event.total })
            }
        }
        xhr.onload = () => {
            cleanup()
            if (xhr.status < 200 || xhr.status >= 300) {
                reject(new Error(`分片上传失败（${xhr.status}），请重试。`))
                return
            }
            const etag =
                xhr.getResponseHeader("ETag") ?? xhr.getResponseHeader("etag")
            if (!etag) {
                reject(
                    new Error(
                        "对象存储未返回分片标识，请检查存储跨域配置后重试，或改用普通上传。",
                    ),
                )
                return
            }
            resolve(etag)
        }
        xhr.onerror = () => {
            cleanup()
            reject(new Error("分片上传网络失败，请检查网络后重试。"))
        }
        xhr.onabort = () => {
            cleanup()
            reject(new DOMException("已取消上传", "AbortError"))
        }
        xhr.send(blob)
    })
