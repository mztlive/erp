import { format } from "date-fns"
import { isApiError } from "@/lib/api/errors"
import type { JdVopAuthorizationState } from "../types/jd-vop-authorization"

export const JD_VOP_AUTHORIZATION_LABEL: Record<
    JdVopAuthorizationState,
    string
> = {
    unconfigured: "授权配置待补齐",
    missing: "未授权",
    identity_unverified: "采购账号身份待核验",
    unreadable: "授权记录待核验",
    expired: "需要重新授权",
    authorizing: "正在授权",
    unknown: "授权结果待核实",
    authorized: "已授权",
    configured: "令牌已配置",
    refreshing: "正在续期",
    reauthorization_required: "需要重新授权",
    rejected: "授权未通过",
}

export function jdVopAuthorizationTime(value: number | null) {
    if (value === null) return "尚未取得"
    const date = new Date(value * 1000)
    return Number.isFinite(date.getTime())
        ? format(date, "yyyy-MM-dd HH:mm")
        : "时间待核实"
}

/** 网络未知仅允许查询状态，不能再次提交账号资料。 */
export function jdVopAuthorizationError(error: unknown) {
    const uncertain =
        !isApiError(error) ||
        ["Network", "Parse", "Unknown"].includes(error.kind) ||
        (error.status === 422 && error.code === "JD_VOP_AUTH_RESULT_UNKNOWN") ||
        error.status === 408 ||
        (error.status ?? 0) >= 500
    if (uncertain)
        return {
            uncertain: true,
            message:
                "本次授权结果尚未查明。密码和私钥已清空，请查询授权状态，不要重复提交。",
        }
    const message =
        error.status === 422 && error.code === "JD_VOP_AUTH_IDENTITY_MISMATCH"
            ? "京东账号与原授权账号不一致，请使用原采购账号或联系管理员核验。"
            : error.status === 401
              ? "登录状态已失效，请重新登录后查看授权状态。"
              : error.status === 403
                ? "当前账号没有京东授权权限，请联系管理员。"
                : error.status === 409
                  ? "授权记录已变化，请关闭窗口并刷新状态后重新办理。"
                  : error.status === 429
                    ? "授权请求过于频繁，请稍后刷新授权状态。"
                    : "授权资料未通过校验，请核对采购账号、密码、应用回调地址及私钥后重新填写。"
    return { uncertain: false, message }
}
