"use client"

import * as React from "react"
import { ChevronDownIcon, ChevronRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { actionLabel } from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useCreateDataScopeMutation,
    useDeleteDataScopeMutation,
} from "@/features/organization/hooks/queries"
import {
    asScopeRule,
    scopeDescription,
} from "@/features/organization/lib/scope-description"
import { DataScopeFormDialog } from "@/features/organization/components/data-scope-form-dialog"
import type { DataScopeRecord, OrgUnit } from "@/features/organization/types"
import { getErrorMessage } from "@/lib/api/errors"

export function RoleScopeEditor({
    role,
    resource,
    actions,
    rows,
    units,
    permissions,
    savedPermissions,
    disabled,
    summary,
    ready,
}: {
    role: { id: string; name: string } | null
    resource: string
    actions: readonly string[]
    rows: readonly DataScopeRecord[]
    units: readonly OrgUnit[]
    permissions: readonly string[]
    savedPermissions: readonly string[]
    disabled: boolean
    summary: string
    ready: boolean
}) {
    const [expanded, setExpanded] = React.useState(false)
    const [adding, setAdding] = React.useState<string[] | null>(null)
    const [exceptions, setExceptions] = React.useState(false)
    const [removing, setRemoving] = React.useState<string | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    const profile = useAccountProfileQuery()
    const create = useCreateDataScopeMutation()
    const remove = useDeleteDataScopeMutation()
    const selected = actions.filter((action) =>
        hasPermission(permissions, `${resource}:${action}`),
    )
    const pendingActions = selected.some(
        (action) => !hasPermission(savedPermissions, `${resource}:${action}`),
    )
    const busy = disabled || create.isPending || remove.isPending
    const canCreate = hasPermission(
        profile.data?.permissions,
        "data_scope:create",
    )
    const canDelete = hasPermission(
        profile.data?.permissions,
        "data_scope:delete",
    )
    return (
        <section className="border-t pt-6">
            <h3 className="text-base font-semibold">
                这些操作可以处理谁的数据？
            </h3>
            <Button
                id="role-access-scope-adjust"
                type="button"
                variant="outline"
                className="mt-4 h-auto min-h-11 w-full justify-between whitespace-normal text-left font-normal"
                onClick={() => setExpanded(!expanded)}
                aria-expanded={expanded}
                disabled={!ready || !role}
            >
                {summary}
                <ChevronDownIcon className="ml-2 size-4 shrink-0" />
            </Button>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
                {summary === "本人负责"
                    ? "每位人员只能处理分配给自己的业务数据。"
                    : "点击查看或调整适用范围；多条规则叠加生效。"}
            </p>
            {expanded && (
                <div className="mt-4 space-y-4 rounded-md border bg-muted/10 p-4">
                    <p className="text-xs leading-5 text-muted-foreground">
                        范围在此处独立保存，成功后立即生效。底部按钮只保存岗位名称与操作权限。
                    </p>
                    {pendingActions && (
                        <p className="text-sm text-amber-700">
                            新增操作尚未保存，请先保存本次调整，再配置新增操作的范围。
                        </p>
                    )}
                    {rows.length === 0 && (
                        <p className="text-sm">
                            尚未配置范围。需要范围授权的操作不会获得业务数据。
                        </p>
                    )}
                    <ul className="divide-y">
                        {rows.map((row) => (
                            <li key={row.id} className="space-y-2 py-3 text-sm">
                                <p className="font-medium">
                                    {scopeDescription(asScopeRule(row), units)}
                                </p>
                                <p className="text-xs leading-5 text-muted-foreground">
                                    适用操作：
                                    {row.actions.map(actionLabel).join("、")}
                                    <br />
                                    限制维度：
                                    {row.targetDimension === "internal_org"
                                        ? "负责人及部门"
                                        : row.targetDimension === "warehouse"
                                          ? "仓库"
                                          : "结算主体"}
                                </p>
                                {canDelete && (
                                    <Button
                                        id={`role-access-remove-${toAutomationIdSegment(row.id)}`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        disabled={busy}
                                        onClick={() => setRemoving(row.id)}
                                    >
                                        移除此范围
                                    </Button>
                                )}
                                {removing === row.id && (
                                    <div className="space-y-2 rounded-md bg-muted p-3">
                                        <p>
                                            确认移除以上全部操作的这条范围？其他范围仍会保留。
                                        </p>
                                        <div className="flex gap-2">
                                            <Button
                                                id={`role-access-confirm-${toAutomationIdSegment(row.id)}`}
                                                type="button"
                                                size="sm"
                                                disabled={busy}
                                                onClick={async () => {
                                                    try {
                                                        setError(null)
                                                        await remove.mutateAsync(
                                                            row.id,
                                                        )
                                                        setRemoving(null)
                                                    } catch (failure) {
                                                        setError(
                                                            getErrorMessage(
                                                                failure,
                                                                "移除失败，请重试",
                                                            ),
                                                        )
                                                    }
                                                }}
                                            >
                                                确认移除
                                            </Button>
                                            <Button
                                                id={`role-access-cancel-${toAutomationIdSegment(row.id)}`}
                                                type="button"
                                                variant="outline"
                                                size="sm"
                                                disabled={busy}
                                                onClick={() =>
                                                    setRemoving(null)
                                                }
                                            >
                                                保留
                                            </Button>
                                        </div>
                                    </div>
                                )}
                            </li>
                        ))}
                    </ul>
                    {canCreate && (
                        <Button
                            id="role-access-add-scope"
                            type="button"
                            variant="outline"
                            disabled={
                                busy ||
                                pendingActions ||
                                !selected.length ||
                                !role
                            }
                            onClick={() => setAdding(selected)}
                        >
                            配置操作范围
                        </Button>
                    )}
                    <p className="text-xs leading-5 text-muted-foreground">
                        收窄范围时须移除原有宽范围；新增窄范围不会覆盖已有授权。
                    </p>
                    {error && (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    )}
                </div>
            )}
            <Button
                id="role-access-scope-exceptions"
                type="button"
                variant="outline"
                className="mt-5 h-auto min-h-12 w-full justify-start whitespace-normal font-normal"
                disabled={!ready || !role}
                onClick={() => setExceptions(!exceptions)}
                aria-expanded={exceptions}
            >
                <ChevronRightIcon className="mr-2 size-4" />
                为个别操作设置不同范围
                <span className="ml-auto text-xs text-muted-foreground">
                    展开规则
                </span>
            </Button>
            {exceptions && (
                <div className="mt-3 divide-y rounded-md border px-4">
                    <p className="py-3 text-xs leading-5 text-muted-foreground">
                        逐项核对适用范围。新增规则会叠加；收窄须在上方移除原有宽范围。
                    </p>
                    {selected.length === 0 && (
                        <p className="py-3 text-sm">先选择需要配置的操作。</p>
                    )}
                    {selected.map((action) => (
                        <div
                            key={action}
                            className="flex flex-wrap items-center justify-between gap-3 py-3 text-sm"
                        >
                            <div className="min-w-0 flex-1">
                                <p className="font-medium">
                                    {actionLabel(action)}
                                </p>
                                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                                    {rows
                                        .filter(
                                            (row) =>
                                                row.enabled &&
                                                row.actions.includes(action),
                                        )
                                        .map((row) =>
                                            scopeDescription(
                                                asScopeRule(row),
                                                units,
                                            ),
                                        )
                                        .join("；") || "尚未配置范围"}
                                </p>
                            </div>
                            {canCreate && (
                                <Button
                                    id={`role-access-action-scope-${toAutomationIdSegment(action)}`}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        busy ||
                                        !hasPermission(
                                            savedPermissions,
                                            `${resource}:${action}`,
                                        )
                                    }
                                    onClick={() => setAdding([action])}
                                >
                                    配置范围
                                </Button>
                            )}
                        </div>
                    ))}
                </div>
            )}
            {adding && role && (
                <DataScopeFormDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setAdding(null)
                    }}
                    subject={{ type: "role", id: role.id, label: role.name }}
                    roles={[]}
                    people={[]}
                    units={[...units]}
                    submitting={create.isPending}
                    onSubmit={async (input) => {
                        await create.mutateAsync(input)
                    }}
                    initialResource={resource}
                    initialActions={adding}
                    permissions={savedPermissions}
                />
            )}
        </section>
    )
}
