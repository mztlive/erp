"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { MoreHorizontalIcon, ShieldOffIcon } from "lucide-react"

import { toAutomationIdSegment } from "@/lib/automation-id"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { RoleAssignmentTarget } from "@/features/access-audit/components/role-assignment-dialog"
import type {
    AccessChangeCommand,
    AccessListView,
    UserRow,
} from "@/features/access-audit/types"

type UseUserColumnsInput = {
    data?: AccessListView
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openExplain: (type: "ROLE" | "USER", id: string) => void
    startChange: (command: AccessChangeCommand) => Promise<void>
    setRoleAssignment: React.Dispatch<
        React.SetStateAction<RoleAssignmentTarget | null>
    >
}

/**
 * 用户授权列表列。
 *
 * 展示登录账号而不是账号内部 ID；后端没有的字段（有效期间、组织、风险、账号状态）
 * 不占列。有效权限由整行点击打开。
 */
function useUserColumns({
    data,
    rowFocusRef,
    startChange,
    setRoleAssignment,
}: UseUserColumnsInput) {
    return React.useMemo<ColumnDef<UserRow>[]>(
        () => [
            {
                id: "identity",
                header: "用户",
                cell: ({ row }) => (
                    <div className="min-w-[9rem]">
                        <div className="font-medium">
                            {row.original.displayName}
                        </div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.accountName}
                        </div>
                    </div>
                ),
            },
            {
                id: "roles",
                header: "当前角色",
                cell: ({ row }) => row.original.activeRoles,
            },
            {
                id: "scope",
                header: "数据范围",
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {row.original.dataScopeSummary}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => {
                    const user = row.original
                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
                                id={`operations-access-users-row-${toAutomationIdSegment(user.id)}-adjust-role`}
                                type="button"
                                size="xs"
                                variant="outline"
                                ref={(el) => {
                                    rowFocusRef.current.set(user.id, el)
                                }}
                                onClick={() =>
                                    setRoleAssignment({
                                        userId: user.userId,
                                        displayName: user.displayName,
                                        accountName: user.accountName,
                                        roleIds: user.roleIds,
                                    })
                                }
                            >
                                调整角色
                            </Button>
                            {user.roleAssignmentId ? (
                                <DropdownMenu>
                                    <DropdownMenuTrigger
                                        id={`operations-access-users-row-${toAutomationIdSegment(user.id)}-menu-trigger`}
                                        render={
                                            <Button
                                                type="button"
                                                size="icon-xs"
                                                variant="ghost"
                                                aria-label={`${user.displayName} 更多操作`}
                                            />
                                        }
                                    >
                                        <MoreHorizontalIcon aria-hidden="true" />
                                    </DropdownMenuTrigger>
                                    <DropdownMenuContent
                                        align="end"
                                        className="min-w-40"
                                    >
                                        <DropdownMenuItem
                                            id={`operations-access-users-row-${toAutomationIdSegment(user.id)}-emergency-revoke`}
                                            variant="destructive"
                                            onClick={() =>
                                                void startChange({
                                                    subjectType: "USER",
                                                    subjectId: user.userId,
                                                    action: "EMERGENCY_REVOKE_USER_ROLE",
                                                    roleAssignmentId:
                                                        user.roleAssignmentId!,
                                                    expectedPermissionVersion:
                                                        data?.permissionVersion ??
                                                        user.permissionVersion,
                                                    reasonCode:
                                                        "EMERGENCY_STOP_LOSS",
                                                    idempotencyKey: "pending",
                                                })
                                            }
                                        >
                                            <ShieldOffIcon aria-hidden="true" />
                                            紧急撤权
                                        </DropdownMenuItem>
                                    </DropdownMenuContent>
                                </DropdownMenu>
                            ) : null}
                        </div>
                    )
                },
            },
        ],
        [startChange, data?.permissionVersion, rowFocusRef, setRoleAssignment],
    )
}

export { useUserColumns }
