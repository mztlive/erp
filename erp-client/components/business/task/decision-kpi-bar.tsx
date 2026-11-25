"use client"

import type * as React from "react"
import { CircleQuestionMarkIcon } from "lucide-react"

import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type DecisionKpiEmphasis = "primary" | "secondary" | "terms"

export type DecisionKpiTone =
    | "default"
    | "muted"
    | "success"
    | "warning"
    | "destructive"
    | "info"

export type DecisionKpiFootnoteTone =
    | "muted"
    | "warning"
    | "destructive"
    | "success"

export type DecisionKpiItem = Readonly<{
    id: string
    label: React.ReactNode
    /** 极轻口径副标（`text-tiny`），如「含税」「供给 − 采购」「含税 / 税后」。 */
    basis?: React.ReactNode
    /** 视觉层级：`primary`（24px）/`secondary`（20px）/`terms`（13px）。 */
    emphasis?: DecisionKpiEmphasis
    /** 指标主值内容（如 `MoneyValue`、`ProfitValue` 或条款摘要）。 */
    value: React.ReactNode
    /** 主值语义色，默认 `default`。 */
    tone?: DecisionKpiTone
    /** 主值右侧行内附着信息（如毛利率、`税后倒挂` 或 `先款门禁` 徽章）。 */
    valueMeta?: React.ReactNode
    /** `?` 悬浮公式与明细分解 Tooltip 内容。 */
    formulaTooltip?: React.ReactNode
    /** 公式按钮稳定原生 id；省略且提供 `idPrefix` 时自动派生。 */
    formulaTooltipId?: string
    /** 公式按钮无障碍标签。 */
    formulaTooltipLabel?: string
    /** 覆盖率/口径/警示脚注（如 `3/5 项`、`不含费用`、`税损 -3.2pp`）。 */
    footnote?: React.ReactNode
    /** 脚注语义态。 */
    footnoteTone?: DecisionKpiFootnoteTone
    /** 额外补充内容（如第二行税后口径、进度条或直达交互按钮）。 */
    extra?: React.ReactNode
    /** 可选跨列设置。 */
    span?: 1 | 2 | "full"
    className?: string
}>

const kpiGridColumnsClass: Record<1 | 2 | 3 | 4, string> = {
    1: "grid-cols-1",
    2: "grid-cols-1 @[480px]/decision-kpi:grid-cols-2",
    3: "grid-cols-1 @[480px]/decision-kpi:grid-cols-2 @[720px]/decision-kpi:grid-cols-3",
    4: "grid-cols-1 @[480px]/decision-kpi:grid-cols-2 @[840px]/decision-kpi:grid-cols-4",
}

const kpiEmphasisClass: Record<DecisionKpiEmphasis, string> = {
    primary: "text-value-section font-semibold tracking-tight",
    secondary: "text-value-compact font-semibold tracking-tight",
    terms: "text-body-compact font-semibold",
}

const kpiToneClass: Record<DecisionKpiTone, string> = {
    default: "text-foreground",
    muted: "text-muted-foreground",
    success: "text-success-soft-foreground",
    warning: "text-warning-soft-foreground",
    destructive: "text-destructive-soft-foreground",
    info: "text-info-soft-foreground",
}

const kpiFootnoteToneClass: Record<DecisionKpiFootnoteTone, string> = {
    muted: "text-muted-foreground",
    warning: "font-medium text-warning-soft-foreground",
    destructive: "font-medium text-destructive-soft-foreground",
    success: "font-medium text-success-soft-foreground",
}

export interface DecisionKpiCellProps extends Omit<
    DecisionKpiItem,
    "id" | "span"
> {
    id?: string
    span?: 1 | 2 | "full"
    /** 三列布局中第 3 项在双列中屏自动通栏，避免出现空白死格。 */
    fillThreeColMidRow?: boolean
}

/**
 * 决策 KPI 栏单项格：
 * - 顶行：12px 标签 + 11px 极轻口径副标 + 可选 `?` 公式 Tooltip
 * - 主值：按 `primary`（24px）/ `secondary`（20px）/ `terms`（13px）分明层级，默认启用 `.num`
 * - 底行：覆盖率/口径/警示脚注与可选补充块
 */
export function DecisionKpiCell({
    id,
    label,
    basis,
    emphasis = "secondary",
    value,
    tone = "default",
    valueMeta,
    formulaTooltip,
    formulaTooltipId,
    formulaTooltipLabel,
    footnote,
    footnoteTone = "muted",
    extra,
    span = 1,
    fillThreeColMidRow = false,
    className,
}: DecisionKpiCellProps) {
    const resolvedTooltipLabel =
        formulaTooltipLabel ??
        (typeof label === "string" ? `${label}计算口径` : "计算口径说明")

    return (
        <div
            id={id}
            data-slot="decision-kpi-cell"
            data-emphasis={emphasis}
            className={cn(
                "min-w-0",
                span === 2 && "@[480px]/decision-kpi:col-span-2",
                span === "full" && "col-span-full",
                span === 1 &&
                    fillThreeColMidRow &&
                    "@[480px]/decision-kpi:col-span-2 @[720px]/decision-kpi:col-span-1",
                className,
            )}
        >
            <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                <span>{label}</span>
                {basis != null && basis !== false && basis !== "" ? (
                    <span className="text-tiny text-muted-foreground/80">
                        {basis}
                    </span>
                ) : null}
                {formulaTooltip != null ? (
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <button
                                    id={formulaTooltipId}
                                    type="button"
                                    aria-label={resolvedTooltipLabel}
                                    className="inline-flex size-4 shrink-0 cursor-help items-center justify-center rounded-sm text-muted-foreground/80 transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                />
                            }
                        >
                            <CircleQuestionMarkIcon
                                aria-hidden="true"
                                className="size-3"
                            />
                        </TooltipTrigger>
                        <TooltipContent className="block">
                            {formulaTooltip}
                        </TooltipContent>
                    </Tooltip>
                ) : null}
            </div>

            <div
                className={cn(
                    "num mt-1 flex max-w-full flex-wrap items-baseline gap-x-2 gap-y-1 wrap-anywhere",
                    kpiEmphasisClass[emphasis],
                    kpiToneClass[tone],
                )}
            >
                <span className="min-w-0">{value}</span>
                {valueMeta != null && valueMeta !== false ? (
                    <span className="inline-flex flex-wrap items-center gap-1.5 text-body-compact font-medium">
                        {valueMeta}
                    </span>
                ) : null}
            </div>

            {footnote != null && footnote !== false && footnote !== "" ? (
                <div
                    className={cn(
                        "num mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-tiny wrap-anywhere",
                        kpiFootnoteToneClass[footnoteTone],
                    )}
                >
                    {footnote}
                </div>
            ) : null}

            {extra != null && extra !== false ? (
                <div className="mt-1.5 min-w-0">{extra}</div>
            ) : null}
        </div>
    )
}

export interface DecisionKpiBarProps extends Omit<
    React.ComponentProps<"div">,
    "children"
> {
    /** 声明式指标项列表（自动过滤空值项）；全部为空且未传 `children` 时返回 `null`。 */
    items?: readonly (DecisionKpiItem | null | undefined | false)[]
    /** 可选组合式内容（配合 `DecisionKpiCell` 使用）。 */
    children?: React.ReactNode
    /** 显式列数；省略时根据有效指标数自适应 `1 / 2 / 3 / 4` 列，绝不留空白死格。 */
    columns?: 1 | 2 | 3 | 4
    /** 指标区无障碍名称。 */
    ariaLabel?: string
    /** 自动化 DOM id 前缀，用于为各格 `?` 公式按钮派生稳定原生 id。 */
    idPrefix?: string
    /** 内层柔底网格容器类名。 */
    gridClassName?: string
}

function resolveColumns(
    explicit: 1 | 2 | 3 | 4 | undefined,
    itemCount: number,
): 1 | 2 | 3 | 4 {
    if (explicit) return explicit
    if (itemCount <= 1) return 1
    if (itemCount === 2) return 2
    if (itemCount === 3) return 3
    return 4
}

/**
 * 任务页模式 A2 决策 KPI 栏（docs/task-page-design-system.md §4.2）：
 * - 柔底聚焦容器：`bg-surface-sunken rounded-lg px-4 py-3.5`
 * - 自带 `@container/decision-kpi` 容器查询，按有效指标数自适应 2/3/4 列，零空白死格
 * - 严格字号主次梯度、轻口径副标、`?` 公式 Tooltip 与覆盖率/警示脚注
 */
export function DecisionKpiBar({
    items,
    children,
    columns,
    ariaLabel = "关键决策指标",
    idPrefix,
    className,
    gridClassName,
    ...props
}: DecisionKpiBarProps) {
    const visibleItems = (items ?? []).filter((item): item is DecisionKpiItem =>
        Boolean(item),
    )

    if (visibleItems.length === 0 && children == null) {
        return null
    }

    const resolvedColumns = resolveColumns(
        columns,
        visibleItems.length > 0 ? visibleItems.length : (columns ?? 3),
    )

    return (
        <TooltipProvider>
            <div
                data-slot="decision-kpi-bar"
                data-columns={resolvedColumns}
                className={cn("@container/decision-kpi min-w-0", className)}
                {...props}
            >
                <div
                    role="region"
                    aria-label={ariaLabel}
                    className={cn(
                        "grid min-w-0 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 [&_[data-slot=money-value]]:max-w-full [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal",
                        kpiGridColumnsClass[resolvedColumns],
                        gridClassName,
                    )}
                >
                    {visibleItems.map((item, index) => (
                        <DecisionKpiCell
                            key={item.id}
                            label={item.label}
                            basis={item.basis}
                            emphasis={item.emphasis}
                            value={item.value}
                            tone={item.tone}
                            valueMeta={item.valueMeta}
                            formulaTooltip={item.formulaTooltip}
                            formulaTooltipId={
                                item.formulaTooltipId ??
                                (idPrefix
                                    ? `${idPrefix}-kpi-${toAutomationIdSegment(item.id)}-formula`
                                    : undefined)
                            }
                            formulaTooltipLabel={item.formulaTooltipLabel}
                            footnote={item.footnote}
                            footnoteTone={item.footnoteTone}
                            extra={item.extra}
                            span={item.span}
                            fillThreeColMidRow={
                                resolvedColumns === 3 &&
                                visibleItems.length === 3 &&
                                index === 2
                            }
                            className={item.className}
                        />
                    ))}
                    {children}
                </div>
            </div>
        </TooltipProvider>
    )
}
