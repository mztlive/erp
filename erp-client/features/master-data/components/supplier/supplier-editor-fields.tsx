"use client"

import * as React from "react"

import { DetailWorkspaceSection } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { SUPPLIER_CAPABILITY_OPTIONS } from "@/features/master-data/lib/resource-fields"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

const CAPABILITY_SEPARATOR = "、"

export const SUPPLIER_SECTIONS: ReadonlyArray<{
    id: "profile" | "delivery" | "portal" | "history"
    label: string
}> = [
    { id: "profile", label: "供应商资料" },
    { id: "delivery", label: "配送模板" },
    { id: "portal", label: "供应商门户" },
    { id: "history", label: "变更记录" },
]

export type SupplierPrimarySectionId = (typeof SUPPLIER_SECTIONS)[number]["id"]

export const SUPPLIER_PORTAL_SUB_TABS: ReadonlyArray<{
    id: "portal-accounts" | "portal-catalog" | "portal-applications"
    label: string
}> = [
    { id: "portal-accounts", label: "门户账号" },
    { id: "portal-catalog", label: "SKU 开放" },
    { id: "portal-applications", label: "供应商申请" },
]

export type SupplierPortalSubTabId =
    (typeof SUPPLIER_PORTAL_SUB_TABS)[number]["id"]

function maskEditedSensitiveValue(raw: string): string {
    const trimmed = raw.trim()
    if (!trimmed) return "已清空（待保存）"
    if (trimmed.length >= 7) {
        return `${trimmed.slice(0, 3)}****${trimmed.slice(-4)}`
    }
    return "已修改（待保存）"
}

function parseCapabilities(value: string): string[] {
    return value
        .split(/[、,，]/)
        .map((item) => item.trim())
        .filter(Boolean)
}

/** 有揭示令牌时默认打码，查看 15 秒后自动隐藏；本地已修改后失焦保留新值脱敏提示而不回退旧值。 */
export function SensitiveEditableField({
    label,
    id,
    value,
    maskedValue,
    revealToken,
    onChange,
    disabled,
    canReveal = false,
    edited = false,
    numeric = false,
    getRevealToken,
    placeholder,
}: {
    label: string
    id: string
    value: string
    maskedValue?: string
    revealToken?: string
    onChange: (next: string) => void
    disabled?: boolean
    canReveal?: boolean
    edited?: boolean
    numeric?: boolean
    getRevealToken?: () => Promise<string | undefined>
    placeholder?: string
}) {
    const [revealed, setRevealed] = React.useState(false)
    const [revealedValue, setRevealedValue] = React.useState<string | null>(
        null,
    )
    const [revealError, setRevealError] = React.useState<string | null>(null)
    const hasLocalEdit = edited || Boolean(value)

    React.useEffect(() => {
        if (!canReveal) {
            setRevealed(false)
            setRevealedValue(null)
            setRevealError(null)
            return
        }
        if (!revealed) return
        const timer = window.setTimeout(() => {
            setRevealed(false)
            setRevealedValue(null)
        }, 15000)
        return () => window.clearTimeout(timer)
    }, [canReveal, revealed])

    if (!revealToken) {
        return (
            <div className="space-y-1.5">
                <Label htmlFor={id}>{label}</Label>
                <Input
                    id={id}
                    value={value}
                    onChange={(event) => onChange(event.target.value)}
                    disabled={disabled}
                    placeholder={placeholder}
                    className={numeric ? "num" : undefined}
                />
            </div>
        )
    }

    const reveal = async () => {
        if (hasLocalEdit) {
            setRevealedValue(value)
            setRevealError(null)
            setRevealed(true)
            return
        }
        try {
            const activeToken = (await getRevealToken?.()) ?? revealToken
            if (!activeToken) {
                throw new Error("查看授权已过期，请刷新页面后重试")
            }
            const plaintext =
                value || (await revealMasterDataSensitive(activeToken))
            setRevealedValue(plaintext)
            setRevealError(null)
            setRevealed(true)
        } catch (error) {
            setRevealError(getErrorMessage(error, "无权查看，请联系管理员。"))
        }
    }

    if (!revealed || !canReveal) {
        return (
            <div className="space-y-1.5">
                <Label htmlFor={canReveal ? `${id}-reveal` : undefined}>
                    {label}
                </Label>
                <div className="flex flex-wrap items-center gap-2">
                    <code
                        className={cn(
                            "num rounded-md px-2 py-1.5 text-body-compact",
                            hasLocalEdit
                                ? "bg-warning-soft text-warning-soft-foreground"
                                : "bg-muted",
                        )}
                    >
                        {hasLocalEdit
                            ? maskEditedSensitiveValue(value)
                            : maskedValue || "****"}
                    </code>
                    {canReveal && (
                        <Button
                            id={`${id}-reveal`}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={disabled && hasLocalEdit}
                            onClick={() => void reveal()}
                        >
                            {hasLocalEdit ? "继续编辑" : "查看完整信息"}
                        </Button>
                    )}
                </div>
                {canReveal && revealError ? (
                    <p className="text-xs text-destructive" role="alert">
                        {revealError}
                    </p>
                ) : hasLocalEdit ? (
                    <p className="text-xs text-warning-soft-foreground">
                        已修改（待保存，点击继续编辑）。
                    </p>
                ) : canReveal ? (
                    <p className="text-xs text-muted-foreground">
                        敏感信息已打码；查看后 15 秒自动隐藏。
                    </p>
                ) : null}
            </div>
        )
    }

    return (
        <div className="space-y-1.5">
            <Label htmlFor={id}>{label}</Label>
            <Input
                id={id}
                value={revealedValue ?? value}
                onChange={(event) => {
                    setRevealedValue(event.target.value)
                    onChange(event.target.value)
                }}
                onBlur={() => {
                    setRevealed(false)
                    setRevealedValue(null)
                }}
                disabled={disabled}
                placeholder={placeholder}
                className={numeric ? "num" : undefined}
            />
            <p className="text-xs text-muted-foreground">
                已显示明文；离开输入框后自动打码，未保存修改将保留。
            </p>
        </div>
    )
}

export function CapabilityCheckboxGroup({
    value,
    onChange,
    disabled,
}: {
    value: string
    onChange: (next: string) => void
    disabled?: boolean
}) {
    const selected = parseCapabilities(value)
    const toggle = (option: string, checked: boolean) => {
        const next = checked
            ? [...selected, option]
            : selected.filter((item) => item !== option)
        onChange(next.join(CAPABILITY_SEPARATOR))
    }
    return (
        <div className="grid grid-cols-2 gap-x-3 gap-y-1.5 sm:grid-cols-3 lg:grid-cols-5">
            {SUPPLIER_CAPABILITY_OPTIONS.map((option) => (
                <label
                    key={option}
                    className="flex items-center gap-2 text-xs leading-none"
                >
                    <Checkbox
                        id={`master-data-supplier-capability-${toAutomationIdSegment(option)}`}
                        checked={selected.includes(option)}
                        disabled={disabled}
                        onCheckedChange={(checked) =>
                            toggle(option, checked === true)
                        }
                    />
                    {option}
                </label>
            ))}
        </div>
    )
}

export function FieldShell({
    className,
    children,
}: {
    className?: string
    children: React.ReactNode
}) {
    return <div className={cn("space-y-2", className)}>{children}</div>
}

export function SectionPanel({
    id,
    title,
    description,
    children,
}: {
    id?: string
    title: string
    description?: string
    children: React.ReactNode
}) {
    return (
        <DetailWorkspaceSection
            id={id}
            title={title}
            subtitle={description}
            width="form"
        >
            <div data-slot="detail-form-grid">{children}</div>
        </DetailWorkspaceSection>
    )
}

export function CredentialGroup({
    title,
    description,
    children,
}: {
    title: string
    description?: string
    children: React.ReactNode
}) {
    return (
        <section className="space-y-4 border-t border-border/60 pt-5 first:border-t-0 first:pt-0">
            <div className="space-y-1">
                <h3 className="text-body-compact font-semibold text-foreground">
                    {title}
                </h3>
                {description ? (
                    <p className="text-xs text-muted-foreground">
                        {description}
                    </p>
                ) : null}
            </div>
            {children}
        </section>
    )
}
