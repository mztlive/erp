"use client"

import { useRoleMutations } from "@/features/admin/hooks/queries"

import { DeleteConfirmationDialog } from "../delete-confirmation-dialog"

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

    return (
        <DeleteConfirmationDialog
            id={id}
            title={`删除角色 ${role.name}`}
            description="删除后，已绑定账号将失去该角色提供的权限。"
            pending={isDeleting}
            onConfirm={() => deleteRole(role.id)}
            onClose={() => onOpenChange(false)}
        />
    )
}
