"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { MoreHorizontalIcon, Trash2Icon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { AccessColumnsInput } from "@/features/access-audit/hooks/access-columns-input"
import type { RoleRow } from "@/features/access-audit/types"

/**
 * 角色列表列。
 *
 * 权限列给摘要不给编码：一屏能比较「谁的权限更大」，逐条编码留给有效权限面板
 * （整行点击打开）。后端没有的字段（组织、风险、角色状态）不占列。
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
                cell: ({ row }) => (
                    <div className="min-w-[8rem]">
                        <div className="font-medium">{row.original.name}</div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.roleCode}
                        </div>
                    </div>
                ),
            },
            {
                id: "perms",
                header: "权限覆盖",
                cell: ({ row }) => {
                    const role = row.original
                    if (role.allPermissions) {
                        return <Badge variant="warning">全部权限</Badge>
                    }
                    if (role.permissionCount === 0) {
                        return (
                            <span className="text-muted-foreground">
                                无权限条目
                            </span>
                        )
                    }
                    return (
                        <div className="flex min-w-[14rem] flex-wrap items-center gap-1.5">
                            <span className="text-sm">
                                共{" "}
                                <span className="num">
                                    {role.permissionCount}
                                </span>{" "}
                                项
                            </span>
                            {role.permissionGroups
                                .slice(0, 3)
                                .map((group) => (
                                    <Badge key={group.name} variant="outline">
                                        {group.name}
                                        <span className="num">
                                            {group.count}
                                        </span>
                                    </Badge>
                                ))}
                            {role.permissionGroups.length > 3 ? (
                                <span className="text-xs text-muted-foreground">
                                    +{role.permissionGroups.length - 3} 个模块
                                </span>
                            ) : null}
                        </div>
                    )
                },
            },
            {
                id: "accounts",
                header: "绑定账号",
                cell: ({ row }) =>
                    row.original.boundAccountCount > 0 ? (
                        <span className="num">
                            {row.original.boundAccountCount}
                        </span>
                    ) : (
                        <span className="text-muted-foreground">—</span>
                    ),
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
                    const role = row.original

                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
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
                            <DropdownMenu>
                                <DropdownMenuTrigger
                                    render={
                                        <Button
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label={`${role.name} 更多操作`}
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
                                        variant="destructive"
                                        onClick={() =>
                                            setDeletingRole({
                                                id: role.id,
                                                name: role.name,
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
        [router, rowFocusRef, setDeletingRole],
    )
}

export { useRoleColumns }
