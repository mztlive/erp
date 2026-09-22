"use client"

import { PencilIcon, Trash2Icon } from "lucide-react"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { TableRowActions } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AccessColumnsInput } from "@/features/access-audit/hooks/access-columns-input"
import type { RoleRow } from "@/features/access-audit/types"

/** 角色列表按身份、操作权限、数据范围、关联账号组织，详细来源沿用行预览。 */
function useRoleColumns({
    router,
    rowFocusRef,
    setDeletingRole,
}: AccessColumnsInput) {
    return React.useMemo<ColumnDef<RoleRow>[]>(
        () => [
            {
                id: "identity",
                header: "角色名称",
                size: 220,
                cell: ({ row }) => (
                    <span className="text-sm font-medium">
                        {row.original.name}
                    </span>
                ),
            },
            {
                id: "perms",
                header: "操作权限",
                size: 320,
                cell: ({ row }) => {
                    const role = row.original
                    const names = role.permissionGroups.map(
                        (group) => group.name,
                    )
                    const summary = names.slice(0, 3).join("、")
                    return (
                        <div className="min-w-0 space-y-1">
                            {role.allPermissions ? (
                                <Badge variant="warning">全部权限</Badge>
                            ) : (
                                <span className="text-sm">
                                    <span className="num font-medium">
                                        {role.permissionCount}
                                    </span>{" "}
                                    项权限
                                </span>
                            )}
                            <p
                                className="max-w-[24rem] truncate text-xs text-muted-foreground"
                                title={names.join("、")}
                            >
                                {role.allPermissions
                                    ? "可访问全部模块"
                                    : summary
                                      ? `${summary}${names.length > 3 ? ` 等 ${names.length} 个模块` : ""}`
                                      : "尚未配置操作权限"}
                            </p>
                        </div>
                    )
                },
            },
            {
                id: "scope",
                header: "数据范围",
                size: 180,
                cell: ({ row }) => (
                    <span
                        className="block max-w-[16rem] truncate text-sm"
                        title={row.original.dataScopeSummary}
                    >
                        {row.original.dataScopeSummary}
                    </span>
                ),
            },
            {
                id: "accounts",
                header: "绑定账号",
                size: 130,
                cell: ({ row }) => {
                    const role = row.original
                    return role.boundAccountCount === 0 ? (
                        <span className="text-sm text-muted-foreground">
                            未绑定
                        </span>
                    ) : (
                        <Button
                            id={`operations-access-roles-row-${toAutomationIdSegment(role.id)}-accounts`}
                            type="button"
                            variant="link"
                            size="xs"
                            className="num h-auto px-0"
                            title={`查看绑定「${role.name}」的账号`}
                            onClick={() =>
                                router.push(
                                    `/system/accounts?q=${encodeURIComponent(role.name)}`,
                                )
                            }
                        >
                            {role.boundAccountCount} 个账号
                        </Button>
                    )
                },
            },
            {
                id: "actions",
                meta: { align: "end" },
                size: 168,
                minSize: 168,
                header: () => <span className="block text-right">操作</span>,
                cell: ({ row }) => {
                    const role = row.original
                    const segment = toAutomationIdSegment(role.id)
                    return (
                        <TableRowActions
                            moreId={`operations-access-roles-row-${segment}-more`}
                            moreLabel={`${role.name} 更多操作`}
                            actions={[
                                {
                                    id: `operations-access-roles-row-${segment}-edit`,
                                    label: "编辑",
                                    icon: PencilIcon,
                                    buttonRef: (element) => {
                                        rowFocusRef.current.set(
                                            role.id,
                                            element,
                                        )
                                    },
                                    onClick: () =>
                                        router.push(
                                            `/system/roles/${role.id}/edit`,
                                        ),
                                },
                                {
                                    id: `operations-access-roles-row-${segment}-delete`,
                                    label: "删除角色",
                                    icon: Trash2Icon,
                                    destructive: true,
                                    disabled: role.system === true,
                                    disabledReason: role.system
                                        ? "系统内置角色不可删除"
                                        : undefined,
                                    onClick: () =>
                                        setDeletingRole({
                                            id: role.id,
                                            name: role.name,
                                        }),
                                },
                            ]}
                        />
                    )
                },
            },
        ],
        [router, rowFocusRef, setDeletingRole],
    )
}

export { useRoleColumns }
