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
import { useAdminMutations } from "@/features/admin/queries"

/** 删除账号确认对话框；系统内置账号会被后端拒绝删除。 */
export function DeleteAdminDialog({
    account,
    onOpenChange,
}: {
    account: { id: string; account: string }
    onOpenChange: (open: boolean) => void
}) {
    const { deleteAdmin, isDeleting } = useAdminMutations()
    const [error, setError] = React.useState<string | null>(null)

    return (
        <AlertDialog open onOpenChange={(open) => !open && onOpenChange(false)}>
            <AlertDialogContent>
                <AlertDialogHeader>
                    <AlertDialogTitle>
                        删除账号 {account.account}
                    </AlertDialogTitle>
                    <AlertDialogDescription>
                        删除后该账号无法登录后台。系统内置账号会被后端拒绝删除；操作会记录审计日志。
                    </AlertDialogDescription>
                </AlertDialogHeader>
                {error ? (
                    <Alert variant="destructive" role="alert">
                        <AlertTitle>删除失败</AlertTitle>
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                ) : null}
                <AlertDialogFooter>
                    <AlertDialogCancel disabled={isDeleting}>
                        取消
                    </AlertDialogCancel>
                    <AlertDialogAction
                        variant="destructive"
                        disabled={isDeleting}
                        onClick={async () => {
                            setError(null)
                            try {
                                await deleteAdmin(account.id)
                                onOpenChange(false)
                            } catch (e) {
                                setError(
                                    getErrorMessage(e, "删除失败，请重试。"),
                                )
                            }
                        }}
                    >
                        {isDeleting ? "删除中…" : "确认删除"}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}
