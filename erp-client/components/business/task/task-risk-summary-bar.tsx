"use client"

import * as React from "react"
import { ChevronRightIcon, TriangleAlertIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type TaskRiskSeverity = "destructive" | "warning"

export type TaskRiskSummaryItem = Readonly<{
    id: string
    /** 分类风险文案，如「负毛利」「税后倒挂」「需垫资」「缺少联行号」。 */
    label: React.ReactNode
    /** 可选分类计数；当显式传入且 `<= 0` 时自动过滤，不占空间。 */
    count?: number
    /** 单项严重级；任一项为 `destructive` 时自动将整条升级为高危态。 */
    severity?: TaskRiskSeverity
    /** 可选行内直达定位回调（适用于每条风险各自带跳转目标的财务审批场景）。 */
    onClick?: () => void
    /** 可点击项的稳定原生 id；省略且提供 `idPrefix` 时自动派生。 */
    buttonId?: string
}>

export type TaskRiskSummaryAction = Readonly<{
    id: string
    /** 动作按钮文案，如「仅看风险项」「采销对照」「定位超额说明」。 */
    label: React.ReactNode
    /** 可选动作数量，自动追加中文括号计数「（N）」。 */
    count?: number
    onClick: () => void
    /** 动作按钮稳定原生 id；省略且提供 `idPrefix` 时自动派生。 */
    buttonId?: string
    disabled?: boolean
    /** 是否在右侧渲染微箭头 `›`，默认 `true`。 */
    showChevron?: boolean
}>

export interface TaskRiskSummaryBarProps extends Omit<
    React.ComponentProps<"div">,
    "children" | "title"
> {
    /** 风险总数；为 `0` 时强制零渲染（返回 `null`）。 */
    riskCount?: number
    /** 严重级；省略时按 `items` 中是否存在 `destructive` 自动推导，默认 `warning`。 */
    severity?: TaskRiskSeverity
    /** 左侧引导前缀，默认高危为「高危复核」、警示为「需复核」；传 `null` 可隐藏。 */
    leadLabel?: React.ReactNode | null
    /** 分类风险汇总项（自动过滤空值与 `count <= 0` 项）。 */
    items?: readonly (TaskRiskSummaryItem | null | undefined | false)[]
    /** 自定义单行摘要内容（可单独使用或与 `items` 并存）。 */
    summary?: React.ReactNode
    /** 鼠标悬停完整风险原因提示（对应原生 `title` 属性）。 */
    tooltipTitle?: string
    /** 右侧内嵌直达动作按钮（如「仅看风险项（N）›」「采销对照 ›」）。 */
    actions?: readonly (TaskRiskSummaryAction | null | undefined | false)[]
    /** 自动化 DOM id 前缀，用于派生动作按钮 id。 */
    idPrefix?: string
    /** 自定义 `data-slot`，默认 `"task-risk-summary-bar"`。 */
    dataSlot?: string
}

const severityContainerClass: Record<TaskRiskSeverity, string> = {
    destructive:
        "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground",
    warning: "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
}

const severityActionButtonClass: Record<TaskRiskSeverity, string> = {
    destructive:
        "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground",
    warning:
        "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
}

/**
 * 任务页模式 A1 可操作风险聚合跳转条（docs/task-page-design-system.md §4.2）：
 * - `riskCount === 0` 或无有效风险项时零渲染（占 0px）
 * - `border-l-2` 区分严重级（`destructive`）与警示级（`warning`）
 * - 左侧单行汇总分类风险计数，右侧内嵌直达定位/过滤动作按钮
 */
export function TaskRiskSummaryBar({
    riskCount,
    severity,
    leadLabel,
    items,
    summary,
    tooltipTitle,
    actions,
    idPrefix,
    dataSlot = "task-risk-summary-bar",
    className,
    ...props
}: TaskRiskSummaryBarProps) {
    const visibleItems = (items ?? []).filter(
        (item): item is TaskRiskSummaryItem =>
            item != null &&
            item !== false &&
            (item.count === undefined || item.count > 0),
    )
    const visibleActions = (actions ?? []).filter(
        (action): action is TaskRiskSummaryAction => Boolean(action),
    )
    const hasSummary =
        summary !== undefined &&
        summary !== null &&
        summary !== false &&
        summary !== ""

    if (riskCount !== undefined && riskCount <= 0) {
        return null
    }
    if (riskCount === undefined && visibleItems.length === 0 && !hasSummary) {
        return null
    }

    const resolvedSeverity: TaskRiskSeverity =
        severity ??
        (visibleItems.some((item) => item.severity === "destructive")
            ? "destructive"
            : "warning")

    const resolvedLead =
        leadLabel === undefined
            ? resolvedSeverity === "destructive"
                ? "高危复核"
                : "需复核"
            : leadLabel

    const staticItems = visibleItems.filter((item) => !item.onClick)
    const interactiveItems = visibleItems.filter((item) =>
        Boolean(item.onClick),
    )
    const fallbackSummary =
        !hasSummary &&
        staticItems.length === 0 &&
        interactiveItems.length === 0 &&
        riskCount !== undefined &&
        riskCount > 0
            ? `发现 ${riskCount} 项风险`
            : null
    const effectiveHasSummary = hasSummary || fallbackSummary !== null
    const effectiveSummary = hasSummary ? summary : fallbackSummary

    return (
        <div
            role="alert"
            data-slot={dataSlot}
            data-severity={resolvedSeverity}
            title={tooltipTitle}
            className={cn(
                "flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-md border-l-2 px-3 py-1.5 text-xs",
                severityContainerClass[resolvedSeverity],
                className,
            )}
            {...props}
        >
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-0.5">
                <div className="flex min-w-0 items-start gap-2">
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0"
                    />
                    <p className="num min-w-0 wrap-anywhere">
                        {resolvedLead != null && resolvedLead !== false ? (
                            <span className="font-medium">{resolvedLead}</span>
                        ) : null}
                        {resolvedLead != null &&
                        resolvedLead !== false &&
                        (staticItems.length > 0 || effectiveHasSummary) ? (
                            <span
                                aria-hidden="true"
                                className="mx-1.5 opacity-60"
                            >
                                ·
                            </span>
                        ) : null}
                        {staticItems.map((item, index) => (
                            <React.Fragment key={item.id}>
                                {index > 0 ? (
                                    <span
                                        aria-hidden="true"
                                        className="mx-1.5 opacity-60"
                                    >
                                        ·
                                    </span>
                                ) : null}
                                <span>
                                    {item.label}
                                    {item.count !== undefined
                                        ? ` ${item.count}`
                                        : null}
                                </span>
                            </React.Fragment>
                        ))}
                        {staticItems.length > 0 && effectiveHasSummary ? (
                            <span
                                aria-hidden="true"
                                className="mx-1.5 opacity-60"
                            >
                                ·
                            </span>
                        ) : null}
                        {effectiveHasSummary ? (
                            <span>{effectiveSummary}</span>
                        ) : null}
                    </p>
                </div>

                {interactiveItems.length > 0 ? (
                    <ul className="flex flex-wrap items-center gap-x-1 gap-y-0.5">
                        {interactiveItems.map((item) => (
                            <li key={item.id}>
                                <Button
                                    id={
                                        item.buttonId ??
                                        (idPrefix
                                            ? `${idPrefix}-risk-item-${toAutomationIdSegment(item.id)}`
                                            : undefined)
                                    }
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className={cn(
                                        "num h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium shadow-none",
                                        severityActionButtonClass[
                                            resolvedSeverity
                                        ],
                                    )}
                                    onClick={item.onClick}
                                >
                                    <span>
                                        {item.label}
                                        {item.count !== undefined
                                            ? `（${item.count}）`
                                            : null}
                                    </span>
                                    <ChevronRightIcon aria-hidden="true" />
                                </Button>
                            </li>
                        ))}
                    </ul>
                ) : null}
            </div>

            {visibleActions.length > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center gap-1">
                    {visibleActions.map((action) => (
                        <Button
                            key={action.id}
                            id={
                                action.buttonId ??
                                (idPrefix
                                    ? `${idPrefix}-risk-action-${toAutomationIdSegment(action.id)}`
                                    : undefined)
                            }
                            type="button"
                            variant="ghost"
                            size="xs"
                            disabled={action.disabled}
                            className={cn(
                                "num h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium shadow-none",
                                severityActionButtonClass[resolvedSeverity],
                            )}
                            onClick={action.onClick}
                        >
                            <span>
                                {action.label}
                                {action.count !== undefined
                                    ? `（${action.count}）`
                                    : null}
                            </span>
                            {action.showChevron !== false ? (
                                <ChevronRightIcon aria-hidden="true" />
                            ) : null}
                        </Button>
                    ))}
                </div>
            ) : null}
        </div>
    )
}
