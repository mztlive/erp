"use client"

import * as React from "react"
import { ChevronRightIcon, TriangleAlertIcon } from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { RequiredMark } from "@/components/ui/required-mark"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"
import type { CatalogItem } from "../../api/catalog"
import type { useAdoptionBatch } from "../../hooks/use-adoption-batch"

/** 首次入库的公司展示内容；来源原文始终保留且只读。 */
export function AdoptionProductText({
    form,
    item,
    index,
    prefix,
}: {
    form: ReturnType<typeof useAdoptionBatch>["form"]
    item: CatalogItem
    index: number
    prefix: string
}) {
    const sourceNameLength = Array.from(item.name.trim()).length
    const sourceDescriptionLength = Array.from(
        item.description?.trim() ?? "",
    ).length
    const overCapacity = sourceNameLength > 128 || sourceDescriptionLength > 512
    const [sourceOpen, setSourceOpen] = React.useState(overCapacity)
    return (
        <div className="space-y-4">
            {overCapacity ? (
                <Alert variant="warning" role="status">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>供应商原文超过公司展示容量</AlertTitle>
                    <AlertDescription>
                        请整理公司名称或介绍后再入库。来源原文完整保留。
                    </AlertDescription>
                </Alert>
            ) : null}
            <details
                open={sourceOpen}
                onToggle={(event) => setSourceOpen(event.currentTarget.open)}
                className="group rounded-lg border bg-muted/20"
            >
                <summary
                    id={`${prefix}-source-text-toggle`}
                    className="flex cursor-pointer list-none flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-sm font-medium [&::-webkit-details-marker]:hidden"
                >
                    <span className="flex items-center gap-2">
                        <ChevronRightIcon
                            className="size-4 text-muted-foreground transition-transform group-open:rotate-90"
                            aria-hidden="true"
                        />
                        供应商原文
                        <Badge variant="secondary">只读</Badge>
                    </span>
                    <span className="text-xs font-normal text-muted-foreground">
                        名称 {sourceNameLength} 字 · 介绍{" "}
                        {sourceDescriptionLength} 字
                    </span>
                </summary>
                <dl className="space-y-3 border-t px-3 py-3 text-sm">
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            商品名称
                        </dt>
                        <dd className="mt-0.5 break-words whitespace-pre-wrap">
                            {item.name}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            商品介绍
                        </dt>
                        <dd className="mt-0.5 break-words whitespace-pre-wrap">
                            {item.description ?? "未提供"}
                        </dd>
                    </div>
                </dl>
            </details>
            <form.AppField name={`rows[${index}].companyName`}>
                {(field) => {
                    const length = Array.from(field.state.value.trim()).length
                    return (
                        <Field>
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <FieldLabel htmlFor={`${prefix}-company-name`}>
                                    公司展示名称
                                    <RequiredMark />
                                </FieldLabel>
                                {field.state.value !== item.name ? (
                                    <Button
                                        id={`${prefix}-restore-company-name`}
                                        type="button"
                                        variant="link"
                                        size="xs"
                                        className="h-auto px-0 text-xs"
                                        onClick={() =>
                                            field.handleChange(item.name)
                                        }
                                    >
                                        使用供应商原文
                                    </Button>
                                ) : null}
                            </div>
                            <Input
                                id={`${prefix}-company-name`}
                                value={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.value)
                                }
                                onBlur={field.handleBlur}
                                aria-invalid={
                                    field.state.meta.errors.length > 0
                                }
                                aria-describedby={`${prefix}-company-name-help ${prefix}-company-name-error`}
                            />
                            <div
                                id={`${prefix}-company-name-help`}
                                className="flex flex-wrap items-start justify-between gap-x-3 gap-y-1"
                            >
                                <FieldDescription>
                                    公司商品及新 SKU 使用此名称。
                                </FieldDescription>
                                <span
                                    className={cn(
                                        "shrink-0 text-xs tabular-nums",
                                        length > 128
                                            ? "text-destructive"
                                            : "text-muted-foreground",
                                    )}
                                >
                                    {length}/128
                                </span>
                            </div>
                            <FieldError
                                id={`${prefix}-company-name-error`}
                                errors={field.state.meta.errors}
                            />
                        </Field>
                    )
                }}
            </form.AppField>
            <form.AppField name={`rows[${index}].companyDescription`}>
                {(field) => {
                    const length = Array.from(field.state.value.trim()).length
                    return (
                        <Field>
                            <FieldLabel
                                htmlFor={`${prefix}-company-description`}
                            >
                                公司商品介绍
                            </FieldLabel>
                            <Textarea
                                id={`${prefix}-company-description`}
                                value={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.value)
                                }
                                onBlur={field.handleBlur}
                                rows={3}
                                aria-invalid={
                                    field.state.meta.errors.length > 0
                                }
                                aria-describedby={`${prefix}-company-description-help ${prefix}-company-description-error`}
                            />
                            <div
                                id={`${prefix}-company-description-help`}
                                className="flex flex-wrap items-start justify-between gap-x-3 gap-y-1"
                            >
                                <FieldDescription>
                                    留空则公司暂不展示介绍，供应商原文仍保留。
                                </FieldDescription>
                                <span
                                    className={cn(
                                        "shrink-0 text-xs tabular-nums",
                                        length > 512
                                            ? "text-destructive"
                                            : "text-muted-foreground",
                                    )}
                                >
                                    {length}/512
                                </span>
                            </div>
                            <FieldError
                                id={`${prefix}-company-description-error`}
                                errors={field.state.meta.errors}
                            />
                        </Field>
                    )
                }}
            </form.AppField>
        </div>
    )
}
