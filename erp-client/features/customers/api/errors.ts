import { getErrorMessage, type ApiError } from "@/lib/api"

/** 提取服务端稳定错误消息。 */
export function apiErrorMessage(error: ApiError): string {
    return getErrorMessage(error, "操作未完成，请稍后重试。")
}
