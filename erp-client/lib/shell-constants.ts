/**
 * 应用壳层与全局会话共用的纯常量（不包含任何业务域依赖）。
 */

/** 后台会话 JWT 在 localStorage 中保存的键名。 */
export const ADMIN_TOKEN_STORAGE_KEY = "erp.token" as const

/** 应用壳层全局浮层层级常量，避免在 layout 组件内散落硬编码 z-[...]。 */
export const SHELL_Z_INDEX = {
    /** 后台任务投递脉冲覆盖层（位于业务弹窗与抽屉之上，不拦截指针事件）。 */
    navDeliveryOverlay: 100,
} as const
