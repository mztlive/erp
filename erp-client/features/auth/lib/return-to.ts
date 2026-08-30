/** 登录成功后默认落地页。 */
export const DEFAULT_RETURN_TO = "/workspace"

/**
 * 解析登录后跳转目标：仅接受站内绝对路径，且拒绝 `//` 开头的协议相对地址。
 *
 * @param raw returnTo 查询参数原始值。
 */
export function resolveReturnTarget(raw: string | null): string {
    return raw && raw.startsWith("/") && !raw.startsWith("//")
        ? raw
        : DEFAULT_RETURN_TO
}
