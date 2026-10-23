"use client"

import * as React from "react"
import { getErrorMessage } from "@/lib/api/errors"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { useAdminMutations } from "@/features/admin/hooks/queries"
import type { AdminAccountStatus } from "@/features/admin/types"

/** 停用或启用账号确认对话框；停用后该账号的现有会话立即失效。 */
export function ToggleAdminStatusDialog({
    account,
    onOpenChange,
    onChanged,
    id = "governance-admin-toggle-account-status-dialog",
}: {
    account: { id: string; account: string; status: AdminAccountStatus }
    onOpenChange: (open: boolean) => void
    onChanged?: (status: AdminAccountStatus) => void
    id?: string
}) {
    const { updateAdmin, isUpdating } = useAdminMutations()
    const [error, setError] = React.useState<string | null>(null)
    const nextStatus: AdminAccountStatus =
        account.status === "active" ? "suspended" : "active"
    const disabling = nextStatus === "suspended"

    return (
        <AlertDialog open onOpenChange={(open) => !open && onOpenChange(false)}>
            <AlertDialogContent>
                <AlertDialogHeader>
                    <AlertDialogTitle>
                        {disabling ? "停用账号" : "启用账号"} {account.account}
                    </AlertDialogTitle>
                    <AlertDialogDescription>
                        {disabling
                            ? "停用后，该账号将立即无法登录，现有会话的后续请求也会被拒绝。角色和人员资料会保留，可以随时重新启用。"
                            : "启用后，该账号可以重新登录后台，原有角色和人员资料保持不变。"}
                    </AlertDialogDescription>
                </AlertDialogHeader>
                {error ? (
                    <Alert variant="destructive" role="alert">
                        <AlertTitle>
                            {disabling ? "停用失败" : "启用失败"}
                        </AlertTitle>
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                ) : null}
                <AlertDialogFooter>
                    <AlertDialogCancel
                        id={`${id}-cancel`}
                        disabled={isUpdating}
                    >
                        取消
                    </AlertDialogCancel>
                    <AlertDialogAction
                        loading={isUpdating}
                        id={`${id}-confirm`}
                        variant={disabling ? "destructive" : "default"}
                        disabled={isUpdating}
                        onClick={async () => {
                            setError(null)
                            try {
                                await updateAdmin({
                                    id: account.id,
                                    payload: { status: nextStatus },
                                })
                                onOpenChange(false)
                                onChanged?.(nextStatus)
                            } catch (e) {
                                setError(
                                    getErrorMessage(
                                        e,
                                        disabling
                                            ? "停用失败，请重试。"
                                            : "启用失败，请重试。",
                                    ),
                                )
                            }
                        }}
                    >
                        {isUpdating
                            ? disabling
                                ? "停用中…"
                                : "启用中…"
                            : disabling
                              ? "确认停用"
                              : "确认启用"}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}
