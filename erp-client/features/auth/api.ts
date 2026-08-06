/**
 * 认证 API：公开登录入口 POST /login。
 * 路径与后端 public routes 对齐（无 /admin 前缀，无需 JWT）。
 */

import { apiPost } from "@/lib/api"
import { setToken } from "@/lib/api/session"

/** 后台账号类型（与 entities::AccountKind snake_case 一致）。 */
export type AccountKind = "admin"

export type LoginInput = {
  account: string
  password: string
  account_kind?: AccountKind
}

export type LoginResult = {
  token: string
}

/**
 * 账号密码登录，成功后写入 localStorage token。
 *
 * @param input 账号、密码；account_kind 默认 admin。
 * @returns 服务端签发的 JWT。
 */
export async function login(input: LoginInput): Promise<LoginResult> {
  const result = await apiPost<LoginResult>("/login", {
    account: input.account.trim(),
    password: input.password,
    account_kind: input.account_kind ?? "admin",
  })
  if (!result?.token) {
    throw {
      kind: "Parse",
      message: "登录响应缺少 token",
      responseData: result,
    }
  }
  setToken(result.token)
  return result
}
