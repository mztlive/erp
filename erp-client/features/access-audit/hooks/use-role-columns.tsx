"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { EyeIcon, MoreHorizontalIcon, Trash2Icon } from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { riskLabel } from "@/features/access-audit/lib/risk-labels"
import type { AccessColumnsInput } from "@/features/access-audit/hooks/access-columns-input"
import type { RoleRow } from "@/features/access-audit/types"

function useRoleColumns({
    data,
    router,
    rowFocusRef,
    openExplain,
    startChange,
    setDeletingRole,
}: AccessColumnsInput) {
    return React.useMemo<ColumnDef<RoleRow>[]>(
        () => [
            {
                id: "identity",
                header: "角色",
                cell: ({ row }) => (
                    <div className="min-w-[10rem]">
                        <div className="font-medium">{row.original.name}</div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.roleCode}
                        </div>
                    </div>
                ),
            },
            {
                id: "org",
                header: "组织",
                cell: ({ row }) => row.original.organizationLabel,
            },
            {
                id: "perms",
                header: "模块与动作权限",
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {row.original.permissionSummary}
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
                header: "状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "version",
                header: "版本",
                cell: ({ row }) => (
                    <span className="num text-xs">
                        v{row.original.permissionVersion.split("-").at(-1)}
                    </span>
                ),
            },
            {
                id: "risk",
                header: "风险",
                cell: ({ row }) =>
                    row.original.riskFlags.length ? (
                        <div className="flex flex-wrap gap-1">
                            {row.original.riskFlags.map((f) => (
                                <Badge key={f} variant="warning">
                                    {riskLabel(f)}
                                </Badge>
                            ))}
                        </div>
                    ) : (
                        "—"
                    ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => {
                    const role = row.original
                    const version =
                        data?.permissionVersion ?? role.permissionVersion
                    const canAdjust =
                        role.status === "enabled" &&
                        !role.riskFlags.includes("HIGH_PRIVILEGE")
                    const canExpand = role.riskFlags.includes("HIGH_PRIVILEGE")
                    const canDisable =
                        role.status === "enabled" &&
                        role.riskFlags.includes("PENDING_DISABLE")

                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                ref={(el) => {
                                    rowFocusRef.current.set(role.id, el)
                                }}
                                onClick={() => openExplain("ROLE", role.id)}
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
                                    {canAdjust ? (
                                        <DropdownMenuItem
                                            onClick={() =>
                                                void startChange({
                                                    subjectType: "ROLE",
                                                    subjectId: role.id,
                                                    action: "UPDATE_ROLE_PERMISSIONS",
                                                    expectedPermissionVersion:
                                                        version,
                                                    reasonCode: "SECURITY_OPS",
                                                    idempotencyKey: "pending",
                                                    changeSet: [
                                                        {
                                                            targetReference:
                                                                "W22.publish",
                                                            operation: "REMOVE",
                                                        },
                                                    ],
                                                })
                                            }
                                        >
                                            调整权限
                                        </DropdownMenuItem>
                                    ) : null}
                                    {canExpand ? (
                                        <DropdownMenuItem
                                            onClick={() =>
                                                void startChange({
                                                    subjectType: "ROLE",
                                                    subjectId: role.id,
                                                    action: "UPDATE_ROLE_PERMISSIONS",
                                                    expectedPermissionVersion:
                                                        version,
                                                    reasonCode: "SECURITY_OPS",
                                                    idempotencyKey: "pending",
                                                    changeSet: [
                                                        {
                                                            targetReference:
                                                                "sensitive.field.expand",
                                                            operation: "ADD",
                                                            valueReference:
                                                                "FULL_COMPANY",
                                                        },
                                                    ],
                                                })
                                            }
                                        >
                                            扩权（将阻断）
                                        </DropdownMenuItem>
                                    ) : null}
                                    {canDisable ? (
                                        <DropdownMenuItem
                                            onClick={() =>
                                                void startChange({
                                                    subjectType: "ROLE",
                                                    subjectId: role.id,
                                                    action: "DISABLE_ROLE",
                                                    expectedPermissionVersion:
                                                        version,
                                                    reasonCode: "SECURITY_OPS",
                                                    idempotencyKey: "pending",
                                                    changeSet: [
                                                        {
                                                            targetReference:
                                                                "status",
                                                            operation:
                                                                "REPLACE",
                                                            valueReference:
                                                                "disabled",
                                                        },
                                                    ],
                                                })
                                            }
                                        >
                                            停用
                                        </DropdownMenuItem>
                                    ) : null}
                                    {(canAdjust || canExpand || canDisable) && (
                                        <DropdownMenuSeparator />
                                    )}
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
        [
            openExplain,
            startChange,
            router,
            data?.permissionVersion,
            rowFocusRef,
            setDeletingRole,
        ],
    )
}

export { useRoleColumns }
