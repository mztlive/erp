/**
 * 认证 API：公开登录入口 POST /login；当前会话资料 GET /account/profile。
 * 登录路径与后端 public routes 对齐（无 /admin 前缀，无需 JWT）。
 * profile 需 JWT，返回角色与有效权限（resource:action），供侧栏裁剪等使用。
 */

import { apiGet, apiPost } from "@/lib/api"
import { setToken } from "@/lib/api/session"

/** 后台账号类型（与 entities::AccountKind snake_case 一致）。 */
type AccountKind = "admin"

export type LoginInput = {
  account: string
  password: string
  account_kind?: AccountKind
}

export type LoginResult = {
  token: string
}

/**
 * 当前登录账号资料（对齐 services::iam::AccountProfile）。
 * permissions 为 Casbin 隐式权限字符串，含通配如 `customer:*` / `*:*`。
 */
export type AccountProfile = {
  userid: string
  account: string
  name: string
  email?: string | null
  phone?: string | null
  avatar?: string | null
  subject: string
  role_ids: string[]
  /** `resource:action` 权限列表（可能含 `*` 通配）。 */
  permissions: string[]
  account_kind: AccountKind
  store_id?: string | null
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

/**
 * 拉取当前账号资料（含有效权限）。
 * 侧栏、顶栏与工作台身份均应基于本接口，禁止本地硬编码角色菜单。
 */
export async function fetchAccountProfile(): Promise<AccountProfile> {
  const profile = await apiGet<AccountProfile>("/account/profile")
  return {
    ...profile,
    permissions: Array.isArray(profile.permissions) ? profile.permissions : [],
    role_ids: Array.isArray(profile.role_ids) ? profile.role_ids : [],
  }
}
