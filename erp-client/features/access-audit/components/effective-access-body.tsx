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
import {
    GROUP_NAME_BY_CODE,
    PERMISSION_CATALOG,
} from "@/features/admin/lib/permission-catalog"
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
    /** 权限编码等技术标识：次要展示，供排查时对照。 */
    code?: string
}

/** 授权来源主体类型文案。 */
function sourceTypeLabel(sourceType: string): string {
    if (sourceType === "ROLE") return "角色"
    if (sourceType === "USER") return "用户"
    return sourceType
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
                <li
                    key={item.id}
                    className="flex items-baseline justify-between gap-3 px-3 py-2.5"
                >
                    <div className="min-w-0">
                        <div className="text-sm font-medium">{item.title}</div>
                        <div className="text-xs text-muted-foreground">
                            {item.detail}
                        </div>
                    </div>
                    {item.code ? (
                        <span className="shrink-0 font-mono text-xs text-muted-foreground">
                            {item.code}
                        </span>
                    ) : null}
                </li>
            ))}
        </ul>
    )
}

/** 权限来源按模块分组，便于逐模块核对，而不是读一长串条目。 */
function groupGrants(
    grants: readonly {
        id: string
        targetLabel: string
        capability: string
        sourceType: string
        sourceLabel: string
    }[],
): readonly { name: string; items: GrantRow[] }[] {
    const sections = new Map<string, GrantRow[]>()
    for (const grant of grants) {
        const name = GROUP_NAME_BY_CODE.get(grant.capability) ?? "其它"
        const row: GrantRow = {
            id: grant.id,
            title: grant.targetLabel,
            detail: `允许 · 来源${sourceTypeLabel(grant.sourceType)} ${grant.sourceLabel}`,
            code: grant.capability,
        }
        const bucket = sections.get(name)
        if (bucket) bucket.push(row)
        else sections.set(name, [row])
    }
    // 按权限目录顺序输出，目录外的编码（通配等）归到末尾的「其它」
    const order = new Map(
        PERMISSION_CATALOG.map((group, index) => [group.name, index] as const),
    )
    return [...sections.entries()]
        .map(([name, items]) => ({ name, items }))
        .sort(
            (a, b) =>
                (order.get(a.name) ?? Number.MAX_SAFE_INTEGER) -
                (order.get(b.name) ?? Number.MAX_SAFE_INTEGER),
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
                    <DescriptionTerm>计算时间</DescriptionTerm>
                    <DescriptionDetails>
                        {formatDateTime(query.data.calculatedAt, "full")}
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>

            <section className="flex flex-col gap-3">
                <h3 className="text-sm font-medium">
                    模块与动作权限来源
                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                        共{" "}
                        <span className="num">
                            {query.data.moduleAndActionGrants.length}
                        </span>{" "}
                        项
                    </span>
                </h3>
                {query.data.moduleAndActionGrants.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        无有效模块授权
                    </p>
                ) : (
                    groupGrants(query.data.moduleAndActionGrants).map(
                        (section) => (
                            <div
                                key={section.name}
                                className="flex flex-col gap-1.5"
                            >
                                <div className="flex items-baseline gap-2 text-xs text-muted-foreground">
                                    <span className="font-medium text-foreground">
                                        {section.name}
                                    </span>
                                    <span className="num">
                                        {section.items.length}
                                    </span>
                                </div>
                                <GrantList items={section.items} />
                            </div>
                        ),
                    )
                )}
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
                                    来源{sourceTypeLabel(e.sourceType)}{" "}
                                    {e.sourceLabel}
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
