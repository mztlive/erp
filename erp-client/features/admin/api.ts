/**
 * 系统管理 · 账号管理 / 角色管理 真实 HTTP API。
 * 后端：services/src/iam（admin + rbac），路由挂在 /admin 下（core/routes/admin.rs）。
 * 所有调用点必须位于 TanStack Query 的 queryFn / mutationFn。
 */

import { apiDelete, apiGet, apiPost, apiPut } from "@/lib/api"
import type {
  AdminRole,
  CreateAdminPayload,
  CreateRolePayload,
  UpdateAdminPayload,
  UpdateAdminRolePayload,
  UpdateRolePayload,
} from "@/features/admin/types"

/** 角色列表：含权限策略，后端一次返回全部角色。 */
export const fetchRoles = (): Promise<AdminRole[]> =>
  apiGet<AdminRole[]>("/admin/roles")

/**
 * 可分配角色列表（仅当前操作者可分配的角色）。
 * 后端无 assignable_list 权限时回落到全部角色，保证账号表单仍可用。
 */
export const fetchAssignableRoles = async (): Promise<AdminRole[]> => {
  try {
    return await apiGet<AdminRole[]>("/admin/roles/assignable")
  } catch {
    return fetchRoles()
  }
}

/** 创建管理员账号。 */
export const createAdmin = (payload: CreateAdminPayload): Promise<void> =>
  apiPost<void>("/admin/admins", payload)

/** 更新管理员姓名/密码/角色；密码为空串时不提交密码字段。 */
export const updateAdmin = (
  id: string,
  payload: UpdateAdminPayload
): Promise<void> => apiPut<void>(`/admin/admins/${id}`, payload)

/** 仅更新管理员角色绑定。 */
export const updateAdminRole = (
  id: string,
  payload: UpdateAdminRolePayload
): Promise<void> => apiPut<void>(`/admin/admins/${id}/role`, payload)

/** 删除管理员账号（系统内置账号会被后端拒绝）。 */
export const deleteAdmin = (id: string): Promise<void> =>
  apiDelete<void>(`/admin/admins/${id}`)

/** 创建角色并写入 Casbin 权限策略。 */
export const createRole = (payload: CreateRolePayload): Promise<void> =>
  apiPost<void>("/admin/roles", payload)

/** 更新角色名称与权限策略。 */
export const updateRole = (
  id: string,
  payload: UpdateRolePayload
): Promise<void> => apiPut<void>(`/admin/roles/${id}`, payload)

/** 删除非系统角色及其策略/绑定。 */
export const deleteRole = (id: string): Promise<void> =>
  apiDelete<void>(`/admin/roles/${id}`)
