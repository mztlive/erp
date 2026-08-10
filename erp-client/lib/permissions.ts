/**
 * 权限判定工具。
 *
 * 与后端 `entities::Permission::covers` / Casbin matcher 对齐：
 * - 权限字符串格式 `resource:action`
 * - `*` 可匹配资源或动作任一侧
 * - 超级管理员授予 `*:*` 时覆盖全部
 */

/** 判断单条已授予权限是否覆盖目标权限。 */
function permissionCovers(granted: string, required: string): boolean {
  const [grantedResource, grantedAction] = splitPermission(granted)
  const [requiredResource, requiredAction] = splitPermission(required)
  if (!grantedResource || !grantedAction || !requiredResource || !requiredAction) {
    return false
  }
  const resourceOk =
    grantedResource === "*" || grantedResource === requiredResource
  const actionOk = grantedAction === "*" || grantedAction === requiredAction
  return resourceOk && actionOk
}

/** 判断权限集合是否覆盖目标权限（任一 granted 命中即可）。 */
export function hasPermission(
  granted: readonly string[] | undefined | null,
  required: string
): boolean {
  if (!granted?.length) return false
  return granted.some((item) => permissionCovers(item, required))
}

/**
 * 判断权限集合是否覆盖任一目标权限（OR）。
 * 用于「进入模块需要 list 或其它入口动作」的菜单可见性。
 */
export function hasAnyPermission(
  granted: readonly string[] | undefined | null,
  required: readonly string[]
): boolean {
  if (!required.length) return true
  if (!granted?.length) return false
  return required.some((item) => hasPermission(granted, item))
}

function splitPermission(value: string): [string | null, string | null] {
  const normalized = value.trim().toLowerCase()
  const idx = normalized.indexOf(":")
  if (idx <= 0 || idx === normalized.length - 1) return [null, null]
  if (normalized.indexOf(":", idx + 1) !== -1) return [null, null]
  return [normalized.slice(0, idx), normalized.slice(idx + 1)]
}
