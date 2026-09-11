/**
 * 公开选品接口：走 /public/selection/{token}，不带员工 JWT 也可访问。
 * 图片经令牌授权的 Blob 预览接口获取，不直接下发永久对象地址。
 */

import { apiGetBlob, getApiBaseUrl } from "@/lib/api"
import { apiGet, apiPost } from "@/lib/api/client"
import type {
    PublicSelection,
    SaveSessionInput,
    SubmitSelectionInput,
} from "@/features/sales-selection/types"

/**
 * 读取公开选品会话（含陈列、已保存选择与后端合计）。
 * @param token 公开令牌
 */
export const fetchPublicSelection = async (
    token: string,
): Promise<PublicSelection> =>
    apiGet<PublicSelection>(`/public/selection/${encodeURIComponent(token)}`)

/**
 * 保存公开会话：携带预期会话版本、幂等键与完整选择。
 * @param input 令牌/会话版本/幂等键/完整选择
 */
export const savePublicSession = async (
    input: SaveSessionInput,
): Promise<PublicSelection> =>
    apiPost<PublicSelection>(
        `/public/selection/${encodeURIComponent(input.token)}/session`,
        {
            expected_session_version: input.expected_session_version,
            idempotency_key: input.idempotency_key,
            choices: input.choices.map((item) => ({ ...item })),
        },
    )

/**
 * 提交选品：后端以已保存版本为事实来源原子创建方案，返回最新公开页。
 * @param input 令牌/已确认会话版本/幂等键
 */
export const submitPublicSelection = async (
    input: SubmitSelectionInput,
): Promise<PublicSelection> =>
    apiPost<PublicSelection>(
        `/public/selection/${encodeURIComponent(input.token)}/submit`,
        {
            expected_session_version: input.expected_session_version,
            idempotency_key: input.idempotency_key,
        },
    )

/**
 * 读取公开回执页（已提交且链接仍有效时，后端返回同一公开页形状）。
 * @param token 公开令牌
 */
export const fetchPublicReceipt = async (
    token: string,
): Promise<PublicSelection> =>
    apiGet<PublicSelection>(
        `/public/selection/${encodeURIComponent(token)}/receipt`,
    )

/**
 * 经令牌授权预览陈列图片，返回可展示的 Blob URL。
 * 调用方负责 revokeObjectURL。
 * @param token 公开令牌
 * @param imageRef 后端下发的图片引用（非文件直链）
 */
export const fetchPublicImageBlobUrl = async (
    token: string,
    imageRef: string,
): Promise<string> => {
    const blob = await apiGetBlob(
        `/public/selection/${encodeURIComponent(token)}/images?ref=${encodeURIComponent(imageRef)}`,
    )
    return URL.createObjectURL(blob)
}

/**
 * 拼出公开图片接口地址（img 直接渲染用，仍带令牌授权）。
 * @param token 公开令牌
 * @param imageRef 图片引用
 */
export const publicImageUrl = (token: string, imageRef: string): string =>
    `${getApiBaseUrl()}/public/selection/${encodeURIComponent(token)}/images?ref=${encodeURIComponent(imageRef)}`

/**
 * P0 换品/自组入口：接口拒绝并提示 P1。
 * 保留函数签名供 P1 复用，当前直接抛业务提示。
 */
export const requestPackageCustomize = async (): Promise<never> => {
    throw new Error("换品与自组为 P1 能力，当前仅支持整套选择")
}
