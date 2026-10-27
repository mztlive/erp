"use client"

import { MoneyValue } from "@/components/business"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import type { CatalogItem } from "../../api/catalog"
import type { useAdoptionBatch } from "../../hooks/use-adoption-batch"

export const SALES_PRICE_LABELS = {
    companyDropshipPrice: "代发价（含税）",
    companyBulkPrice: "集采价（含税）",
    companyMinimumQuantity: "集采起订量",
    companyMarketPrice: "市场价（含税）",
} as const

export function AdoptionSalesPrices({
    form,
    index,
    prefix,
    item,
}: {
    form: ReturnType<typeof useAdoptionBatch>["form"]
    index: number
    prefix: string
    item: CatalogItem
}) {
    return (
        <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
                可在此定价，也可入库后补齐。入库后保持未上架。
            </p>
            <div className="grid gap-4 sm:grid-cols-2">
                {(
                    Object.keys(SALES_PRICE_LABELS) as Array<
                        keyof typeof SALES_PRICE_LABELS
                    >
                ).map((name) => (
                    <form.AppField key={name} name={`rows[${index}].${name}`}>
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor={`${prefix}-${name}`}>
                                    {SALES_PRICE_LABELS[name]}
                                </FieldLabel>
                                <Input
                                    id={`${prefix}-${name}`}
                                    inputMode="decimal"
                                    value={field.state.value}
                                    onChange={(event) =>
                                        field.handleChange(event.target.value)
                                    }
                                    onBlur={field.handleBlur}
                                    aria-invalid={
                                        field.state.meta.errors.length > 0
                                    }
                                    placeholder="可选"
                                />
                                {name === "companyMarketPrice" ? (
                                    <FieldDescription>
                                        {item.market_price != null ? (
                                            <>
                                                接口市场价：
                                                <MoneyValue
                                                    value={item.market_price}
                                                />
                                                ，已预填，可调整。
                                            </>
                                        ) : (
                                            "接口未提供市场价，可自行填写。"
                                        )}
                                    </FieldDescription>
                                ) : name === "companyMinimumQuantity" ? (
                                    <FieldDescription>
                                        达到此数量时使用公司集采价；未填时使用代发价。
                                    </FieldDescription>
                                ) : null}
                                <FieldError errors={field.state.meta.errors} />
                            </Field>
                        )}
                    </form.AppField>
                ))}
            </div>
        </div>
    )
}
