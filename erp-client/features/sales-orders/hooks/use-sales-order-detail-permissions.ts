"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    gateCommand,
    gateNavigate,
    type PermissionGate,
    type SalesOrderDetailPermissionKind,
} from "@/features/sales-orders/lib/sales-order-detail-permissions"
import { getErrorMessage } from "@/lib/api/errors"

/**
 * 销售单详情按钮权限：profile.permissions + 服务端 allowed。
 * pending/error 时入口一律不可点，避免权限未就绪时误操作。
 */
export function useSalesOrderDetailPermissions() {
    const accountQuery = useAccountProfileQuery()
    const granted = accountQuery.data?.permissions
    const profileReady =
        !accountQuery.isPending && !accountQuery.isError && Boolean(granted)

    const profileGate = (fallbackKind: SalesOrderDetailPermissionKind): PermissionGate => {
        if (accountQuery.isPending) {
            return { enabled: false, reason: "正在核对权限，请稍候。" }
        }
        if (accountQuery.isError) {
            return {
                enabled: false,
                reason: getErrorMessage(
                    accountQuery.error,
                    "暂时无法核对权限，请刷新后重试。",
                ),
            }
        }
        if (!profileReady) {
            return {
                enabled: false,
                reason: permissionKindFallback(fallbackKind),
            }
        }
        return { enabled: true }
    }

    const navigate = (
        kind: Parameters<typeof gateNavigate>[1],
    ): PermissionGate => {
        const profile = profileGate(kind)
        if (!profile.enabled) return profile
        return gateNavigate(granted, kind)
    }

    const command = (
        serverAllows: boolean,
        kind: Parameters<typeof gateCommand>[2],
        serverReason?: string,
    ): PermissionGate => {
        const profile = profileGate(kind)
        if (!profile.enabled) return profile
        return gateCommand(serverAllows, granted, kind, serverReason)
    }

    return {
        accountQuery,
        granted,
        profileReady,
        navigate,
        command,
        openPurchase: navigate("openPurchase"),
        createPurchase: (serverAllows: boolean, serverReason?: string) =>
            command(serverAllows, "createPurchase", serverReason),
        openFulfillment: navigate("openFulfillment"),
        openReceivable: navigate("openReceivable"),
        registerAcceptance: (serverAllows: boolean, serverReason?: string) =>
            command(serverAllows, "registerAcceptance", serverReason),
        cancelApproval: (serverAllows: boolean, serverReason?: string) =>
            command(serverAllows, "cancelApproval", serverReason),
        startChange: (serverAllows: boolean, serverReason?: string) =>
            command(serverAllows, "startChange", serverReason),
        voidOrder: (serverAllows: boolean, serverReason?: string) =>
            command(serverAllows, "voidOrder", serverReason),
        editAfterRejection: (serverAllows: boolean, serverReason?: string) =>
            command(serverAllows, "editAfterRejection", serverReason),
    }
}

function permissionKindFallback(kind: SalesOrderDetailPermissionKind): string {
    switch (kind) {
        case "openPurchase":
            return "当前账号没有采购单列表权限"
        case "createPurchase":
            return "当前账号没有创建采购单权限"
        case "openFulfillment":
            return "当前账号没有履约/交付工作面权限"
        case "openReceivable":
            return "当前账号没有客户往来权限"
        case "registerAcceptance":
            return "当前账号没有登记客户验收权限"
        case "cancelApproval":
            return "当前账号没有撤回销售单审批权限"
        case "startChange":
            return "当前账号没有发起销售变更权限"
        case "voidOrder":
            return "当前账号没有作废销售单权限"
        case "editAfterRejection":
            return "当前账号没有编辑销售单权限"
    }
}
