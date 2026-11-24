"use client"

import type { ReactNode } from "react"
import {
    AlertTriangleIcon,
    ArrowUpRightIcon,
    CheckCircle2Icon,
    ChevronRightIcon,
    CircleAlertIcon,
    PencilLineIcon,
    RefreshCwIcon,
} from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type { WorkspaceRejectionInfo } from "./workspace-rejection-notice"

export type WorkspaceReworkStance = "ready" | "needs_fix" | "blocked"

export type WorkspaceReworkTriageAction = Readonly<{
    id: string
    label: string
    onClick: () => void
    icon?: "locate" | "external"
    variant?: "outline" | "ghost"
    disabled?: boolean
}>

export type WorkspaceReworkTriageItem = Readonly<{
    id: string
    tone: "destructive" | "warning" | "info"
    text: ReactNode
    action?: WorkspaceReworkTriageAction
}>

export type WorkspaceReworkCommandCardProps = Readonly<{
    idPrefix: string
    rejection: WorkspaceRejectionInfo
    stance: WorkspaceReworkStance
    headline: string
    description?: string
    items?: readonly WorkspaceReworkTriageItem[]
    evidence?: readonly ReactNode[]
    primaryAction?: Readonly<{
        id: string
        label: string
        onClick: () => void
        variant?: "default" | "outline"
        disabled?: boolean
        icon?: "edit" | "external"
    }>
    secondaryActions?: readonly WorkspaceReworkTriageAction[]
    onRefresh?: () => void
    refreshing?: boolean
    refreshLabel?: string
}>

function TriageActionIcon({
    icon,
}: {
    icon?: WorkspaceReworkTriageAction["icon"]
}) {
    if (icon === "locate") {
        return <ChevronRightIcon data-icon="inline-end" aria-hidden="true" />
    }
    if (icon === "external") {
        return <ArrowUpRightIcon data-icon="inline-end" aria-hidden="true" />
    }
    return null
}

/**
 * 统一驳回行动指挥卡（遵循设计系统 §4.4）：
 * - 上段单点聚焦展示驳回人、节点、时间与加粗驳回原话；
 * - 下段按「正常可重提 / 存在可修异常 / 前提已失效或硬阻塞」三态势分流，杜绝多张红黄框上下打架。
 */
export function WorkspaceReworkCommandCard({
    idPrefix,
    rejection,
    stance,
    headline,
    description,
    items = [],
    evidence = [],
    primaryAction,
    secondaryActions = [],
    onRefresh,
    refreshing = false,
    refreshLabel,
}: WorkspaceReworkCommandCardProps) {
    const hasDestructive =
        stance === "blocked" ||
        items.some((item) => item.tone === "destructive")

    return (
        <section
            aria-label="驳回处理指引"
            data-slot="rework-command-card"
            data-rework-stance={stance}
            className={cn(
                "overflow-hidden rounded-lg border border-border/70 border-l-2 bg-card",
                hasDestructive
                    ? "border-l-destructive"
                    : stance === "needs_fix"
                      ? "border-l-warning"
                      : "border-l-info",
            )}
        >
            {/* 上段：为什么被打回（单点聚焦） */}
            <div className="space-y-1.5 bg-destructive-soft/40 px-4 py-3">
                <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5 text-xs">
                    <span className="inline-flex items-center gap-1.5 font-semibold text-destructive-soft-foreground">
                        <CircleAlertIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0 text-destructive"
                        />
                        本次审批已驳回
                    </span>
                    {rejection.rejector ? (
                        <span className="font-medium text-foreground/85">
                            {rejection.rejector}
                        </span>
                    ) : null}
                    {rejection.node ? (
                        <span className="text-muted-foreground">
                            {rejection.node}
                        </span>
                    ) : null}
                    {rejection.rejectedAt ? (
                        <time
                            dateTime={rejection.rejectedAt.dateTime}
                            className="num text-muted-foreground"
                        >
                            {rejection.rejectedAt.label}
                        </time>
                    ) : null}
                </div>

                {rejection.reason ? (
                    <p className="text-body-compact font-semibold whitespace-pre-wrap wrap-anywhere text-foreground">
                        「{rejection.reason}」
                    </p>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        {rejection.loading
                            ? "正在读取驳回意见…"
                            : "驳回意见暂不可用，可核对右侧审批记录。"}
                    </p>
                )}

                {rejection.isError ? (
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <span role="status">驳回意见读取失败。</span>
                        <Button
                            id={`${idPrefix}-rejection-retry`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            disabled={rejection.isFetching}
                            onClick={rejection.onRetry}
                        >
                            重试读取
                        </Button>
                    </div>
                ) : null}
            </div>

            {/* 下段：前置体检结论与三态势分流 */}
            <div className="space-y-2.5 border-t border-border/60 bg-card px-4 py-3 text-xs">
                {stance === "blocked" ? (
                    <div role="alert" className="space-y-2.5">
                        <div className="flex items-start gap-2">
                            <AlertTriangleIcon
                                aria-hidden="true"
                                className="mt-0.5 size-3.5 shrink-0 text-destructive"
                            />
                            <div className="min-w-0 space-y-1">
                                <p className="font-semibold text-destructive-soft-foreground">
                                    {headline}
                                </p>
                                {description ? (
                                    <p className="text-foreground/80">
                                        {description}
                                    </p>
                                ) : null}
                            </div>
                        </div>

                        {evidence.length > 0 ? (
                            <ul className="space-y-1 rounded-md bg-surface-sunken px-3 py-2 text-foreground/85">
                                {evidence.map((entry, index) => (
                                    <li
                                        key={index}
                                        className="num flex items-baseline gap-1.5"
                                    >
                                        <span
                                            aria-hidden="true"
                                            className="text-muted-foreground"
                                        >
                                            •
                                        </span>
                                        <span className="min-w-0 wrap-anywhere">
                                            {entry}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        ) : null}

                        {primaryAction ||
                        secondaryActions.length > 0 ||
                        onRefresh ? (
                            <div className="flex flex-wrap items-center gap-2 pt-0.5">
                                {primaryAction ? (
                                    <Button
                                        id={primaryAction.id}
                                        type="button"
                                        variant={
                                            primaryAction.variant ?? "default"
                                        }
                                        size="xs"
                                        disabled={primaryAction.disabled}
                                        onClick={primaryAction.onClick}
                                    >
                                        {primaryAction.icon === "edit" ? (
                                            <PencilLineIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                        ) : null}
                                        {primaryAction.label}
                                        {primaryAction.icon === "external" ? (
                                            <ArrowUpRightIcon
                                                data-icon="inline-end"
                                                aria-hidden="true"
                                            />
                                        ) : null}
                                    </Button>
                                ) : null}
                                {secondaryActions.map((action) => (
                                    <Button
                                        key={action.id}
                                        id={action.id}
                                        type="button"
                                        variant={action.variant ?? "outline"}
                                        size="xs"
                                        disabled={action.disabled}
                                        onClick={action.onClick}
                                    >
                                        {action.label}
                                        <TriageActionIcon icon={action.icon} />
                                    </Button>
                                ))}
                                {onRefresh ? (
                                    <Button
                                        id={`${idPrefix}-card-refresh`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        disabled={refreshing}
                                        onClick={onRefresh}
                                    >
                                        <RefreshCwIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                            className={cn(
                                                refreshing && "animate-spin",
                                            )}
                                        />
                                        {refreshLabel ?? "刷新校验"}
                                    </Button>
                                ) : null}
                            </div>
                        ) : null}
                    </div>
                ) : stance === "needs_fix" ? (
                    <div className="space-y-2">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <p className="font-medium text-foreground">
                                {headline}
                            </p>
                            {onRefresh ? (
                                <Button
                                    id={`${idPrefix}-triage-refresh`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    disabled={refreshing}
                                    onClick={onRefresh}
                                >
                                    <RefreshCwIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                        className={cn(
                                            refreshing && "animate-spin",
                                        )}
                                    />
                                    {refreshLabel ?? "刷新校验"}
                                </Button>
                            ) : null}
                        </div>

                        {items.length > 0 ? (
                            <ul className="space-y-1.5">
                                {items.map((entry) => (
                                    <li
                                        key={entry.id}
                                        className={cn(
                                            "flex flex-wrap items-center justify-between gap-2 rounded-md px-3 py-2",
                                            entry.tone === "destructive"
                                                ? "bg-destructive-soft/35 text-destructive-soft-foreground"
                                                : entry.tone === "warning"
                                                  ? "bg-warning-soft/50 text-warning-soft-foreground"
                                                  : "bg-surface-sunken text-foreground/85",
                                        )}
                                    >
                                        <span className="flex min-w-0 items-start gap-1.5 font-medium">
                                            {entry.tone === "destructive" ? (
                                                <AlertTriangleIcon
                                                    aria-hidden="true"
                                                    className="mt-0.5 size-3.5 shrink-0 text-destructive"
                                                />
                                            ) : (
                                                <CircleAlertIcon
                                                    aria-hidden="true"
                                                    className={cn(
                                                        "mt-0.5 size-3.5 shrink-0",
                                                        entry.tone === "info" &&
                                                            "text-muted-foreground",
                                                    )}
                                                />
                                            )}
                                            <span className="num min-w-0 wrap-anywhere">
                                                {entry.text}
                                            </span>
                                        </span>
                                        {entry.action ? (
                                            <Button
                                                id={entry.action.id}
                                                type="button"
                                                variant={
                                                    entry.action.variant ??
                                                    "outline"
                                                }
                                                size="xs"
                                                disabled={entry.action.disabled}
                                                onClick={entry.action.onClick}
                                            >
                                                {entry.action.label}
                                                <TriageActionIcon
                                                    icon={entry.action.icon}
                                                />
                                            </Button>
                                        ) : null}
                                    </li>
                                ))}
                            </ul>
                        ) : null}

                        {description ? (
                            <p className="text-muted-foreground">
                                {description}
                            </p>
                        ) : null}
                    </div>
                ) : (
                    <div className="space-y-2">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <p className="font-medium text-foreground">
                                {headline}
                            </p>
                            {onRefresh ? (
                                <Button
                                    id={`${idPrefix}-triage-refresh`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    disabled={refreshing}
                                    onClick={onRefresh}
                                >
                                    <RefreshCwIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                        className={cn(
                                            refreshing && "animate-spin",
                                        )}
                                    />
                                    {refreshLabel ?? "刷新校验"}
                                </Button>
                            ) : null}
                        </div>
                        {description ? (
                            <div className="flex items-start gap-1.5 text-muted-foreground">
                                <CheckCircle2Icon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0 text-success-foreground"
                                />
                                <p className="min-w-0 wrap-anywhere">
                                    {description}
                                </p>
                            </div>
                        ) : null}
                    </div>
                )}
            </div>
        </section>
    )
}
