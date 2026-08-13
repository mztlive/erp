"use client"

import * as React from "react"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { SUPPLIER_CAPABILITY_OPTIONS } from "@/features/master-data/lib/resource-fields"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { getErrorMessage } from "@/lib/api/errors"
import { cn } from "@/lib/utils"

const CAPABILITY_SEPARATOR = "、"

export const SUPPLIER_SECTIONS: ReadonlyArray<{
    id: string
    label: string
}> = [
    { id: "basic", label: "基本信息" },
    { id: "commercial", label: "商务合作" },
    { id: "contract", label: "合同资质" },
    { id: "invoice", label: "开票信息" },
    { id: "history", label: "历史引用" },
]

function parseCapabilities(value: string): string[] {
    return value
        .split(/[、,，]/)
        .map((item) => item.trim())
        .filter(Boolean)
}

/** 有揭示令牌时默认打码，查看 15 秒后自动隐藏。 */
export function SensitiveEditableField({
    label,
    id,
    value,
    maskedValue,
    revealToken,
    onChange,
    disabled,
    canReveal = false,
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
    getRevealToken?: () => Promise<string | undefined>
    placeholder?: string
}) {
    const [revealed, setRevealed] = React.useState(false)
    const [revealedValue, setRevealedValue] = React.useState<string | null>(
        null,
    )
    const [revealError, setRevealError] = React.useState<string | null>(null)

    React.useEffect(() => {
        if (!revealed) return
        const timer = window.setTimeout(() => {
            setRevealed(false)
            setRevealedValue(null)
        }, 15000)
        return () => window.clearTimeout(timer)
    }, [revealed])

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
                />
            </div>
        )
    }

    const reveal = async () => {
        try {
            const activeToken = (await getRevealToken?.()) ?? revealToken
            if (!activeToken) {
                throw new Error("敏感字段查看凭证已失效，请刷新后重试")
            }
            const plaintext =
                value || (await revealMasterDataSensitive(activeToken))
            setRevealedValue(plaintext)
            setRevealError(null)
            setRevealed(true)
        } catch (error) {
            setRevealError(getErrorMessage(error, "无权查看"))
        }
    }

    if (!revealed) {
        return (
            <div className="space-y-1.5">
                <Label htmlFor={id}>{label}</Label>
                <div className="flex flex-wrap items-center gap-2">
                    <code className="num rounded-md bg-muted px-2 py-1.5 text-sm">
                        {maskedValue || "****"}
                    </code>
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!canReveal}
                        onClick={() => void reveal()}
                    >
                        短时查看
                    </Button>
                </div>
                {revealError ? (
                    <p className="text-xs text-destructive" role="alert">
                        {revealError}
                    </p>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        敏感信息已打码；查看后 15 秒自动隐藏。
                    </p>
                )}
            </div>
        )
    }

    return (
        <div className="space-y-1.5">
            <Label htmlFor={id}>{label}</Label>
            <Input
                id={id}
                value={revealedValue ?? value}
                autoFocus
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
            />
            <p className="text-xs text-muted-foreground">
                已显示明文；离开输入框后自动打码。
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
                    className="flex items-center gap-2 text-sm leading-none"
                >
                    <Checkbox
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
    return (
        <div
            className={cn(
                "space-y-2 [&_[data-slot=label]]:text-[13px] [&_[data-slot=label]]:font-medium [&_[data-slot=label]]:text-foreground/80",
                className,
            )}
        >
            {children}
        </div>
    )
}

export function SectionPanel({
    title,
    description,
    children,
}: {
    title: string
    description?: string
    children: React.ReactNode
}) {
    return (
        <section className="space-y-5">
            <div className="space-y-1 border-b border-border/60 pb-3">
                <h2 className="text-base font-semibold tracking-tight">
                    {title}
                </h2>
                {description ? (
                    <p className="max-w-3xl text-sm leading-5 text-muted-foreground">
                        {description}
                    </p>
                ) : null}
            </div>
            {children}
        </section>
    )
}

export function CredentialGroup({
    title,
    description,
    children,
}: {
    title: string
    description: string
    children: React.ReactNode
}) {
    return (
        <section className={cn(surfaceInsetClassName, "overflow-hidden")}>
            <div className="border-b border-border/60 px-4 py-3">
                <h3 className="text-sm font-semibold text-foreground">
                    {title}
                </h3>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                    {description}
                </p>
            </div>
            <div className="p-4">{children}</div>
        </section>
    )
}
