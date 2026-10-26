"use client"

import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
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
    return (
        <section
            className="space-y-3 rounded-lg border p-3"
            aria-labelledby={`${prefix}-company-text-title`}
        >
            <h3
                id={`${prefix}-company-text-title`}
                className="text-sm font-medium"
            >
                公司展示资料
            </h3>
            {overCapacity ? (
                <p
                    role="status"
                    className="text-sm text-warning-soft-foreground"
                >
                    供应商原文超过公司展示容量，请整理公司名称或介绍后再入库。来源原文完整保留。
                </p>
            ) : null}
            <details
                open={overCapacity}
                className="rounded-md bg-muted/40 p-3 text-sm"
            >
                <summary
                    id={`${prefix}-source-text-toggle`}
                    className="cursor-pointer font-medium"
                >
                    供应商原文（只读）
                </summary>
                <dl className="mt-3 space-y-3">
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            商品名称 · {sourceNameLength} 个字符
                        </dt>
                        <dd className="mt-1 whitespace-pre-wrap break-words">
                            {item.name}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            商品介绍 · {sourceDescriptionLength} 个字符
                        </dt>
                        <dd className="mt-1 whitespace-pre-wrap break-words">
                            {item.description ?? "未提供"}
                        </dd>
                    </div>
                </dl>
            </details>
            <form.AppField name={`rows[${index}].companyName`}>
                {(field) => (
                    <Field>
                        <FieldLabel htmlFor={`${prefix}-company-name`}>
                            公司展示名称
                        </FieldLabel>
                        <Input
                            id={`${prefix}-company-name`}
                            value={field.state.value}
                            onChange={(event) =>
                                field.handleChange(event.target.value)
                            }
                            onBlur={field.handleBlur}
                            aria-invalid={field.state.meta.errors.length > 0}
                            aria-describedby={`${prefix}-company-name-help ${prefix}-company-name-error`}
                        />
                        <FieldDescription id={`${prefix}-company-name-help`}>
                            已填写 {Array.from(field.state.value.trim()).length}{" "}
                            / 128 个字符。公司商品及新 SKU
                            使用此名称，不修改供应商原文。
                        </FieldDescription>
                        <FieldError
                            id={`${prefix}-company-name-error`}
                            errors={field.state.meta.errors}
                        />
                    </Field>
                )}
            </form.AppField>
            <form.AppField name={`rows[${index}].companyDescription`}>
                {(field) => (
                    <Field>
                        <FieldLabel htmlFor={`${prefix}-company-description`}>
                            公司商品介绍
                        </FieldLabel>
                        <Textarea
                            id={`${prefix}-company-description`}
                            value={field.state.value}
                            onChange={(event) =>
                                field.handleChange(event.target.value)
                            }
                            onBlur={field.handleBlur}
                            rows={4}
                            aria-invalid={field.state.meta.errors.length > 0}
                            aria-describedby={`${prefix}-company-description-help ${prefix}-company-description-error`}
                        />
                        <FieldDescription
                            id={`${prefix}-company-description-help`}
                        >
                            已填写 {Array.from(field.state.value.trim()).length}{" "}
                            / 512
                            个字符。清空后入库表示公司暂不展示介绍，供应商原文仍保留。
                        </FieldDescription>
                        <FieldError
                            id={`${prefix}-company-description-error`}
                            errors={field.state.meta.errors}
                        />
                    </Field>
                )}
            </form.AppField>
        </section>
    )
}
