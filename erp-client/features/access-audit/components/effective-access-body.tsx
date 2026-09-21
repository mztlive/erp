"use client"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useEffectiveAccessQuery } from "@/features/access-audit/hooks/queries"
import {
    groupDataScopes,
    permissionPreview,
    type DataScopePreviewGroup,
} from "@/features/access-audit/lib/effective-access-preview"
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

function ScopeResources({ group }: { group: DataScopePreviewGroup }) {
    if (group.resources.length === 0) return null
    if (group.scopeType === "company" && group.resources.length > 8) {
        return (
            <p className="text-[13px] leading-6">
                适用于已接入的业务对象（
                <strong className="num text-foreground">
                    {group.resources.length}
                </strong>{" "}
                类）。
            </p>
        )
    }
    return (
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-[13px] leading-6">
            {group.resources.map((resource) => (
                <span key={resource}>{resource}</span>
            ))}
        </div>
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
    const groups = groupDataScopes(query.data?.dataScopes ?? [])
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
                <div className="space-y-5">
                    {groups.map((group) => {
                        const showSource =
                            group.sources.length > 0 &&
                            (group.sources.length > 1 ||
                                group.sources[0] !== subjectLabel)
                        return (
                            <div key={group.scopeType} className="space-y-1.5">
                                <p className="font-medium">{group.label}</p>
                                <p className="text-xs leading-5 text-muted-foreground">
                                    {group.explanation}
                                </p>
                                <ScopeResources group={group} />
                                {group.specifiedTargetCount > 0 ? (
                                    <p className="text-xs leading-5 text-muted-foreground">
                                        已指定{" "}
                                        <strong className="num text-foreground">
                                            {group.specifiedTargetCount}
                                        </strong>{" "}
                                        个组织或团队
                                    </p>
                                ) : null}
                                {showSource ? (
                                    <p className="text-xs leading-5 text-muted-foreground">
                                        来自{group.sources.join("、")}
                                    </p>
                                ) : null}
                            </div>
                        )
                    })}
                </div>
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
