"use client"

import { UserRoundCogIcon } from "lucide-react"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { TableRowActions } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
    previewingCommand?: AccessChangeCommand | null
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
    rowFocusRef,
    setRoleAssignment,
}: UseUserColumnsInput) {
    return React.useMemo<ColumnDef<UserRow>[]>(
        () => [
            {
                id: "identity",
                header: "用户",
                cell: ({ row }) => (
                    <div className="flex min-w-[9rem] items-center gap-2">
                        <span
                            aria-hidden="true"
                            className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium text-foreground"
                        >
                            {row.original.displayName.slice(0, 1)}
                        </span>
                        <div className="min-w-0">
                            <div className="truncate font-medium">
                                {row.original.displayName}
                            </div>
                            <div className="truncate font-mono text-tiny text-muted-foreground">
                                {row.original.accountName}
                            </div>
                        </div>
                    </div>
                ),
            },
            {
                id: "roles",
                header: "当前角色",
                cell: ({ row }) => {
                    if (row.original.activeRoles === "—") {
                        return (
                            <span className="text-muted-foreground">
                                暂无角色
                            </span>
                        )
                    }
                    const names = row.original.activeRoles
                        .split("、")
                        .filter(Boolean)
                    return (
                        <div
                            className="flex max-w-[20rem] flex-wrap items-center gap-1"
                            title={row.original.activeRoles}
                        >
                            {names.slice(0, 2).map((name) => (
                                <Badge key={name} variant="secondary">
                                    {name}
                                </Badge>
                            ))}
                            {names.length > 2 ? (
                                <span className="text-xs text-muted-foreground">
                                    +{names.length - 2}
                                </span>
                            ) : null}
                        </div>
                    )
                },
            },
            {
                id: "scope",
                header: "数据范围",
                cell: ({ row }) => (
                    <span
                        className={
                            row.original.dataScopeSummary === "—"
                                ? "text-muted-foreground"
                                : ""
                        }
                    >
                        {row.original.dataScopeSummary}
                    </span>
                ),
            },
            {
                id: "actions",
                meta: { align: "end" },
                size: 196,
                minSize: 196,
                header: "操作",
                cell: ({ row }) => {
                    const user = row.original
                    const segment = toAutomationIdSegment(user.id)
                    return (
                        <TableRowActions
                            moreId={`operations-access-users-row-${segment}-menu-trigger`}
                            moreLabel={`${user.displayName} 更多操作`}
                            actions={[
                                {
                                    id: `operations-access-users-row-${segment}-adjust-role`,
                                    label: "调整角色",
                                    icon: UserRoundCogIcon,
                                    emphasis: "outline",
                                    buttonRef: (element) => {
                                        rowFocusRef.current.set(
                                            user.id,
                                            element,
                                        )
                                    },
                                    onClick: () =>
                                        setRoleAssignment({
                                            userId: user.userId,
                                            displayName: user.displayName,
                                            accountName: user.accountName,
                                            roleIds: user.roleIds,
                                        }),
                                },
                            ]}
                        />
                    )
                },
            },
        ],
        [rowFocusRef, setRoleAssignment],
    )
}

export { useUserColumns }
