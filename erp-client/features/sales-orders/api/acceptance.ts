/**
 * W06 客户验收 API（queryFn / mutationFn）。
 * 兼容 re-export 旧 detail/acceptance 入口；新 UI 使用 workspace 系列。
 * 实现已拆分到 lib/acceptance-workspace-fetch.ts（读取）与
 * lib/acceptance-mutations.ts（登记/冲正/草稿），本模块保持原导出名。
 */

export type { FetchAcceptanceWorkspaceParams } from "@/features/sales-orders/lib/acceptance-workspace-fetch"
export { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/lib/acceptance-workspace-fetch"
export {
    postCustomerAcceptanceWorkspace,
    reverseCustomerAcceptanceWorkspace,
    saveCustomerAcceptanceDraft,
} from "@/features/sales-orders/lib/acceptance-mutations"
