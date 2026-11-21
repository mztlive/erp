"use client"

import * as React from "react"

import { BusinessStatusBadge } from "@/components/business/values"
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
    return (
        <div
            data-slot="table-identity-cell"
            className={cn("min-w-0", className)}
        >
            <div
                className="truncate leading-5 font-medium text-foreground"
                title={primaryTitle}
            >
                {primary}
            </div>
            {secondary != null && secondary !== "" ? (
                <div
                    className={cn(
                        "truncate text-xs leading-4 text-muted-foreground/80",
                        numericSecondary && "num",
                    )}
                    title={secondaryTitle}
                >
                    {secondary}
                </div>
            ) : null}
        </div>
    )
}

/**
 * 标准表格文本单元格：继承表格 13px（`text-body-compact`），空值自动降级为淡灰 `—`。
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
 * 行内即时开关单元格：
 * - 有写权限时渲染 `Switch size="sm"`；
 * - 无写权限时自动降级为只读 `BusinessStatusBadge context="list"`。
 */
export function TableSwitchCell({
    id,
    canEdit,
    checked,
    disabled = false,
    statusLabel,
    statusTone,
    accessibleName,
    title,
    onCheckedChange,
    className,
}: {
    id: string
    canEdit: boolean
    checked: boolean
    disabled?: boolean
    statusLabel: string
    statusTone?: StatusTone
    accessibleName: string
    title?: string
    onCheckedChange: (checked: boolean) => void
    className?: string
}) {
    if (!canEdit) {
        return (
            <BusinessStatusBadge
                context="list"
                tone={statusTone ?? (checked ? "success" : "neutral")}
                label={statusLabel}
            />
        )
    }

    const resolvedTitle = title ?? statusLabel
    return (
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
            onCheckedChange={onCheckedChange}
            aria-label={accessibleName}
            title={resolvedTitle}
            className={className}
        />
    )
}
