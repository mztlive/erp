/**
 * 公开选品接口：走 /public/selection/{token}，不带员工 JWT 也可访问。
 * 图片经令牌授权的 Blob 预览接口获取，不直接下发永久对象地址。
 */

import { getApiBaseUrl } from "@/lib/api"
import { apiPost } from "@/lib/api/client"
import type {
    PublicSelection,
    SaveSessionInput,
} from "@/features/sales-selection/types"

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
 * 拼出公开图片接口地址（img 直接渲染用，仍带令牌授权）。
 * @param token 公开令牌
 * @param imageRef 图片引用
 */
export const publicImageUrl = (token: string, imageRef: string): string =>
    `${getApiBaseUrl()}/public/selection/${encodeURIComponent(token)}/images?ref=${encodeURIComponent(imageRef)}`
