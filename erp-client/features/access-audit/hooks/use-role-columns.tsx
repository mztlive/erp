"use client"

import { PencilIcon, Trash2Icon } from "lucide-react"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AccessColumnsInput } from "@/features/access-audit/hooks/access-columns-input"
import type { RoleRow } from "@/features/access-audit/types"

/** 角色列表展示身份、操作权限和关联账号，模块权限摘要沿用行预览。 */
function useRoleColumns({
    router,
    rowFocusRef,
    setDeletingRole,
    canEditRole,
    canDeleteRole,
}: AccessColumnsInput) {
    return React.useMemo<ColumnDef<RoleRow>[]>(
        () => [
            {
                id: "identity",
                header: "角色名称",
                meta: { width: "flex" },
                cell: ({ row }) => (
                    <TableIdentityCell
                        primary={row.original.name}
                        primaryTitle={row.original.name}
                        className="max-sm:max-w-table-column-default-min"
                    />
                ),
            },
            {
                id: "perms",
                header: "操作权限",
                meta: { width: "reference" },
                cell: ({ row }) => {
                    const role = row.original
                    const names = role.permissionGroups.map(
                        (group) => group.name,
                    )
                    const summary = names.slice(0, 3).join("、")
                    return (
                        <div className="flex min-w-0 flex-col items-start gap-1">
                            {role.allPermissions ? (
                                <BusinessStatusBadge
                                    context="list"
                                    tone="warning"
                                    label="全部权限"
                                />
                            ) : (
                                <span>
                                    <span className="num font-medium">
                                        {role.permissionCount}
                                    </span>{" "}
                                    项权限
                                </span>
                            )}
                            <TableTextCell
                                className="max-w-table-column-reference-min text-xs text-muted-foreground"
                                clamp={1}
                                title={names.join("、") || undefined}
                                value={
                                    role.allPermissions
                                        ? "可访问全部模块"
                                        : summary
                                          ? `${summary}${names.length > 3 ? ` 等 ${names.length} 个模块` : ""}`
                                          : "尚未配置操作权限"
                                }
                            />
                        </div>
                    )
                },
            },
            {
                id: "accounts",
                header: "绑定账号",
                meta: { width: "quantity", align: "end", numeric: true },
                cell: ({ row }) => {
                    const role = row.original
                    if (role.boundAccountCount === null)
                        return <TableEmptyCell />
                    return role.boundAccountCount === 0 ? (
                        <TableEmptyCell variant="label" label="未绑定" />
                    ) : (
                        <Button
                            id={`operations-access-roles-row-${toAutomationIdSegment(role.id)}-accounts`}
                            type="button"
                            variant="link"
                            size="xs"
                            className="num h-auto px-0"
                            title={`查看绑定「${role.name}」的账号`}
                            onClick={(event) => {
                                event.stopPropagation()
                                router.push(
                                    `/system/accounts?q=${encodeURIComponent(role.name)}`,
                                )
                            }}
                        >
                            {role.boundAccountCount} 个账号
                        </Button>
                    )
                },
            },
            ...((canEditRole || canDeleteRole
                ? [
                      {
                          id: "actions",
                          meta: { align: "end", width: "status" },
                          header: "操作",
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
                                              hidden: !canEditRole,
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
                                              hidden: !canDeleteRole,
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
                  ]
                : []) satisfies ColumnDef<RoleRow>[]),
        ],
        [router, rowFocusRef, setDeletingRole, canEditRole, canDeleteRole],
    )
}

export { useRoleColumns }
