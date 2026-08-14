import type { ApiError } from "@/lib/api/errors"

/** 把登录失败的未知异常转换为可展示给用户的下一步提示。 */
export function loginErrorMessage(error: unknown): string {
    const err = error as Partial<ApiError> | undefined
    if (err?.kind === "Auth") return "账号或密码不正确，请重试"
    if (err?.kind === "Validation") return err.message || "登录信息未通过校验"
    if (err?.kind === "Network") return "无法连接服务器，请确认后端已启动"
    if (err?.message) return err.message
    return "登录失败，请稍后重试"
}
