"use client"

import * as React from "react"
import { Button } from "@/components/ui/button"
import { BusinessFailureState } from "@/components/business"
import { Checkbox } from "@/components/ui/checkbox"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import { PERMISSION_BY_CODE } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useDataScopesQuery,
    useOrganizationStateQuery,
    useCreateDataScopeMutation,
    useDeleteDataScopeMutation,
} from "@/features/organization/hooks/queries"
import { registeredResources } from "@/features/organization/lib/scope-payload"
import {
    asScopeRule,
    scopeDescription,
} from "@/features/organization/lib/scope-description"
import { DataScopeFormDialog } from "@/features/organization/components/data-scope-form-dialog"
import { getErrorMessage } from "@/lib/api/errors"

export function RoleAccessMatrix({
    role,
    permissions,
    savedPermissions,
    onChange,
    disabled,
}: {
    role: { id: string; name: string } | null
    permissions: readonly string[]
    savedPermissions: readonly string[]
    onChange: (permissions: string[]) => void
    disabled: boolean
}) {
    const [resource, setResource] = React.useState("sales_order")
    const [adding, setAdding] = React.useState<string | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    const [removing, setRemoving] = React.useState<string | null>(null)
    const profile = useAccountProfileQuery()
    const canRead = hasPermission(profile.data?.permissions, "data_scope:list")
    const scopes = useDataScopesQuery(
        { subjectType: "role", subjectId: role?.id, scopeType: "all" },
        Boolean(role) && canRead,
    )
    const org = useOrganizationStateQuery(
        hasPermission(profile.data?.permissions, "org_unit:list"),
    )
    const create = useCreateDataScopeMutation()
    const remove = useDeleteDataScopeMutation()
    const registered = registeredResources()
        .map((entry) => ({
            ...entry,
            actions: entry.actions.filter(
                (action) =>
                    PERMISSION_BY_CODE.has(`${entry.resource}:${action}`) ||
                    hasPermission(
                        savedPermissions,
                        `${entry.resource}:${action}`,
                    ),
            ),
        }))
        .filter((entry) => entry.actions.length > 0)
    const entry = registered.find((item) => item.resource === resource)!
    const rows =
        scopes.data?.items.filter((row) => row.resource === resource) ?? []
    const pending = disabled || create.isPending || remove.isPending
    return (
        <section className="min-h-0 flex-1 space-y-4 overflow-y-auto py-4">
            <p className="text-sm text-muted-foreground">
                1. 选择业务和操作 → 2. 保存操作权限 → 3.
                配置对应范围。范围逐条保存，不会自动授予操作权限。
            </p>
            <label
                className="block space-y-2 text-sm"
                htmlFor="role-access-resource"
            >
                <span>配置业务</span>
                <select
                    id="role-access-resource"
                    className="h-10 w-full rounded-md border bg-background px-3"
                    value={resource}
                    onChange={(event) => {
                        setResource(event.target.value)
                        setAdding(null)
                        setRemoving(null)
                    }}
                >
                    {registered.map((item) => (
                        <option key={item.resource} value={item.resource}>
                            {resourceLabel(item.resource)}
                        </option>
                    ))}
                </select>
            </label>
            {!role && (
                <p role="status" className="rounded-md bg-muted p-3 text-sm">
                    先创建角色。创建成功后将留在此处继续配置范围。
                </p>
            )}
            {role && !canRead && (
                <p>没有查看范围配置的权限，请由权限管理员核对。</p>
            )}
            {scopes.isError && (
                <BusinessFailureState
                    error={scopes.error}
                    onRetry={() => void scopes.refetch()}
                    id="role-access-retry"
                />
            )}
            {role && canRead && scopes.isPending && (
                <p role="status">正在读取范围，暂不能判断配置是否完整…</p>
            )}
            <div className="divide-y rounded-lg border px-4">
                {entry.actions.map((action) => {
                    const code = `${resource}:${action}`
                    const selected = hasPermission(permissions, code)
                    const saved = hasPermission(savedPermissions, code)
                    const rules = rows.filter(
                        (row) => row.enabled && row.actions.includes(action),
                    )
                    return (
                        <div
                            key={action}
                            className="grid gap-3 py-4 sm:grid-cols-[10rem_1fr_auto]"
                        >
                            <label
                                className="flex items-center gap-2"
                                htmlFor={`role-access-${code.replaceAll(":", "-")}`}
                            >
                                <Checkbox
                                    id={`role-access-${code.replaceAll(":", "-")}`}
                                    checked={selected}
                                    disabled={
                                        pending ||
                                        !PERMISSION_BY_CODE.has(code) ||
                                        (selected &&
                                            !permissions.includes(code))
                                    }
                                    onCheckedChange={(checked) =>
                                        onChange(
                                            checked
                                                ? [
                                                      ...new Set([
                                                          ...permissions,
                                                          code,
                                                      ]),
                                                  ]
                                                : permissions.filter(
                                                      (item) => item !== code,
                                                  ),
                                        )
                                    }
                                />
                                {actionLabel(action)}
                            </label>
                            <div className="space-y-1 text-sm">
                                {!selected ? (
                                    "未授予操作权限"
                                ) : !saved || !role ? (
                                    "操作权限待保存"
                                ) : !canRead || !scopes.isSuccess ? (
                                    "范围待确认"
                                ) : rules.length ? (
                                    rules.map((rule) => (
                                        <p key={rule.id}>
                                            {scopeDescription(
                                                asScopeRule(rule),
                                                org.data?.units,
                                            )}
                                        </p>
                                    ))
                                ) : (
                                    <p className="text-amber-700">
                                        尚未配置角色范围；合法历史读取按业务规则判断。
                                    </p>
                                )}
                            </div>
                            {selected &&
                                hasPermission(
                                    profile.data?.permissions,
                                    "data_scope:create",
                                ) && (
                                    <Button
                                        id={`role-access-add-${action}`}
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={pending || !saved || !role}
                                        onClick={() => setAdding(action)}
                                    >
                                        添加范围
                                    </Button>
                                )}
                        </div>
                    )
                })}
            </div>
            {rows.length > 0 && (
                <div className="space-y-2">
                    <h3 className="text-sm font-medium">本业务已有范围规则</h3>
                    <p className="text-xs text-muted-foreground">
                        新增规则会叠加授权。要收窄范围，请核对并移除原有宽范围；移除一条规则会影响它包含的全部操作。
                    </p>
                    {rows.map((row) => (
                        <div
                            key={row.id}
                            className="flex flex-wrap items-center justify-between gap-2 rounded-md border p-3 text-sm"
                        >
                            <span>
                                {row.actions.map(actionLabel).join("、")} ·{" "}
                                {scopeDescription(
                                    asScopeRule(row),
                                    org.data?.units,
                                )}
                            </span>
                            {hasPermission(
                                profile.data?.permissions,
                                "data_scope:delete",
                            ) && (
                                <Button
                                    id={`role-access-remove-${toAutomationIdSegment(row.id)}`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    disabled={pending}
                                    onClick={async () => {
                                        if (removing !== row.id) {
                                            setRemoving(row.id)
                                            return
                                        }
                                        try {
                                            setError(null)
                                            await remove.mutateAsync(row.id)
                                            setRemoving(null)
                                        } catch (failure) {
                                            setError(
                                                getErrorMessage(
                                                    failure,
                                                    "移除失败",
                                                ),
                                            )
                                        }
                                    }}
                                >
                                    {removing === row.id
                                        ? "确认移除这些操作的范围"
                                        : "移除规则"}
                                </Button>
                            )}
                            {removing === row.id && (
                                <Button
                                    id={`role-access-remove-cancel-${toAutomationIdSegment(row.id)}`}
                                    type="button"
                                    variant="ghost"
                                    onClick={() => setRemoving(null)}
                                >
                                    取消
                                </Button>
                            )}
                        </div>
                    ))}
                </div>
            )}
            {error && (
                <p role="alert" className="text-sm text-destructive">
                    {error}
                </p>
            )}
            {adding && role && (
                <DataScopeFormDialog
                    key={`${resource}-${adding}`}
                    open
                    onOpenChange={(open) => {
                        if (!open) setAdding(null)
                    }}
                    subject={{ type: "role", id: role.id, label: role.name }}
                    roles={[]}
                    people={[]}
                    units={org.data?.units ?? []}
                    submitting={create.isPending}
                    onSubmit={async (input) => {
                        await create.mutateAsync(input)
                    }}
                    initialResource={resource}
                    initialActions={[adding]}
                    permissions={savedPermissions}
                />
            )}
        </section>
    )
}
