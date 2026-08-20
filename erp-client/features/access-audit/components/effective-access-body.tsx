"use client"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { useEffectiveAccessQuery } from "@/features/access-audit/hooks/queries"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

type EffectiveAccessBodyProps = {
    query: ReturnType<typeof useEffectiveAccessQuery>
}

type GrantRow = {
    id: string
    title: string
    detail: string
}

function GrantList({
    items,
    empty,
    tone = "default",
}: {
    items: readonly GrantRow[]
    empty?: string
    tone?: "default" | "warning"
}) {
    if (items.length === 0) {
        return empty ? (
            <p className="text-sm text-muted-foreground">{empty}</p>
        ) : null
    }
    return (
        <ul
            className={cn(
                "divide-y overflow-hidden rounded-lg border",
                tone === "warning"
                    ? "divide-warning/20 border-warning/40 bg-warning/5"
                    : "divide-grid border-border",
            )}
        >
            {items.map((item) => (
                <li key={item.id} className="px-3 py-2.5">
                    <div className="text-sm font-medium">{item.title}</div>
                    <div className="text-xs text-muted-foreground">
                        {item.detail}
                    </div>
                </li>
            ))}
        </ul>
    )
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
        <div className="flex flex-col gap-5">
            <DescriptionList columns="two" aria-label="主体摘要">
                <DescriptionItem>
                    <DescriptionTerm>主体</DescriptionTerm>
                    <DescriptionDetails>
                        <span className="flex flex-wrap items-center gap-2">
                            <Badge variant="secondary">
                                {query.data.subject.type === "ROLE"
                                    ? "角色"
                                    : "用户"}
                            </Badge>
                            {query.data.subject.label}
                        </span>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>权限版本</DescriptionTerm>
                    <DescriptionDetails>
                        <span className="num">
                            v{query.data.permissionVersion.split("-").at(-1)}
                        </span>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem className="sm:col-span-2">
                    <DescriptionTerm>计算时间</DescriptionTerm>
                    <DescriptionDetails>
                        {formatDateTime(query.data.calculatedAt, "full")}
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>

            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">模块与动作权限来源</h3>
                <GrantList
                    empty="无有效模块授权"
                    items={query.data.moduleAndActionGrants.map((g) => ({
                        id: g.id,
                        title: g.targetLabel,
                        detail: `${g.capability} · 来源 ${g.sourceLabel}（${g.sourceType}）`,
                    }))}
                />
            </section>

            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">数据范围来源</h3>
                <GrantList
                    empty="无数据范围记录"
                    items={query.data.dataScopes.map((g) => ({
                        id: g.id,
                        title: g.targetLabel,
                        detail: `来源 ${g.sourceLabel}`,
                    }))}
                />
            </section>

            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">字段策略来源</h3>
                <GrantList
                    empty="无字段策略记录"
                    items={query.data.fieldPolicies.map((g) => ({
                        id: g.id,
                        title: g.targetLabel,
                        detail: `${g.capability} · ${g.sourceLabel}`,
                    }))}
                />
            </section>

            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">历史参与者</h3>
                <GrantList
                    empty="无历史参与者规则"
                    items={query.data.historicalParticipantRules.map((e) => ({
                        id: e.id,
                        title: e.sourceLabel,
                        detail: e.message,
                    }))}
                />
            </section>

            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">
                    拒绝 / 阻塞（含对象状态，不混淆为配置缺失）
                </h3>
                {query.data.deniedOrBlocked.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        当前无拒绝或阻塞项
                    </p>
                ) : (
                    <ul className="divide-y divide-warning/20 overflow-hidden rounded-lg border border-warning/40 bg-warning/5">
                        {query.data.deniedOrBlocked.map((e) => (
                            <li
                                key={e.id}
                                className="flex flex-col gap-1 px-3 py-2.5"
                            >
                                <div className="flex flex-wrap items-center gap-2">
                                    <Badge variant="warning">
                                        {e.layerLabel}
                                    </Badge>
                                    <span className="font-mono text-xs">
                                        {e.code}
                                    </span>
                                </div>
                                <p className="text-sm text-muted-foreground">
                                    {e.message}
                                </p>
                                <p className="text-xs text-muted-foreground">
                                    来源 {e.sourceLabel}（{e.sourceType}）
                                </p>
                            </li>
                        ))}
                    </ul>
                )}
            </section>

            {query.data.actionBlockers.length > 0 ? (
                <section className="flex flex-col gap-2">
                    <h3 className="text-sm font-medium">当前被阻断的操作</h3>
                    {query.data.actionBlockers.map((b) => (
                        <Alert key={`${b.action}-${b.code}`} variant="warning">
                            <AlertTitle>{b.message}</AlertTitle>
                            <AlertDescription>{b.code}</AlertDescription>
                        </Alert>
                    ))}
                </section>
            ) : null}
        </div>
    )
}

export { EffectiveAccessBody }
