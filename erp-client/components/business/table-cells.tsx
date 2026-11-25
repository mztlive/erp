"use client"

import * as React from "react"
import type { LucideIcon } from "lucide-react"

import { BusinessStatusBadge } from "@/components/business/values"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

/**
 * 表格空值占位。
 * - dash（默认）：淡化全角破折号 `—`（`text-muted-foreground/60`）
 * - label：语义未填写文案（如「未填写」，12px `text-muted-foreground`）
 */
export function TableEmptyCell({
    label = "—",
    variant = "dash",
    className,
}: {
    label?: React.ReactNode
    variant?: "dash" | "label"
    className?: string
}) {
    return (
        <span
            data-slot="table-empty-cell"
            className={cn(
                variant === "label"
                    ? "text-xs text-muted-foreground"
                    : "text-muted-foreground/60",
                className,
            )}
        >
            {label}
        </span>
    )
}

/**
 * 表格副查询局部状态单元格。
 * 副查询（如 SKU、供给）加载中或失败时仅局部降级，不阻塞整表渲染。
 */
export function TableAsyncCell({
    pending = false,
    failed = false,
    pendingLabel = "读取中…",
    failedLabel = "暂不可查",
    className,
    children,
}: {
    pending?: boolean
    failed?: boolean
    pendingLabel?: React.ReactNode
    failedLabel?: React.ReactNode
    className?: string
    children: React.ReactNode
}) {
    if (pending || failed) {
        return (
            <span
                data-slot="table-async-cell"
                className={cn("text-xs text-muted-foreground", className)}
            >
                {pending ? pendingLabel : failedLabel}
            </span>
        )
    }
    return <>{children}</>
}

/**
 * 主身份双行单元格（13px 中粗主名 + 12px 副身份编号/关键事实）。
 * 搭配 `meta: { width: "flex" }` 与左冻结列使用。
 */
export function TableIdentityCell({
    primary,
    secondary,
    primaryTitle,
    secondaryTitle,
    numericSecondary = false,
    className,
}: {
    primary: React.ReactNode
    secondary?: React.ReactNode
    primaryTitle?: string
    secondaryTitle?: string
    numericSecondary?: boolean
    className?: string
}) {
    const resolvedPrimaryTitle =
        primaryTitle ??
        (typeof primary === "string" || typeof primary === "number"
            ? String(primary)
            : undefined)
    const resolvedSecondaryTitle =
        secondaryTitle ??
        (typeof secondary === "string" || typeof secondary === "number"
            ? String(secondary)
            : undefined)

    return (
        <div
            data-slot="table-identity-cell"
            className={cn("min-w-0", className)}
        >
            <div
                className="truncate font-medium text-foreground"
                title={resolvedPrimaryTitle}
            >
                {primary}
            </div>
            {secondary != null && secondary !== "" ? (
                <div
                    className={cn(
                        "truncate text-xs leading-4 text-muted-foreground/80",
                        numericSecondary && "num",
                    )}
                    title={resolvedSecondaryTitle}
                >
                    {secondary}
                </div>
            ) : null}
        </div>
    )
}

/**
 * 标准表格文本单元格：继承表格 13px（`text-body-compact`）与 `text-foreground/90`，空值自动降级为淡灰 `—`。
 */
export function TableTextCell({
    value,
    emptyLabel = "—",
    emptyVariant = "dash",
    clamp,
    numeric = false,
    title,
    className,
}: {
    value?: string | null
    emptyLabel?: React.ReactNode
    emptyVariant?: "dash" | "label"
    /** 多行或单行截断；2 行默认限制 max-w-56。 */
    clamp?: 1 | 2
    numeric?: boolean
    title?: string
    className?: string
}) {
    const trimmed = value?.trim() ?? ""
    if (!trimmed) {
        return <TableEmptyCell label={emptyLabel} variant={emptyVariant} />
    }
    return (
        <span
            data-slot="table-text-cell"
            className={cn(
                "text-foreground/90",
                numeric && "num",
                clamp === 1 && "block truncate",
                clamp === 2 && "line-clamp-2 max-w-56",
                className,
            )}
            title={title ?? (clamp ? trimmed : undefined)}
        >
            {trimmed}
        </span>
    )
}

/**
 * 可点击语义状态单元格（对齐 list-table-design-system.md §5.4 #6）：
 * 自动拦截事件冒泡（`stopPropagation`），防止触发整行点击/预览；
 * 支持直接传 `label`/`tone`/`appearance` 渲染 `BusinessStatusBadge`，或传自定义 `children`。
 */
export function TableStatusActionCell({
    label,
    tone,
    icon,
    appearance = "badge",
    description,
    children,
    className,
    onClick,
    type = "button",
    variant = "ghost",
    size = "xs",
    ...props
}: Omit<React.ComponentProps<typeof Button>, "children"> & {
    label?: string
    tone?: StatusTone
    icon?: LucideIcon
    appearance?: "badge" | "text" | "quiet"
    description?: React.ReactNode
    children?: React.ReactNode
}) {
    return (
        <Button
            data-slot="table-status-action-cell"
            type={type}
            variant={variant}
            size={size}
            className={cn(
                "h-auto max-w-full px-0 py-0.5 font-normal",
                className,
            )}
            onClick={(event) => {
                event.stopPropagation()
                onClick?.(event)
            }}
            {...props}
        >
            {children ??
                (label != null ? (
                    <BusinessStatusBadge
                        context="list"
                        tone={tone}
                        icon={icon}
                        appearance={appearance}
                        description={description}
                        label={label}
                    />
                ) : null)}
        </Button>
    )
}

/**
 * 行内即时开关单元格：
 * - 有写权限时渲染 `Switch size="sm"`，并支持在开关旁挂轻量 `text-tiny` 琥珀中间态微标（如「部分」）；
 * - 无写权限时自动降级为只读 `BusinessStatusBadge context="list"`，支持 `appearance="quiet"` 与 `className`。
 */
export function TableSwitchCell({
    id,
    canEdit,
    checked,
    disabled = false,
    disabledReason,
    statusLabel,
    statusTone,
    appearance = "badge",
    intermediateLabel,
    intermediateTitle,
    accessibleName,
    title,
    onCheckedChange,
    className,
}: {
    id: string
    canEdit: boolean
    checked: boolean
    disabled?: boolean
    /** 禁用时包一层容器承载原生 tooltip，保证悬停可见原因。 */
    disabledReason?: string
    statusLabel: string
    statusTone?: StatusTone
    /** 无写权降级为只读徽章时的呈现风格（支持 `"quiet"` 降噪）。 */
    appearance?: "badge" | "text" | "quiet"
    /** 中间态（如「部分上架」）在开关旁展示的 `text-tiny` 琥珀微标内容。 */
    intermediateLabel?: React.ReactNode
    /** 中间态琥珀微标的悬停说明。 */
    intermediateTitle?: string
    accessibleName: string
    title?: string
    onCheckedChange: (checked: boolean) => void
    className?: string
}) {
    if (!canEdit) {
        return (
            <BusinessStatusBadge
                context="list"
                appearance={appearance}
                tone={statusTone ?? (checked ? "success" : "neutral")}
                label={statusLabel}
                className={className}
            />
        )
    }

    const hasIntermediate =
        intermediateLabel != null &&
        intermediateLabel !== "" &&
        intermediateLabel !== false
    const resolvedTitle = title ?? disabledReason ?? statusLabel
    const switchNode = (
        <Switch
            nativeButton
            render={
                <button
                    type="button"
                    aria-label={accessibleName}
                    title={resolvedTitle}
                />
            }
            id={id}
            size="sm"
            checked={checked}
            disabled={disabled}
            onClick={(event) => event.stopPropagation()}
            onCheckedChange={onCheckedChange}
            aria-label={accessibleName}
            title={resolvedTitle}
            className={hasIntermediate ? undefined : className}
        />
    )
    const switchWithHint =
        disabled && disabledReason ? (
            <span className="inline-flex" title={disabledReason}>
                {switchNode}
            </span>
        ) : (
            switchNode
        )

    if (!hasIntermediate) {
        return switchWithHint
    }

    return (
        <div
            data-slot="table-switch-cell"
            className={cn("inline-flex items-center gap-1.5", className)}
        >
            {switchWithHint}
            <span
                data-slot="table-switch-intermediate"
                title={intermediateTitle}
                className="inline-flex items-center rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny leading-4 font-medium text-warning-soft-foreground"
            >
                {intermediateLabel}
            </span>
        </div>
    )
}
