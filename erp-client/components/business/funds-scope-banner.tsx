"use client"

import { ChevronDownIcon, ShieldAlertIcon, ShieldCheckIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverHeader,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { scopeText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import { MoneyValue } from "./values"

type FundsScopeBannerProps = {
    scopeSummary: string | undefined
    asOf: string | undefined
    permissionLimited: boolean
    unassigned?: string | null
    className?: string
}

/** 范围行列表共用的紧凑通栏口径条：范围口径、统计时点与受限标识单行呈现。 */
export function FundsScopeBanner({
    scopeSummary,
    asOf,
    permissionLimited,
    unassigned,
    className,
}: FundsScopeBannerProps) {
    if (!scopeSummary && !permissionLimited) return null
    const limitDetail = [
        scopeText.limitedOnlyVisibleShare,
        unassigned != null && unassigned !== ""
            ? `${scopeText.unassignedShare} ${unassigned}`
            : null,
    ]
        .filter(Boolean)
        .join("；")

    return (
        <div
            role={permissionLimited ? "status" : undefined}
            className={cn(
                "-mx-(--scaffold-px) flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-1.5 border-b border-border/60 bg-muted/25 px-(--scaffold-px) py-2 text-xs text-muted-foreground",
                className,
            )}
        >
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                {permissionLimited ? (
                    <ShieldAlertIcon
                        className="size-3.5 shrink-0 text-warning"
                        aria-hidden="true"
                    />
                ) : (
                    <ShieldCheckIcon
                        className="size-3.5 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                    />
                )}
                <span className="min-w-0 break-words text-foreground/90">
                    {scopeSummary ??
                        (permissionLimited
                            ? "仅显示本人权限内金额"
                            : "当前可查看范围")}
                </span>
                {asOf ? (
                    <span className="text-muted-foreground">
                        · 统计截至 <span className="num">{asOf}</span>
                    </span>
                ) : null}
            </div>
            {permissionLimited ? (
                <Badge
                    variant="warning"
                    title={limitDetail}
                    className="shrink-0"
                >
                    部分受限 · 仅显示本人权限内金额
                </Badge>
            ) : null}
        </div>
    )
}

export type FundsScopeOwnerShare = Readonly<{
    id: string
    name: string
    amount: string
}>

type FundsScopeSummaryBarProps = {
    wholeTotal: string | null
    unassigned: string | null
    groups: readonly FundsScopeOwnerShare[]
    popoverIdPrefix: string
    maxInline?: number
    className?: string
}

/** 范围汇总统一无边框指标带：整单合计、未分配份额与按负责人分布（超限进 Popover）。 */
export function FundsScopeSummaryBar({
    wholeTotal,
    unassigned,
    groups,
    popoverIdPrefix,
    maxInline = 4,
    className,
}: FundsScopeSummaryBarProps) {
    const inlineGroups = groups.slice(0, maxInline)
    const overflowCount = Math.max(0, groups.length - inlineGroups.length)

    return (
        <section
            aria-label="范围金额汇总"
            className={cn(
                "-mx-(--scaffold-px) border-b border-border/60 bg-card px-(--scaffold-px) py-3",
                className,
            )}
        >
            <dl className="grid min-w-0 gap-x-8 gap-y-3 text-body-compact sm:grid-cols-2 lg:grid-cols-[auto_auto_minmax(0,1fr)]">
                <div className="min-w-0 space-y-0.5">
                    <dt className="text-xs text-muted-foreground">整单合计</dt>
                    <dd>
                        <MoneyValue
                            value={wholeTotal}
                            size="compact"
                            display="exact"
                            unavailableReason={
                                wholeTotal == null
                                    ? scopeText.wholeRestricted
                                    : undefined
                            }
                        />
                    </dd>
                </div>
                <div className="min-w-0 space-y-0.5 lg:border-l lg:border-border/60 lg:pl-8">
                    <dt className="text-xs text-muted-foreground">
                        {scopeText.unassignedShare}
                    </dt>
                    <dd>
                        <MoneyValue
                            value={unassigned}
                            size="compact"
                            display="exact"
                        />
                    </dd>
                </div>
                <div className="min-w-0 space-y-1 sm:col-span-2 lg:col-span-1 lg:border-l lg:border-border/60 lg:pl-8">
                    <dt className="text-xs text-muted-foreground">
                        按负责人分布（
                        <span className="num">{groups.length}</span>）
                    </dt>
                    <dd className="flex flex-wrap items-center gap-x-4 gap-y-1">
                        {inlineGroups.length === 0 ? (
                            <span className="text-xs text-muted-foreground">
                                暂无匹配份额
                            </span>
                        ) : (
                            inlineGroups.map((share) => (
                                <span
                                    key={share.id}
                                    className="inline-flex min-w-0 items-baseline gap-1.5 text-xs"
                                >
                                    <span className="max-w-28 truncate text-muted-foreground">
                                        {share.name}
                                    </span>
                                    <MoneyValue
                                        value={share.amount}
                                        display="exact"
                                        className="font-medium text-foreground"
                                    />
                                </span>
                            ))
                        )}
                        {overflowCount > 0 ? (
                            <Popover>
                                <PopoverTrigger
                                    render={
                                        <Button
                                            id={`${popoverIdPrefix}-more-owners`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className="h-6 gap-0.5 px-1.5 text-xs text-primary"
                                        />
                                    }
                                >
                                    +{overflowCount} 人
                                    <ChevronDownIcon
                                        className="size-3"
                                        aria-hidden="true"
                                    />
                                </PopoverTrigger>
                                <PopoverContent align="end" className="w-80">
                                    <PopoverHeader>
                                        <PopoverTitle>
                                            全部负责人份额（{groups.length}）
                                        </PopoverTitle>
                                    </PopoverHeader>
                                    <ul className="max-h-64 divide-y divide-border/60 overflow-y-auto text-xs">
                                        {groups.map((share) => (
                                            <li
                                                key={share.id}
                                                className="flex items-baseline justify-between gap-4 py-1.5"
                                            >
                                                <span className="min-w-0 break-words text-muted-foreground">
                                                    {share.name}
                                                </span>
                                                <MoneyValue
                                                    value={share.amount}
                                                    display="exact"
                                                    className="shrink-0 font-medium text-foreground"
                                                />
                                            </li>
                                        ))}
                                    </ul>
                                </PopoverContent>
                            </Popover>
                        ) : null}
                    </dd>
                </div>
            </dl>
        </section>
    )
}

type LimitedBadgeProps = {
    id: string
    className?: string
}

/**
 * 范围列表表格的标题和说明。
 * 标题固定为「按数据范围查询」加条数，说明为范围摘要；查询未返回时说明为「加载中…」。
 */
export function scopeQueryFrameHeading(
    total: number | undefined,
    scopeSummary: string | undefined,
) {
    return {
        title: (
            <span className="inline-flex min-w-0 flex-wrap items-baseline gap-2">
                往来账款汇总列表
                <span
                    aria-live="polite"
                    className="font-normal text-muted-foreground"
                >
                    {(total ?? 0).toLocaleString("zh-CN")} 条
                </span>
            </span>
        ),
        description: (
            <span aria-live="polite" className="break-words">
                {scopeSummary ?? "加载中…"}
            </span>
        ),
    }
}

/** 部分受限行的统一标记；金额统一用 MoneyValue 空值展示，不写零。 */
export function LimitedBadge({ id, className }: LimitedBadgeProps) {
    return (
        <Badge
            id={id}
            variant="warning"
            title={scopeText.limitedOnlyVisibleShare}
            className={cn("shrink-0", className)}
        >
            部分受限
        </Badge>
    )
}
