"use client"

import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import type {
    AllocationMode,
    ReverseFactInput,
} from "@/features/customer-receivables/types"
import { getErrorMessage } from "@/lib/api/errors"
import { hasAnyPermission, hasPermission } from "@/lib/permissions"

export function useCustomerReceivablesPermissions() {
    const profileQuery = useAccountProfileQuery()
    const granted = profileQuery.data?.permissions
    const ready = Boolean(profileQuery.data) && !profileQuery.isError

    const canRegisterReceipt =
        ready &&
        hasPermission(granted, "customer_receipt:create") &&
        hasPermission(granted, "customer_receipt:submit")
    const canRegisterInvoice =
        ready &&
        hasPermission(granted, "invoice:create") &&
        hasPermission(granted, "invoice:post")

    const canReverseReceipt =
        ready &&
        hasPermission(granted, "receipt_reversal:create") &&
        hasPermission(granted, "receipt_reversal:submit")
    const canRefund =
        ready &&
        hasPermission(granted, "customer_refund:create") &&
        hasPermission(granted, "customer_refund:submit")
    const canRedInvoice = ready && hasPermission(granted, "invoice:reverse")

    const reason = profileQuery.isPending
        ? "正在核对操作权限，请稍候。"
        : profileQuery.isError
          ? getErrorMessage(
                profileQuery.error,
                "暂时无法核对操作权限，请刷新后重试。",
            )
          : "当前账号没有执行此操作的权限。"

    return {
        canRegisterReceipt,
        canRegisterInvoice,
        canExport:
            ready &&
            hasAnyPermission(granted, [
                "receivable_account:list",
                "customer_receipt:list",
                "invoice:list",
            ]),
        canReverseReceipt,
        canRefund,
        canRedInvoice,
        canSubmitRefund:
            ready && hasPermission(granted, "customer_refund:submit"),
        canSubmitReversal:
            ready && hasPermission(granted, "receipt_reversal:submit"),
        canStartSession: (mode: AllocationMode) =>
            mode === "receipt" ? canRegisterReceipt : canRegisterInvoice,
        canReverse: (kind: ReverseFactInput["kind"]) =>
            kind === "receipt_reverse"
                ? canReverseReceipt
                : kind === "refund"
                  ? canRefund
                  : canRedInvoice,
        reason,
    }
}
