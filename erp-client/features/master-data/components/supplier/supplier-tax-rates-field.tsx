"use client"

import { useRef, useState } from "react"
import { PlusIcon, XIcon } from "lucide-react"
import { useAppForm } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
    InputGroupText,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    parseSupplierTaxRates,
    supplierTaxPercentages,
} from "@/lib/supplier-tax-rates"

/** 候选税率逐项维护；确认添加后写入供应商原表单，沿用其保存和离开保护。 */
export function SupplierTaxRatesField({
    id,
    value,
    onChange,
    disabled,
}: {
    id: string
    value: string
    onChange: (value: string) => void
    disabled: boolean
}) {
    const [open, setOpen] = useState(false)
    const triggerRef = useRef<HTMLButtonElement>(null)
    const rates = parseSupplierTaxRates(value)
    const form = useAppForm({
        defaultValues: { rate: "" },
        onSubmit: ({ value: draft }) => {
            if (disabled) return
            onChange(
                supplierTaxPercentages(
                    parseSupplierTaxRates(`${value}、${draft.rate}`),
                ),
            )
            setOpen(false)
        },
    })

    return (
        <div
            className="min-w-0 space-y-2"
            role="group"
            aria-labelledby={`${id}-label`}
            aria-describedby={`${id}-description`}
        >
            <Label id={`${id}-label`}>常用进项税率</Label>
            <div className="flex min-h-9 flex-wrap items-center gap-2">
                {rates.map((rate) => {
                    const percent = supplierTaxPercentages([rate])
                    return (
                        <Badge
                            key={rate}
                            variant="secondary"
                            className="h-8 gap-1 pl-3 pr-1 text-sm"
                        >
                            <span className="num">{percent}%</span>
                            <Button
                                id={`${id}-${toAutomationIdSegment(rate)}-remove`}
                                type="button"
                                variant="ghost"
                                size="icon-sm"
                                className="size-6"
                                aria-label={`删除税率 ${percent}%`}
                                disabled={disabled}
                                onClick={() => {
                                    onChange(
                                        supplierTaxPercentages(
                                            rates.filter(
                                                (item) => item !== rate,
                                            ),
                                        ),
                                    )
                                    triggerRef.current?.focus()
                                }}
                            >
                                <XIcon aria-hidden="true" />
                            </Button>
                        </Badge>
                    )
                })}
                {rates.length === 0 && (
                    <span className="text-sm text-muted-foreground">
                        未登记
                    </span>
                )}
                <Popover
                    open={open && !disabled}
                    onOpenChange={(next) => {
                        if (next) form.reset()
                        setOpen(next)
                    }}
                >
                    <PopoverTrigger
                        render={
                            <Button
                                ref={triggerRef}
                                id={`${id}-add`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={disabled}
                            />
                        }
                    >
                        <PlusIcon aria-hidden="true" />
                        添加税率
                    </PopoverTrigger>
                    <PopoverContent
                        align="start"
                        className="w-72 max-w-[calc(100vw-2rem)]"
                    >
                        <PopoverTitle>添加常用进项税率</PopoverTitle>
                        <form.AppField
                            name="rate"
                            validators={{
                                onChange: ({ value: raw }) => {
                                    try {
                                        const next = parseSupplierTaxRates(
                                            raw.trim(),
                                        )
                                        if (
                                            next.length !== 1 ||
                                            /[、,，;；\s]/.test(raw.trim())
                                        )
                                            return "每次填写一个税率，如 9%"
                                        if (rates.includes(next[0]))
                                            return "该税率已添加"
                                        parseSupplierTaxRates(
                                            `${value}、${raw}`,
                                        )
                                    } catch (error) {
                                        return error instanceof Error
                                            ? error.message
                                            : "请输入有效税率"
                                    }
                                },
                            }}
                        >
                            {(field) => (
                                <div className="space-y-2">
                                    <Label htmlFor={id}>税率（%）</Label>
                                    <InputGroup>
                                        <InputGroupInput
                                            id={id}
                                            value={field.state.value}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                            }
                                            onBlur={field.handleBlur}
                                            onKeyDown={(event) => {
                                                if (
                                                    event.key !== "Enter" ||
                                                    event.nativeEvent
                                                        .isComposing
                                                )
                                                    return
                                                event.preventDefault()
                                                event.stopPropagation()
                                                void form.handleSubmit()
                                            }}
                                            inputMode="decimal"
                                            placeholder="如：9"
                                            disabled={disabled}
                                            aria-invalid={
                                                field.state.meta.errors.length >
                                                0
                                            }
                                            aria-describedby={
                                                field.state.meta.errors.length
                                                    ? `${id}-error`
                                                    : undefined
                                            }
                                        />
                                        <InputGroupAddon align="inline-end">
                                            <InputGroupText>%</InputGroupText>
                                        </InputGroupAddon>
                                    </InputGroup>
                                    {field.state.meta.errors.length > 0 && (
                                        <p
                                            id={`${id}-error`}
                                            role="alert"
                                            className="text-xs text-destructive"
                                        >
                                            {field.state.meta.errors.join("；")}
                                        </p>
                                    )}
                                </div>
                            )}
                        </form.AppField>
                        <div className="flex justify-end gap-2">
                            <Button
                                id={`${id}-cancel`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <Button
                                id={`${id}-confirm`}
                                type="button"
                                size="sm"
                                disabled={disabled}
                                onClick={() => void form.handleSubmit()}
                            >
                                添加
                            </Button>
                        </div>
                    </PopoverContent>
                </Popover>
            </div>
            <p
                id={`${id}-description`}
                className="text-xs leading-5 text-muted-foreground"
            >
                仅作为商品供给的候选税率；每条供给须确定一个实际税率。
            </p>
        </div>
    )
}
