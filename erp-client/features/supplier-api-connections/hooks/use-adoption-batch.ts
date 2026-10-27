"use client"

import * as React from "react"
import { useQueries } from "@tanstack/react-query"
import { resolveAdoption, type AdoptionMapping } from "../api/adoption"
import { useStore } from "@tanstack/react-form"
import { format } from "date-fns"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useUnitOptionsQuery } from "@/hooks/use-options"
import { rateFromPercentage } from "@/features/supplier-offerings/lib/offering-forms"
import {
    PRODUCT_KIND_LABELS,
    type ProductKind,
} from "@/features/master-data/types"
import type {
    AdoptionPreview,
    AdoptionResultRow,
    AdoptionRowInput,
} from "../api/adoption"
import { newIdempotencyKey } from "../lib/operations"
import { useConfirmAdoptionMutation } from "./adoption"

export const AUTO_UNIT = "__AUTO_UNIT__"

export type AdoptionRowState = "SUCCEEDED" | "FAILED" | "READY" | "INCOMPLETE"

type RowDraft = {
    itemId: string
    kind: "CREATE" | "EXISTING"
    categoryId: string
    unitId: string
    productKind: ProductKind
    companyName: string
    companyDescription: string
    specification: string
    companyDropshipPrice: string
    companyBulkPrice: string
    companyMinimumQuantity: string
    companyMarketPrice: string
    dropshipPrice: string
    bulkPrice: string
    minimumQuantity: string
    inputTaxPercentage: string
    validFrom: string
    validTo: string
    deliveryTemplateId: string
}

export const rowSchema = z
    .object({
        itemId: z.string(),
        kind: z.enum(["CREATE", "EXISTING"]),
        categoryId: z.string(),
        unitId: z.string().min(1, "请选择公司计量单位"),
        productKind: z.enum([
            "PHYSICAL",
            "VIRTUAL",
            "OFFLINE_SERVICE",
            "VOUCHER",
        ]),
        companyName: z.string(),
        companyDescription: z.string(),
        specification: z.string(),
        companyDropshipPrice: z.string(),
        companyBulkPrice: z.string(),
        companyMinimumQuantity: z.string(),
        companyMarketPrice: z.string(),
        dropshipPrice: z.string(),
        bulkPrice: z.string(),
        minimumQuantity: z.string(),
        inputTaxPercentage: z.string(),
        validFrom: z.string().min(1, "请选择生效日期"),
        validTo: z.string(),
        deliveryTemplateId: z.string(),
    })
    .superRefine((row, ctx) => {
        if (row.kind === "CREATE") {
            for (const name of [
                "companyDropshipPrice",
                "companyBulkPrice",
                "companyMinimumQuantity",
                "companyMarketPrice",
            ] as const) {
                if (!row[name].trim()) continue
                const quantity = name === "companyMinimumQuantity"
                try {
                    parseDecimal(row[name], { maxScale: quantity ? 6 : 2 })
                    if (quantity && compareDecimal(row[name], "0", 6) <= 0)
                        throw new Error()
                } catch {
                    ctx.addIssue({
                        code: "custom",
                        path: [name],
                        message: quantity
                            ? "请填写大于 0、最多 6 位小数的起订量"
                            : "请填写非负、最多 2 位小数的销售参考价",
                    })
                }
            }
            if (
                !row.companyName.trim() ||
                Array.from(row.companyName.trim()).length > 128
            )
                ctx.addIssue({
                    code: "custom",
                    path: ["companyName"],
                    message: "请将公司展示名称整理为 1 至 128 个字符",
                })
            if (Array.from(row.companyDescription.trim()).length > 512)
                ctx.addIssue({
                    code: "custom",
                    path: ["companyDescription"],
                    message: "请将公司介绍整理为 512 个字符以内，或明确清空",
                })
        }
        for (const name of [
            "dropshipPrice",
            "bulkPrice",
            "minimumQuantity",
            "inputTaxPercentage",
        ] as const) {
            if (
                (name === "bulkPrice" || name === "minimumQuantity") &&
                !row[name].trim()
            )
                continue
            try {
                parseDecimal(row[name], {
                    maxScale: name === "minimumQuantity" ? 6 : 4,
                })
                if (
                    name === "minimumQuantity" &&
                    compareDecimal(row[name], "0", 6) <= 0
                )
                    throw new Error()
                if (
                    name === "inputTaxPercentage" &&
                    compareDecimal(row[name], "100", 4) > 0
                )
                    throw new Error()
            } catch {
                ctx.addIssue({
                    code: "custom",
                    path: [name],
                    message:
                        name === "inputTaxPercentage"
                            ? "请填写 0–100 的税率"
                            : name === "minimumQuantity"
                              ? "请填写大于 0 的起订量"
                              : "请填写有效的非负供给价",
                })
            }
        }
        if (
            Boolean(row.bulkPrice.trim()) !==
            Boolean(row.minimumQuantity.trim())
        ) {
            ctx.addIssue({
                code: "custom",
                path: [row.bulkPrice.trim() ? "minimumQuantity" : "bulkPrice"],
                message:
                    "集采价与集采起订数量须同时填写；无集采条件时两项均留空",
            })
        }
        if (row.validTo && row.validTo <= row.validFrom)
            ctx.addIssue({
                code: "custom",
                path: ["validTo"],
                message: "失效日期须晚于生效日期",
            })
    })

function rowMapping(row: RowDraft): AdoptionMapping {
    return {
        category_id: row.categoryId,
        unit_id: row.unitId === AUTO_UNIT ? "" : row.unitId,
        product_kind: row.productKind,
        company_name: row.companyName.trim(),
        company_description: row.companyDescription.trim(),
    }
}

function rowInput(row: RowDraft, expectedMatch: string): AdoptionRowInput {
    return {
        item_id: row.itemId,
        target: {
            kind: "AUTO",
            mapping: rowMapping(row),
            expected_match: expectedMatch,
        },
        sales_prices:
            row.kind === "CREATE"
                ? {
                      sales_visible_price_gross:
                          row.companyDropshipPrice.trim() || null,
                      bulk_price_gross: row.companyBulkPrice.trim() || null,
                      bulk_min_quantity:
                          row.companyMinimumQuantity.trim() || null,
                      market_price: row.companyMarketPrice.trim() || null,
                  }
                : null,
        terms: {
            dropship_supply_price_gross: row.dropshipPrice.trim(),
            bulk_supply_price_gross: row.bulkPrice.trim() || null,
            input_tax_rate: rateFromPercentage(row.inputTaxPercentage),
            bulk_minimum_order_quantity: row.minimumQuantity.trim() || null,
            product_capabilities: [],
            valid_from: row.validFrom,
            valid_to: row.validTo || null,
        },
        delivery_template_id: row.deliveryTemplateId || null,
    }
}

export function useAdoptionBatch({
    connectionId,
    rows,
    preview,
    onComplete,
}: {
    connectionId: string
    rows: AdoptionPreview["rows"]
    preview: AdoptionPreview
    onComplete: (rows: AdoptionResultRow[]) => void
}) {
    const [activeIndex, setActiveIndex] = React.useState(0)
    const categoryPrefillDone = React.useRef(false)
    const editedCategories = React.useRef(new Set<string>())
    const requestIdentity = React.useRef<{
        rows: AdoptionRowInput[]
        key: string
    } | null>(null)
    const [rowResults, setRowResults] = React.useState<AdoptionResultRow[]>([])
    const mutation = useConfirmAdoptionMutation()
    const categories = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "enabled",
        revisionTiming: "current",
    })
    const units = useUnitOptionsQuery()
    const form = useAppForm({
        defaultValues: {
            rows: rows.map(({ item }): RowDraft => ({
                itemId: item.id,
                kind: "CREATE",
                categoryId: "",
                unitId: item.suggested_unit || item.unit ? AUTO_UNIT : "",
                productKind: "PHYSICAL",
                companyName: item.name,
                companyDescription: item.description ?? "",
                specification: item.specification,
                companyDropshipPrice: "",
                companyBulkPrice: "",
                companyMinimumQuantity: "",
                companyMarketPrice: item.market_price ?? "",
                dropshipPrice: item.purchase_price ?? "",
                bulkPrice: "",
                minimumQuantity: "",
                inputTaxPercentage: "13",
                validFrom: format(new Date(), "yyyy-MM-dd"),
                validTo: "",
                deliveryTemplateId: "",
            })),
        },
        validators: {
            onSubmit: z.object({ rows: z.array(z.custom<RowDraft>()) }),
        },
        onSubmitInvalid: ({ formApi }) => {
            const index = formApi.state.values.rows.findIndex(
                (row) => !rowSchema.safeParse(row).success,
            )
            if (index >= 0) setActiveIndex(index)
        },
        onSubmit: async ({ value }) => {
            try {
                const successes = rowResults.filter(
                    (row) => row.status === "SUCCEEDED",
                )
                const succeededIds = new Set(
                    successes.map((row) => row.item_id),
                )
                const inputs = value.rows.flatMap((row, index) => {
                    const resolution = resolutions[index]
                    if (
                        succeededIds.has(row.itemId) ||
                        !rowSchema.safeParse(row).success ||
                        !resolution?.data ||
                        resolution.isError ||
                        resolution.isFetching
                    )
                        return []
                    return [rowInput(row, resolution.data.expected_match)]
                })
                if (!requestIdentity.current && inputs.length === 0) return
                if (!requestIdentity.current) {
                    requestIdentity.current = {
                        rows: inputs,
                        key: newIdempotencyKey("catalog-adoption"),
                    }
                }
                const result = await mutation.mutateAsync({
                    connectionId,
                    previewId: preview.preview_id,
                    expected_version: preview.version,
                    idempotency_key: requestIdentity.current.key,
                    rows: requestIdentity.current.rows,
                })
                if (!result.rows.some((row) => row.retry_original))
                    requestIdentity.current = null
                const combined = Array.from(
                    new Map(
                        [...successes, ...result.rows].map((row) => [
                            row.item_id,
                            row,
                        ]),
                    ).values(),
                )
                setRowResults(combined)
                if (
                    combined.length === value.rows.length &&
                    combined.every((row) => row.status === "SUCCEEDED")
                ) {
                    onComplete(combined)
                } else {
                    const failedId = result.rows.find(
                        (row) => row.status === "FAILED",
                    )?.item_id
                    const failedIndex = value.rows.findIndex(
                        (row) => row.itemId === failedId,
                    )
                    if (failedIndex >= 0) setActiveIndex(failedIndex)
                }
            } catch {
                // 保留本批次填写内容，失败原因由 mutation.error 展示。
            }
        },
    })
    React.useEffect(() => {
        if (!categories.data || categoryPrefillDone.current) return
        categoryPrefillDone.current = true
        const currentRows = form.state.values.rows
        rows.forEach(({ item }, index) => {
            const row = currentRows[index]
            const name = item.category_name?.trim()
            if (
                !row ||
                !name ||
                row.kind !== "CREATE" ||
                row.categoryId ||
                editedCategories.current.has(item.id)
            )
                return
            const matches = categories.data.rows.filter(
                (category) =>
                    category.name.trim() === name &&
                    category.productKind ===
                        PRODUCT_KIND_LABELS[row.productKind],
            )
            if (matches.length === 1) {
                form.setFieldValue(
                    `rows[${index}].categoryId`,
                    matches[0].stableId,
                    { dontUpdateMeta: true },
                )
            }
        })
    }, [categories.data, form, rows])

    const values = useStore(form.store, (state) => state.values)
    const resolutions = useQueries({
        queries: values.rows.map((row) => ({
            queryKey: [
                "supplier-api-connections",
                "adoption-resolution",
                connectionId,
                preview.preview_id,
                row.itemId,
                rowMapping(row),
            ],
            queryFn: () =>
                resolveAdoption(
                    connectionId,
                    preview.preview_id,
                    row.itemId,
                    rowMapping(row),
                ),
            enabled:
                Boolean(row.unitId) &&
                !rowResults.some(
                    (result) =>
                        result.item_id === row.itemId &&
                        result.status === "SUCCEEDED",
                ),
            retry: false,
        })),
    })
    React.useEffect(() => {
        resolutions.forEach((result, index) => {
            const kind =
                result.data?.kind === "EXISTING" ? "EXISTING" : "CREATE"
            if (form.state.values.rows[index]?.kind !== kind)
                form.setFieldValue(`rows[${index}].kind`, kind, {
                    dontUpdateMeta: true,
                })
        })
    }, [resolutions, form])
    const rowStates = React.useMemo<AdoptionRowState[]>(() => {
        const succeeded = new Set(
            rowResults
                .filter((row) => row.status === "SUCCEEDED")
                .map((row) => row.item_id),
        )
        const failed = new Set(
            rowResults
                .filter((row) => row.status === "FAILED")
                .map((row) => row.item_id),
        )
        return values.rows.map((row, index) => {
            if (succeeded.has(row.itemId)) return "SUCCEEDED"
            return rowSchema.safeParse(row).success &&
                resolutions[index]?.data &&
                !resolutions[index]?.isFetching &&
                !resolutions[index]?.isError
                ? "READY"
                : failed.has(row.itemId)
                  ? "FAILED"
                  : "INCOMPLETE"
        })
    }, [rowResults, values.rows, resolutions])
    const active = values.rows[activeIndex]
    const target = resolutions[activeIndex]
    const markCategoryEdited = (itemId: string) => {
        editedCategories.current.add(itemId)
    }
    const applyShared = () => {
        if (!active) return
        for (const row of form.state.values.rows) markCategoryEdited(row.itemId)
        const shared = [
            "categoryId",
            "unitId",
            "productKind",
            "bulkPrice",
            "minimumQuantity",
            "inputTaxPercentage",
            "validFrom",
            "validTo",
            "deliveryTemplateId",
        ] as const
        form.setFieldValue("rows", (current) =>
            current.map((row, index) => {
                if (
                    index === activeIndex ||
                    rowResults.some(
                        (result) =>
                            result.item_id === row.itemId &&
                            result.status === "SUCCEEDED",
                    )
                )
                    return row
                const next = { ...row }
                for (const field of shared) {
                    if (field === "productKind")
                        next.productKind = active.productKind
                    else if (field !== "unitId" || next.kind === "CREATE")
                        next[field] = active[field]
                }
                return next
            }),
        )
    }
    return {
        form,
        values,
        rowStates,
        active,
        activeIndex,
        setActiveIndex,
        mutation,
        categories,
        units,
        target,
        applyShared,
        markCategoryEdited,
        rowResults,
        hasUnresolvedRequest: requestIdentity.current !== null,
    }
}
