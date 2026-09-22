"use client"

import * as React from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useDataScopesQuery,
    useOrganizationStateQuery,
    useCreateDataScopeMutation,
    useDeleteDataScopeMutation,
} from "@/features/organization/hooks/queries"
import {
    SCOPE_TYPE_LABEL,
    TARGET_MODE_LABEL,
} from "@/features/organization/lib/labels"
import { unitLabel } from "@/features/organization/lib/tree"
import { DataScopeFormDialog } from "./data-scope-form-dialog"
import { getErrorMessage } from "@/lib/api/errors"

/** 在角色上下文内维护数据范围；主体固定，避免给另一个角色误授权。 */
export function SubjectScopesPanel({
    roleId,
    roleName,
}: {
    roleId: string
    roleName: string
}) {
    const { data: profile } = useAccountProfileQuery()
    const scopes = useDataScopesQuery({
        subjectType: "role",
        subjectId: roleId,
        scopeType: "all",
    })
    const org = useOrganizationStateQuery(
        hasPermission(profile?.permissions, "org_unit:list"),
    )
    const create = useCreateDataScopeMutation()
    const remove = useDeleteDataScopeMutation()
    const [adding, setAdding] = React.useState(false)
    const [removing, setRemoving] = React.useState<string | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    if (adding)
        return (
            <DataScopeFormDialog
                open
                embedded
                onOpenChange={setAdding}
                subject={{ type: "role", id: roleId, label: roleName }}
                roles={[{ id: roleId, name: roleName, enabled: true }]}
                people={[]}
                units={org.data?.units ?? []}
                submitting={create.isPending}
                onSubmit={async (input) => {
                    await create.mutateAsync(input)
                }}
            />
        )
    return (
        <div className="min-w-0 space-y-4">
            <p className="text-sm text-muted-foreground">
                操作权限决定能做什么，数据范围决定这些操作可用于哪些数据。本人部门随成员归属变化；管理部门需在组织与人员中单独设置。个人范围限制仍会进一步收窄结果。
            </p>
            {hasPermission(profile?.permissions, "data_scope:create") ? (
                <Button
                    id="role-scopes-create"
                    type="button"
                    onClick={() => setAdding(true)}
                >
                    添加数据范围
                </Button>
            ) : null}
            {error ? (
                <Alert variant="destructive">
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            ) : null}
            {scopes.isPending ? (
                <p role="status">正在加载数据范围…</p>
            ) : scopes.isError ? (
                <BusinessFailureState
                    id="role-scopes-retry"
                    title="数据范围加载失败"
                    error={scopes.error}
                    onRetry={() => {
                        void scopes.refetch()
                    }}
                />
            ) : !scopes.data?.items.length ? (
                <p className="rounded-lg bg-muted/30 p-5 text-sm">
                    尚未配置角色数据范围。需要范围授权的业务不会因此获得全公司数据；合法参与记录按业务规则另行判断。
                </p>
            ) : (
                <ul className="divide-y">
                    {scopes.data.items.map((row) => (
                        <li key={row.id} className="space-y-2 py-3">
                            <div className="flex flex-wrap justify-between gap-2">
                                <div className="min-w-0 text-sm">
                                    <p className="font-medium">
                                        {resourceLabel(row.resource)} ·{" "}
                                        {row.actions
                                            .map(actionLabel)
                                            .join("、")}
                                    </p>
                                    <p className="mt-1 text-muted-foreground">
                                        {SCOPE_TYPE_LABEL[row.scopeType]}
                                        {row.targetMode
                                            ? ` · ${TARGET_MODE_LABEL[row.targetMode]}`
                                            : ""}
                                        {row.includeDescendants
                                            ? " · 包含下级部门"
                                            : ""}
                                    </p>
                                    {row.scopeTargets.length ? (
                                        <p className="text-muted-foreground">
                                            {row.targetDimension ===
                                            "internal_org"
                                                ? row.scopeTargets
                                                      .map((target) =>
                                                          unitLabel(
                                                              org.data?.units ??
                                                                  [],
                                                              target,
                                                          ),
                                                      )
                                                      .join("、")
                                                : `已指定 ${row.scopeTargets.length} 个${row.targetDimension === "warehouse" ? "仓库" : "结算主体"}`}
                                        </p>
                                    ) : null}
                                </div>
                                {hasPermission(
                                    profile?.permissions,
                                    "data_scope:delete",
                                ) ? (
                                    <Button
                                        id={`role-scopes-remove-${toAutomationIdSegment(row.id)}`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={() => setRemoving(row.id)}
                                    >
                                        移除
                                    </Button>
                                ) : null}
                            </div>
                            {removing === row.id ? (
                                <div className="flex flex-wrap items-center gap-2 text-sm">
                                    <span>
                                        移除后，该角色关联账号的数据范围将重新计算。
                                    </span>
                                    <Button
                                        id="role-scopes-remove-confirm"
                                        type="button"
                                        size="sm"
                                        disabled={remove.isPending}
                                        onClick={async () => {
                                            setError(null)
                                            try {
                                                await remove.mutateAsync(row.id)
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
                                        id="role-scopes-remove-cancel"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={remove.isPending}
                                        onClick={() => setRemoving(null)}
                                    >
                                        取消
                                    </Button>
                                </div>
                            ) : null}
                        </li>
                    ))}
                </ul>
            )}
        </div>
    )
}
