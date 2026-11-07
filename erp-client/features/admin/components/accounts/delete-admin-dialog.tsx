"use client"

import { useAdminMutations } from "@/features/admin/hooks/queries"

import { DeleteConfirmationDialog } from "../delete-confirmation-dialog"

/** 删除账号确认对话框；系统内置账号会被后端拒绝删除。 */
export function DeleteAdminDialog({
    account,
    onOpenChange,
    onDeleted,
    id = "governance-admin-delete-account-dialog",
}: {
    account: { id: string; account: string }
    onOpenChange: (open: boolean) => void
    onDeleted?: () => void
    id?: string
}) {
    const { deleteAdmin, isDeleting } = useAdminMutations()

    return (
        <DeleteConfirmationDialog
            id={id}
            title={`删除账号 ${account.account}`}
            description="删除后，该账号将无法登录后台。"
            pending={isDeleting}
            onConfirm={() => deleteAdmin(account.id)}
            onClose={() => onOpenChange(false)}
            onDeleted={onDeleted}
        />
    )
}
