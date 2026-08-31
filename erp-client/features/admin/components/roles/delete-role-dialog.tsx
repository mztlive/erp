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
import { useRoleMutations } from "@/features/admin/hooks/queries"

/** 删除角色确认对话框；系统内置角色会被后端拒绝删除。 */
export function DeleteRoleDialog({
    role,
    onOpenChange,
    id = "governance-admin-delete-role-dialog",
}: {
    role: { id: string; name: string }
    onOpenChange: (open: boolean) => void
    id?: string
}) {
    const { deleteRole, isDeleting } = useRoleMutations()
    const [error, setError] = React.useState<string | null>(null)

    return (
        <AlertDialog open onOpenChange={(open) => !open && onOpenChange(false)}>
            <AlertDialogContent>
                <AlertDialogHeader>
                    <AlertDialogTitle>删除角色 {role.name}</AlertDialogTitle>
                    <AlertDialogDescription>
                        删除后该角色及权限策略一并移除，已绑定该角色的账号将失去对应权限。系统内置角色会被后端拒绝删除。
                    </AlertDialogDescription>
                </AlertDialogHeader>
                {error ? (
                    <Alert variant="destructive" role="alert">
                        <AlertTitle>删除失败</AlertTitle>
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                ) : null}
                <AlertDialogFooter>
                    <AlertDialogCancel
                        id={`${id}-cancel`}
                        disabled={isDeleting}
                    >
                        取消
                    </AlertDialogCancel>
                    <AlertDialogAction
                        id={`${id}-confirm`}
                        variant="destructive"
                        disabled={isDeleting}
                        onClick={async () => {
                            setError(null)
                            try {
                                await deleteRole(role.id)
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
