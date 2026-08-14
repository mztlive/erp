"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import {
    EyeIcon,
    MoreHorizontalIcon,
    ShieldOffIcon,
    Trash2Icon,
} from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { AccountDraft } from "@/features/admin/account-form-dialog"
import { RiskFlagsBadges } from "@/features/access-audit/components/risk-flags-badges"
import type {
    AccessChangeCommand,
    AccessListView,
    UserRow,
} from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

type AccountFormState = {
    mode: "create" | "edit"
    account: AccountDraft | null
}

type DeletingAccountState = {
    id: string
    account: string
}

type UseUserColumnsInput = {
    data?: AccessListView
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openExplain: (type: "ROLE" | "USER", id: string) => void
    startChange: (command: AccessChangeCommand) => Promise<void>
    setAccountForm: React.Dispatch<
        React.SetStateAction<AccountFormState | null>
    >
    setDeletingAccount: React.Dispatch<
        React.SetStateAction<DeletingAccountState | null>
    >
}

function useUserColumns({
    data,
    rowFocusRef,
    openExplain,
    startChange,
    setAccountForm,
    setDeletingAccount,
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
                            {row.original.userId}
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
                id: "period",
                header: "有效期间",
                cell: ({ row }) => (
                    <span
                        className="num text-xs text-muted-foreground"
                        title="只读记录；策略未配置时不可编辑预约/到期"
                    >
                        {formatDateTime(row.original.effectiveFrom, "full")}
                        {" ~ "}
                        {row.original.effectiveTo
                            ? formatDateTime(row.original.effectiveTo, "full")
                            : "长期"}
                    </span>
                ),
            },
            {
                id: "scope",
                header: "数据范围",
                cell: ({ row }) => row.original.dataScopeSummary,
            },
            {
                id: "status",
                header: "账号状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "risk",
                header: "风险",
                cell: ({ row }) => (
                    <RiskFlagsBadges flags={row.original.riskFlags} />
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
                                type="button"
                                size="xs"
                                variant="ghost"
                                ref={(el) => {
                                    rowFocusRef.current.set(user.id, el)
                                }}
                                onClick={() => openExplain("USER", user.userId)}
                            >
                                <EyeIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                有效权限
                            </Button>
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() =>
                                    setAccountForm({
                                        mode: "edit",
                                        account: {
                                            id: user.userId,
                                            account: user.accountName,
                                            name: user.displayName,
                                            role_ids: [...user.roleIds],
                                        },
                                    })
                                }
                            >
                                编辑
                            </Button>
                            <DropdownMenu>
                                <DropdownMenuTrigger
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
                                    {user.roleAssignmentId ? (
                                        <>
                                            <DropdownMenuItem
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
                                                        idempotencyKey:
                                                            "pending",
                                                    })
                                                }
                                            >
                                                <ShieldOffIcon aria-hidden="true" />
                                                紧急撤权
                                            </DropdownMenuItem>
                                            <DropdownMenuSeparator />
                                        </>
                                    ) : null}
                                    <DropdownMenuItem
                                        variant="destructive"
                                        onClick={() =>
                                            setDeletingAccount({
                                                id: user.userId,
                                                account: user.accountName,
                                            })
                                        }
                                    >
                                        <Trash2Icon aria-hidden="true" />
                                        删除
                                    </DropdownMenuItem>
                                </DropdownMenuContent>
                            </DropdownMenu>
                        </div>
                    )
                },
            },
        ],
        [
            openExplain,
            startChange,
            data?.permissionVersion,
            rowFocusRef,
            setAccountForm,
            setDeletingAccount,
        ],
    )
}

export { useUserColumns, type AccountFormState, type DeletingAccountState }
