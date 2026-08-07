/**
 * 文件资产（D05）共享上传适配层。
 * multipart 上传 `/admin/file-assets/upload`：lib/api 仅 JSON，故用原生 fetch + 鉴权头。
 */

import { getApiBaseUrl, getToken, notifyUnauthorized, type ApiError } from "@/lib/api"

type BackendFileAsset = {
  id: string
  storage_object_key: string
  public_url?: string | null
  file_name: string
  content_type: string
  byte_size: number
  security_scan_status: string
  created_by: string
  created_at: number
  version?: number
}

/** 已上传文件资产的浏览器可访问 URL（优先存储层公开地址，兜底本地挂载点）。 */
function assetUrl(storageObjectKey: string, publicUrl?: string | null): string {
  if (publicUrl?.trim()) return publicUrl
  return `${getApiBaseUrl()}/uploads/${storageObjectKey}`
}

/**
 * multipart 上传图片并登记文件资产。
 *
 * @param file 待上传文件
 * @param usage 附件用途（`image`/`attachment`/`manifest`），默认 `image`
 * @returns 上传成功的 `{ fileAssetId, url }`（url 为浏览器可访问地址）
 * @throws {ApiError} 网络失败 / 鉴权失效 / 业务失败统一抛 ApiError
 */
export async function uploadFileAssetImage(
  file: File,
  usage: "image" | "attachment" | "manifest" = "image",
): Promise<{ fileAssetId: string; url: string }> {
  const form = new FormData()
  form.append("file", file, file.name)
  form.append("sensitivity_class", "general")
  form.append("retention_class", "long_term")
  form.append("usage", usage)

  const headers: Record<string, string> = {}
  const token = getToken()
  if (token) headers.Authorization = `Bearer ${token}`

  const timeoutMs = 60_000
  let res: Response
  try {
    res = await fetch(`${getApiBaseUrl()}/admin/file-assets/upload`, {
      method: "POST",
      headers,
      body: form,
      signal: AbortSignal.timeout(timeoutMs),
    })
  } catch (cause) {
    const err: ApiError = {
      kind: "Network",
      message: "文件上传网络请求失败或连接超时",
      cause,
    }
    throw err
  }

  const text = await res.text()
  let parsed: unknown
  try {
    parsed = text ? JSON.parse(text) : null
  } catch (cause) {
    const err: ApiError = {
      kind: "Parse",
      message: "文件上传响应解析失败",
      cause,
      responseData: text,
    }
    throw err
  }

  const envelope = parsed as {
    success?: boolean
    status?: number
    errorMessage?: string
    data?: BackendFileAsset | null
  } | null

  if (res.status === 401 || envelope?.status === 401) {
    notifyUnauthorized()
    const err: ApiError = {
      kind: "Auth",
      message: "登录状态已失效，请重新登录",
      status: 401,
      responseData: parsed,
    }
    throw err
  }

  if (!res.ok) {
    const err: ApiError = {
      kind: res.status === 400 ? "Validation" : "Http",
      message:
        envelope?.errorMessage ||
        (res.status === 400
          ? "文件未通过上传校验"
          : `文件上传失败（HTTP ${res.status}）`),
      status: res.status,
      responseData: parsed,
    }
    throw err
  }

  if (envelope && envelope.success === false) {
    const err: ApiError = {
      kind: "Validation",
      message: envelope.errorMessage || "文件未通过上传校验",
      status: envelope.status,
      responseData: envelope,
    }
    throw err
  }

  const data = envelope?.data
  if (!data?.id || !data.storage_object_key) {
    const err: ApiError = {
      kind: "Parse",
      message: "上传响应缺少文件资产 ID 或对象键",
      responseData: parsed,
    }
    throw err
  }
  return {
    fileAssetId: data.id,
    url: assetUrl(data.storage_object_key, data.public_url),
  }
}
