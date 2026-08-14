"use client"

import {
    BusinessEmptyState,
    BusinessFailureState,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useEffectiveAccessQuery } from "@/features/access-audit/hooks/queries"
import { formatDateTime } from "@/lib/datetime"

type EffectiveAccessBodyProps = {
    query: ReturnType<typeof useEffectiveAccessQuery>
}

function EffectiveAccessBody({ query }: EffectiveAccessBodyProps) {
    if (query.isPending) {
        return <div className="h-40 animate-pulse rounded-lg bg-muted" />
    }
    if (query.isError) {
        return (
            <BusinessFailureState
                error={query.error}
                action={
                    <Button
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
    if (!query.data) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="主体不存在或无权解释"
                description="仅解释当前用户有权管理的主体。"
            />
        )
    }
    return (
        <div className="flex flex-col gap-4">
            <div className="flex flex-wrap items-center gap-2">
                <Badge variant="secondary">
                    {query.data.subject.type === "ROLE" ? "角色" : "用户"}
                </Badge>
                <span className="font-medium">
                    {query.data.subject.label}
                </span>
                <Badge variant="outline">
                    版本 v
                    {query.data.permissionVersion.split("-").at(-1)}
                </Badge>
                <span className="text-xs text-muted-foreground">
                    计算于 {formatDateTime(query.data.calculatedAt, "full")}
                </span>
            </div>

            <section className="space-y-2">
                <h3 className="text-sm font-semibold">
                    模块与动作权限来源
                </h3>
                {query.data.moduleAndActionGrants.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        无有效模块授权
                    </p>
                ) : (
                    query.data.moduleAndActionGrants.map((g) => (
                        <div
                            key={g.id}
                            className="rounded-lg border border-border p-3 text-sm"
                        >
                            <div className="font-medium">{g.targetLabel}</div>
                            <div className="text-muted-foreground">
                                {g.capability} · 来源 {g.sourceLabel}（
                                {g.sourceType}）
                            </div>
                        </div>
                    ))
                )}
            </section>

            <section className="space-y-2">
                <h3 className="text-sm font-semibold">数据范围来源</h3>
                {query.data.dataScopes.map((g) => (
                    <div
                        key={g.id}
                        className="rounded-lg border border-border p-3 text-sm"
                    >
                        <div className="font-medium">{g.targetLabel}</div>
                        <div className="text-muted-foreground">
                            来源 {g.sourceLabel}
                        </div>
                    </div>
                ))}
            </section>

            <section className="space-y-2">
                <h3 className="text-sm font-semibold">字段策略来源</h3>
                {query.data.fieldPolicies.map((g) => (
                    <div
                        key={g.id}
                        className="rounded-lg border border-border p-3 text-sm"
                    >
                        <div className="font-medium">{g.targetLabel}</div>
                        <div className="text-muted-foreground">
                            {g.capability} · {g.sourceLabel}
                        </div>
                    </div>
                ))}
            </section>

            <section className="space-y-2">
                <h3 className="text-sm font-semibold">历史参与者</h3>
                {query.data.historicalParticipantRules.map((e) => (
                    <div
                        key={e.id}
                        className="rounded-lg border border-border p-3 text-sm"
                    >
                        <div className="font-medium">{e.sourceLabel}</div>
                        <div className="text-muted-foreground">{e.message}</div>
                    </div>
                ))}
            </section>

            <section className="space-y-2">
                <h3 className="text-sm font-semibold">
                    拒绝 / 阻塞（含对象状态，不混淆为配置缺失）
                </h3>
                {query.data.deniedOrBlocked.map((e) => (
                    <div
                        key={e.id}
                        className="rounded-lg border border-warning/40 bg-warning/5 p-3 text-sm"
                    >
                        <div className="flex flex-wrap items-center gap-2">
                            <Badge variant="warning">{e.layerLabel}</Badge>
                            <span className="font-mono text-xs">{e.code}</span>
                        </div>
                        <p className="mt-1 text-muted-foreground">
                            {e.message}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                            来源 {e.sourceLabel}（{e.sourceType}）
                        </p>
                    </div>
                ))}
            </section>

            {query.data.actionBlockers.length > 0 ? (
                <section className="space-y-2">
                    <h3 className="text-sm font-semibold">
                        当前被阻断的操作
                    </h3>
                    {query.data.actionBlockers.map((b) => (
                        <Alert
                            key={`${b.action}-${b.code}`}
                            variant="warning"
                        >
                            <AlertTitle>{b.message}</AlertTitle>
                            <AlertDescription>{b.message}</AlertDescription>
                        </Alert>
                    ))}
                </section>
            ) : null}
        </div>
    )
}

export { EffectiveAccessBody }
