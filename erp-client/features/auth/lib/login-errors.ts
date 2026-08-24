import { getErrorMessage, type ApiError } from "@/lib/api/errors"

/** 把登录失败的未知异常转换为可展示给用户的下一步提示。 */
export function loginErrorMessage(error: unknown): string {
    const err = error as Partial<ApiError> | undefined
    if (err?.kind === "Auth") return "账号或密码不正确，请重试"
    if (err?.kind === "Validation") {
        return getErrorMessage(error, "登录信息未通过检查，请修改后重试。")
    }
    if (err?.kind === "Network") return "网络连接失败，请检查网络后重试。"
    return getErrorMessage(error, "登录失败，请稍后重试。")
}
