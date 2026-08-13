/**
 * 系统管理 · 账号管理 / 角色管理 HTTP API 稳定入口。
 * 实现见 api/admin；本文件只做再导出。
 */

export {
    createAdmin,
    createRole,
    deleteAdmin,
    deleteRole,
    fetchAssignableRoles,
    fetchRoles,
    updateAdmin,
    updateAdminRole,
    updateRole,
} from "./api/admin"
