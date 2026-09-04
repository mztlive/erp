"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { Trash2Icon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AccessColumnsInput } from "@/features/access-audit/hooks/access-columns-input"
import type { RoleRow } from "@/features/access-audit/types"

/**
 * 角色列表列。
 *
 * 四列：角色（含首字锚点与数据范围小字）／权限（纯数字右对齐）／
 * 主要模块（纯文本）／绑定账号（可跳转的链接数字）。
 * 逐条权限编码留给有效权限面板（整行点击打开）；后端没有的字段
 * （组织、风险、角色状态）不占列。
 */
function useRoleColumns({
    router,
    rowFocusRef,
    setDeletingRole,
}: AccessColumnsInput) {
    return React.useMemo<ColumnDef<RoleRow>[]>(
        () => [
            {
                id: "identity",
                header: "角色",
                cell: ({ row }) => {
                    const role = row.original
                    const scopeLine =
                        role.dataScopeSummary === "—"
                            ? null
                            : role.dataScopeSummary
                    return (
                        <div
                            className="flex min-w-[10rem] items-center gap-2"
                            title={`编码 ${role.roleCode}`}
                        >
                            <span
                                aria-hidden="true"
                                className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium text-foreground"
                            >
                                {role.name.slice(0, 1)}
                            </span>
                            <div className="min-w-0">
                                <div className="truncate text-sm font-medium">
                                    {role.name}
                                </div>
                                <div className="truncate text-xs text-muted-foreground">
                                    {scopeLine ?? (
                                        <span className="font-mono text-[11px]">
                                            {role.roleCode}
                                        </span>
                                    )}
                                </div>
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "perms",
                header: () => <span className="block text-right">权限</span>,
                cell: ({ row }) => {
                    const role = row.original
                    if (role.allPermissions) {
                        return (
                            <div className="flex justify-end">
                                <Badge variant="warning">全部权限</Badge>
                            </div>
                        )
                    }
                    return (
                        <span
                            className="num block text-right text-sm font-medium"
                            title={
                                role.permissionCount === 0
                                    ? "无权限条目"
                                    : `共 ${role.permissionCount} 项权限`
                            }
                        >
                            {role.permissionCount}
                        </span>
                    )
                },
            },
            {
                id: "modules",
                header: "主要模块",
                cell: ({ row }) => {
                    const role = row.original
                    if (role.allPermissions) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                全部模块
                            </span>
                        )
                    }
                    if (role.permissionGroups.length === 0) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    const names = role.permissionGroups.map(
                        (group) => group.name,
                    )
                    const full = role.permissionGroups
                        .map((group) => `${group.name} ${group.count}`)
                        .join(" · ")
                    const visible = names.slice(0, 2).join("、")
                    return (
                        <span
                            className="block max-w-[16rem] truncate text-sm"
                            title={full}
                        >
                            {visible}
                            {names.length > 2 ? (
                                <span className="text-xs text-muted-foreground">
                                    {` +${names.length - 2}`}
                                </span>
                            ) : null}
                        </span>
                    )
                },
            },
            {
                id: "accounts",
                header: "绑定账号",
                cell: ({ row }) => {
                    const role = row.original
                    if (role.boundAccountCount === 0) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    return (
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
                            {role.boundAccountCount}
                        </Button>
                    )
                },
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => {
                    const role = row.original

                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
                                id={`operations-access-roles-row-${toAutomationIdSegment(role.id)}-edit`}
                                type="button"
                                size="xs"
                                variant="outline"
                                ref={(el) => {
                                    rowFocusRef.current.set(role.id, el)
                                }}
                                onClick={() =>
                                    router.push(`/system/roles/${role.id}/edit`)
                                }
                            >
                                编辑
                            </Button>
                            <Button
                                id={`operations-access-roles-row-${toAutomationIdSegment(role.id)}-delete`}
                                type="button"
                                size="xs"
                                variant="destructive"
                                onClick={() =>
                                    setDeletingRole({
                                        id: role.id,
                                        name: role.name,
                                    })
                                }
                            >
                                <Trash2Icon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                删除
                            </Button>
                        </div>
                    )
                },
            },
        ],
        [router, rowFocusRef, setDeletingRole],
    )
}

export { useRoleColumns }
