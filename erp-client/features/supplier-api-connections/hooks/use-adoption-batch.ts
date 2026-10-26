"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { format } from "date-fns"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useUnitOptionsQuery } from "@/hooks/use-options"
import { rateFromPercentage } from "@/features/supplier-offerings/lib/offering-forms"
import type { ProductKind } from "@/features/master-data/types"
import type {
    AdoptionPreview,
    AdoptionResultRow,
    AdoptionRowInput,
} from "../api/adoption"
import { newIdempotencyKey } from "../lib/operations"
import { useAdoptionTargetQuery, useConfirmAdoptionMutation } from "./adoption"

export type AdoptionRowState = "SUCCEEDED" | "FAILED" | "READY" | "INCOMPLETE"

type RowDraft = {
    itemId: string
    kind: "CREATE" | "EXISTING"
    categoryId: string
    brandId: string
    unitId: string
    productKind: ProductKind
    companyName: string
    companyDescription: string
    skuId: string
    skuVersion: number
    productVersion: number
    specification: string
    confirmed: boolean
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
        brandId: z.string(),
        unitId: z.string().min(1, "请选择公司计量单位"),
        productKind: z.enum([
            "PHYSICAL",
            "VIRTUAL",
            "OFFLINE_SERVICE",
            "VOUCHER",
        ]),
        companyName: z.string(),
        companyDescription: z.string(),
        skuId: z.string(),
        skuVersion: z.number(),
        productVersion: z.number(),
        specification: z.string(),
        confirmed: z.boolean().refine(Boolean, "请核对来源规格、包装及单位"),
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
            if (!row.categoryId)
                ctx.addIssue({
                    code: "custom",
                    path: ["categoryId"],
                    message: "请选择公司分类",
                })
            if (!row.brandId)
                ctx.addIssue({
                    code: "custom",
                    path: ["brandId"],
                    message: "请选择公司品牌",
                })
        } else if (!row.skuId || !row.skuVersion || !row.productVersion) {
            ctx.addIssue({
                code: "custom",
                path: ["skuId"],
                message: "请选择公司 SKU 并等待资料核对完成",
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
                              : "请填写有效的非负采购价",
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

function rowInput(row: RowDraft): AdoptionRowInput {
    return {
        item_id: row.itemId,
        target:
            row.kind === "CREATE"
                ? {
                      kind: "CREATE",
                      category_id: row.categoryId,
                      brand_id: row.brandId,
                      unit_id: row.unitId,
                      product_kind: row.productKind,
                      company_name: row.companyName.trim(),
                      company_description: row.companyDescription.trim(),
                      spec_entries: row.specification.trim()
                          ? [
                                {
                                    attribute_code: "规格",
                                    attribute_value_code:
                                        row.specification.trim(),
                                },
                            ]
                          : [],
                      unit_confirmed: row.confirmed,
                      specification_confirmed: row.confirmed,
                  }
                : {
                      kind: "EXISTING",
                      sku_id: row.skuId,
                      expected_sku_version: row.skuVersion,
                      expected_product_version: row.productVersion,
                      unit_id: row.unitId,
                      specification_confirmed: row.confirmed,
                  },
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
    const requestIdentity = React.useRef<{
        payload: string
        key: string
    } | null>(null)
    const [rowResults, setRowResults] = React.useState<AdoptionResultRow[]>([])
    const mutation = useConfirmAdoptionMutation()
    const categories = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "enabled",
        revisionTiming: "current",
    })
    const brands = useMasterDataListQuery({
        resource: "brands",
        lifecycleStatus: "enabled",
        revisionTiming: "current",
    })
    const units = useUnitOptionsQuery()
    const form = useAppForm({
        defaultValues: {
            rows: rows.map(({ item }): RowDraft => ({
                itemId: item.id,
                kind: item.sku_id ? "EXISTING" : "CREATE",
                categoryId: "",
                brandId: "",
                unitId: "",
                productKind: "PHYSICAL",
                companyName: item.name,
                companyDescription: item.description ?? "",
                skuId: item.sku_id ?? "",
                skuVersion: 0,
                productVersion: 0,
                specification: item.specification,
                confirmed: false,
                dropshipPrice: item.purchase_price ?? "",
                bulkPrice: "",
                minimumQuantity: "",
                inputTaxPercentage: "",
                validFrom: format(new Date(), "yyyy-MM-dd"),
                validTo: "",
                deliveryTemplateId: "",
            })),
        },
        validators: { onSubmit: z.object({ rows: z.array(rowSchema) }) },
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
                const inputs = value.rows
                    .filter((row) => !succeededIds.has(row.itemId))
                    .map(rowInput)
                const payload = JSON.stringify(inputs)
                if (requestIdentity.current?.payload !== payload) {
                    requestIdentity.current = {
                        payload,
                        key: newIdempotencyKey("catalog-adoption"),
                    }
                }
                const result = await mutation.mutateAsync({
                    connectionId,
                    previewId: preview.preview_id,
                    expected_version: preview.version,
                    idempotency_key: requestIdentity.current.key,
                    rows: inputs,
                })
                const combined = [...successes, ...result.rows]
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
    const values = useStore(form.store, (state) => state.values)
    const isDirty = useStore(form.store, (state) => state.isDirty)
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
        return values.rows.map((row) => {
            if (succeeded.has(row.itemId)) return "SUCCEEDED"
            if (failed.has(row.itemId)) return "FAILED"
            return rowSchema.safeParse(row).success ? "READY" : "INCOMPLETE"
        })
    }, [rowResults, values.rows])
    const active = values.rows[activeIndex]
    const target = useAdoptionTargetQuery(
        connectionId,
        active?.kind === "EXISTING" ? active.skuId : "",
    )
    React.useEffect(() => {
        if (
            !target.data ||
            active?.kind !== "EXISTING" ||
            active.skuId !== target.data.sku_id
        )
            return
        if (
            (active.skuVersion !== 0 &&
                active.skuVersion !== target.data.sku_version) ||
            (active.productVersion !== 0 &&
                active.productVersion !== target.data.product_version)
        )
            form.setFieldValue(`rows[${activeIndex}].confirmed`, false)
        form.setFieldValue(`rows[${activeIndex}].unitId`, target.data.unit_id)
        form.setFieldValue(
            `rows[${activeIndex}].skuVersion`,
            target.data.sku_version,
        )
        form.setFieldValue(
            `rows[${activeIndex}].productVersion`,
            target.data.product_version,
        )
    }, [
        target.data,
        active?.kind,
        active?.skuId,
        active?.skuVersion,
        active?.productVersion,
        activeIndex,
        form,
    ])
    const applyShared = () => {
        if (!active) return
        const shared = [
            "categoryId",
            "brandId",
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
                const next = { ...row, confirmed: false }
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
        isDirty,
        rowStates,
        active,
        activeIndex,
        setActiveIndex,
        mutation,
        categories,
        brands,
        units,
        target,
        applyShared,
        rowResults,
    }
}
