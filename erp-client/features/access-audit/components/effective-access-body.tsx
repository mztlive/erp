"use client"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ScopeRulesView } from "@/features/organization/components/scope-rules-view"
import { useEffectiveAccessQuery } from "@/features/access-audit/hooks/queries"
import { permissionPreview } from "@/features/access-audit/lib/effective-access-preview"
import type { RoleRow } from "@/features/access-audit/types"

type EffectiveAccessBodyProps = {
    query: ReturnType<typeof useEffectiveAccessQuery>
    previewRole?: RoleRow | null
}

function PermissionLead({
    preview,
}: {
    preview: ReturnType<typeof permissionPreview>
}) {
    return (
        <section className="border-b border-border pb-6">
            <h3 className="text-xs font-medium text-muted-foreground">
                操作权限
            </h3>
            <p className="mt-2 text-[32px] font-semibold leading-10 tracking-tight">
                {preview.allPermissions ? (
                    "全部操作权限"
                ) : (
                    <>
                        <span className="num">{preview.count}</span>
                        <span className="ml-2 text-sm font-normal text-muted-foreground">
                            项
                        </span>
                    </>
                )}
            </p>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
                {preview.allPermissions
                    ? "可进入全部模块。实际能看哪些单据仍受数据范围限制。"
                    : preview.groups.length > 0
                      ? `覆盖 ${preview.groups
                            .slice(0, 3)
                            .map((group) => group.name)
                            .join("、")}${
                            preview.groups.length > 3
                                ? ` 等 ${preview.groups.length} 个模块`
                                : ""
                        }。具体动作在角色资料中调整。`
                      : "尚未配置操作权限。"}
            </p>
        </section>
    )
}

function DataScopeSection({
    query,
    subjectLabel,
}: {
    query: EffectiveAccessBodyProps["query"]
    subjectLabel: string
}) {
    if (query.isPending) {
        return (
            <section className="space-y-3">
                <h3 className="font-medium">数据范围</h3>
                <p role="status" className="text-xs text-muted-foreground">
                    正在读取数据范围…
                </p>
            </section>
        )
    }
    if (query.isError) {
        return (
            <section className="space-y-3">
                <h3 className="font-medium">数据范围</h3>
                <BusinessFailureState
                    error={query.error}
                    action={
                        <Button
                            id="operations-access-effective-access-retry"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </section>
        )
    }
    const groups = query.data?.dataScopes ?? []
    return (
        <section className="space-y-3">
            <h3 className="font-medium">数据范围</h3>
            <p className="text-xs leading-5 text-muted-foreground">
                在已有操作权限的前提下，限制能看到哪些单据。
            </p>
            {groups.length === 0 ? (
                <p className="text-xs leading-5 text-muted-foreground">
                    尚未配置数据范围，不代表可访问全部数据。
                </p>
            ) : (
                <ScopeRulesView
                    rules={groups.map((row) => ({
                        id: row.id,
                        subject_type:
                            row.sourceType === "USER" ? "user" : "role",
                        subject_id: row.sourceLabel,
                        resource: row.resource ?? "",
                        actions: row.actions ? [...row.actions] : undefined,
                        scope_type: row.scopeType ?? "",
                        scope_targets: [...(row.scopeTargets ?? [])],
                        target_dimension: row.targetDimension,
                        target_mode: row.targetMode,
                        include_descendants: row.includeDescendants,
                        enabled: row.enabled,
                    }))}
                    roles={groups.map((row) => ({
                        id: row.sourceLabel,
                        name: row.sourceLabel || subjectLabel,
                    }))}
                />
            )}
        </section>
    )
}

function EffectiveAccessBody({ query, previewRole }: EffectiveAccessBodyProps) {
    if (!previewRole && query.isPending) {
        return (
            <p role="status" className="text-sm text-muted-foreground">
                正在读取角色权限…
            </p>
        )
    }
    if (!previewRole && query.isError) {
        return (
            <BusinessFailureState
                error={query.error}
                action={
                    <Button
                        id="operations-access-effective-access-retry"
                        type="button"
                        size="sm"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        )
    }
    if (!previewRole && !query.data) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="主体不存在或无权查看"
                description="仅展示当前账号有权管理的角色。"
            />
        )
    }

    const preview = permissionPreview({
        previewRole,
        grants: query.data?.moduleAndActionGrants ?? [],
    })
    const subjectLabel = previewRole?.name ?? query.data?.subject.label ?? ""
    const denied = query.data?.deniedOrBlocked ?? []
    const blockers = query.data?.actionBlockers ?? []

    return (
        <div className="space-y-6 text-sm">
            <PermissionLead preview={preview} />
            <DataScopeSection query={query} subjectLabel={subjectLabel} />

            {denied.length > 0 || blockers.length > 0 ? (
                <section className="space-y-3 border-t border-border pt-6">
                    <h3 className="font-medium">当前限制</h3>
                    {denied.map((item) => (
                        <div key={item.id} className="space-y-1">
                            <div className="flex flex-wrap items-center gap-2">
                                <Badge variant="warning">
                                    {item.layerLabel}
                                </Badge>
                            </div>
                            <p className="text-xs leading-5 text-muted-foreground">
                                {item.message}
                            </p>
                        </div>
                    ))}
                    {blockers.map((blocker) => (
                        <Alert
                            key={`${blocker.action}-${blocker.code}`}
                            variant="warning"
                        >
                            <AlertTitle>{blocker.message}</AlertTitle>
                            <AlertDescription>{blocker.code}</AlertDescription>
                        </Alert>
                    ))}
                </section>
            ) : null}

            {query.data ? (
                <section className="border-t border-border pt-6">
                    <h3 className="text-xs font-medium text-muted-foreground">
                        查询范围
                    </h3>
                    <p className="mt-2 text-xs leading-5 text-muted-foreground">
                        以上为当前角色的操作权限与数据范围配置。具体业务操作是否允许，以执行时的权限校验为准。
                    </p>
                </section>
            ) : null}
        </div>
    )
}

export { EffectiveAccessBody }
