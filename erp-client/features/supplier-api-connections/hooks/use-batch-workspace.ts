"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { divideFixed, multiplyFixed, parseDecimal } from "@/lib/fixed-decimal"
import type {
    BatchDefaults,
    BatchPricing,
    BatchStep,
    BatchView,
    PriceRule,
    SaveBatchRequest,
} from "../api/batch"
import {
    usePreviewAdoptionBatchMutation,
    useSaveAdoptionBatchMutation,
} from "./batch"

export type PricingMode = BatchPricing["mode"]
export type RuleMethod = PriceRule["method"]
export type BatchWorkspaceDraft = {
    mode: PricingMode
    inactivePricing: BatchPricing | null
    dropshipMethod: RuleMethod
    dropshipValue: string
    bulkMethod: RuleMethod
    bulkValue: string
    minimumQuantity: string
    overwrite: boolean
    preserveManual: boolean
    dropshipBulkAction: "KEEP" | "CLEAR"
    categoryMappings: Array<{
        sourceId: string
        sourceName: string
        count: number
        categoryId: string
    }>
    inputTaxPercentage: string
    validFrom: string
    validTo: string
    deliveryTemplateId: string
    clearFields: string[]
}
const rule = (method: RuleMethod, value: string): PriceRule =>
    method === "UNSET" ? { method } : { method, value: value.trim() }
export function draftPricing(value: BatchWorkspaceDraft): BatchPricing {
    const dropship = rule(value.dropshipMethod, value.dropshipValue)
    if (value.mode === "DROPSHIP_ONLY") return { mode: value.mode, dropship }
    const minimum_quantity = value.minimumQuantity.trim() || null
    if (value.mode === "SAME")
        return { mode: value.mode, rule: dropship, minimum_quantity }
    return {
        mode: value.mode,
        dropship,
        bulk: rule(value.bulkMethod, value.bulkValue),
        minimum_quantity,
    }
}
export function batchManualImpact(
    view: BatchView,
    draft: BatchWorkspaceDraft,
): number {
    if (!draft.overwrite || draft.preserveManual) return 0
    return view.rows.filter((row) => {
        if (
            !["DRAFT", "READY", "FAILED"].includes(row.status) ||
            row.operation_id ||
            row.resolution?.kind === "EXISTING"
        )
            return false
        const fields = [
            ...(draft.dropshipMethod !== "UNSET"
                ? [row.sales_prices.dropship]
                : []),
            ...(draft.mode === "SAME" ||
            (draft.mode === "SEPARATE" && draft.bulkMethod !== "UNSET") ||
            (draft.mode === "DROPSHIP_ONLY" &&
                draft.dropshipBulkAction === "CLEAR")
                ? [row.sales_prices.bulk]
                : []),
            ...(draft.mode !== "DROPSHIP_ONLY" && draft.minimumQuantity
                ? [row.sales_prices.minimum_quantity]
                : []),
        ]
        return fields.some(
            (field) => field.origin === "MANUAL" || field.origin === "CLEARED",
        )
    }).length
}
function draftFrom(view: BatchView): BatchWorkspaceDraft {
    const pricing = view.batch.rules
    const dropship = pricing.mode === "SAME" ? pricing.rule : pricing.dropship
    const inactive = view.batch.inactive_pricing
    const bulk =
        pricing.mode === "SEPARATE"
            ? pricing.bulk
            : inactive?.mode === "SEPARATE"
              ? inactive.bulk
              : dropship
    const groups = new Map<
        string,
        {
            sourceId: string
            sourceName: string
            count: number
            categoryId: string
        }
    >()
    for (const row of view.rows) {
        const sourceId = row.source.category_id ?? ""
        const current = groups.get(sourceId)
        if (current) current.count += 1
        else
            groups.set(sourceId, {
                sourceId,
                sourceName: row.source.category_name ?? "来源未分类",
                count: 1,
                categoryId:
                    view.batch.defaults.category_mappings?.find(
                        (entry) =>
                            (entry.source_category_id ?? "") === sourceId,
                    )?.category_id ??
                    view.batch.defaults.category_id ??
                    "",
            })
    }
    const tax = view.batch.defaults.input_tax_rate
    return {
        mode: pricing.mode,
        inactivePricing: inactive ?? null,
        dropshipMethod: dropship.method,
        dropshipValue: dropship.method === "UNSET" ? "" : dropship.value,
        bulkMethod: bulk.method,
        bulkValue: bulk.method === "UNSET" ? "" : bulk.value,
        minimumQuantity:
            pricing.mode === "DROPSHIP_ONLY"
                ? inactive && inactive.mode !== "DROPSHIP_ONLY"
                    ? (inactive.minimum_quantity ?? "")
                    : ""
                : (pricing.minimum_quantity ?? ""),
        overwrite: view.batch.fill_policy.overwrite,
        preserveManual: view.batch.fill_policy.preserve_manual,
        dropshipBulkAction: "KEEP",
        categoryMappings: [...groups.values()],
        inputTaxPercentage: tax
            ? multiplyFixed(tax, "100", {
                  leftMaxScale: 6,
                  rightMaxScale: 0,
                  outputScale: 4,
              })
            : "",
        validFrom: view.batch.defaults.valid_from ?? "",
        validTo: view.batch.defaults.valid_to ?? "",
        deliveryTemplateId: view.batch.defaults.delivery_template_id ?? "",
        clearFields: view.batch.defaults.clear_fields ?? [],
    }
}
const draftSchema = z
    .custom<BatchWorkspaceDraft>()
    .superRefine((value, context) => {
        const validateRule = (
            method: RuleMethod,
            input: string,
            field: string,
        ) => {
            if (method === "UNSET") return
            try {
                parseDecimal(input, {
                    maxScale: method === "PERCENTAGE" ? 4 : 2,
                })
            } catch {
                context.addIssue({
                    code: "custom",
                    path: [field],
                    message:
                        method === "PERCENTAGE"
                            ? "请填写非负比例，最多四位小数"
                            : "请填写非负金额，最多两位小数",
                })
            }
        }
        validateRule(value.dropshipMethod, value.dropshipValue, "dropshipValue")
        if (value.mode === "SEPARATE")
            validateRule(value.bulkMethod, value.bulkValue, "bulkValue")
        if (value.mode !== "DROPSHIP_ONLY" && value.minimumQuantity) {
            try {
                if (
                    parseDecimal(value.minimumQuantity, { maxScale: 6 })
                        .unscaled <= BigInt(0)
                )
                    throw new Error("INVALID")
            } catch {
                context.addIssue({
                    code: "custom",
                    path: ["minimumQuantity"],
                    message: "公司集采起订量须大于零，最多六位小数",
                })
            }
        }
        if (
            value.mode !== "DROPSHIP_ONLY" &&
            (value.mode === "SAME"
                ? value.dropshipMethod !== "UNSET"
                : value.bulkMethod !== "UNSET") &&
            !value.minimumQuantity
        ) {
            context.addIssue({
                code: "custom",
                path: ["minimumQuantity"],
                message: "设置公司集采价时，请同时填写公司集采起订量",
            })
        }
        if (value.inputTaxPercentage) {
            try {
                const tax = parseDecimal(value.inputTaxPercentage, {
                    maxScale: 4,
                })
                if (
                    tax.unscaled >
                    BigInt(100) * BigInt(10) ** BigInt(tax.scale)
                )
                    throw new Error("INVALID")
            } catch {
                context.addIssue({
                    code: "custom",
                    path: ["inputTaxPercentage"],
                    message: "进项税率应在 0% 至 100% 之间，最多四位小数",
                })
            }
        }
        if (
            value.validFrom &&
            value.validTo &&
            value.validTo <= value.validFrom
        )
            context.addIssue({
                code: "custom",
                path: ["validTo"],
                message: "失效日期须晚于生效日期",
            })
    })
export function useBatchWorkspace(view: BatchView) {
    const identity = {
        connectionId: view.batch.connection_id,
        batchId: view.batch.id,
    }
    const save = useSaveAdoptionBatchMutation()
    const preview = usePreviewAdoptionBatchMutation()
    const [reviewedVersion, setReviewedVersion] = React.useState<number | null>(
        null,
    )
    const nextStep = React.useRef<BatchStep | undefined>(undefined)
    const previewAfterSave = React.useRef(false)
    const savedSuccessfully = React.useRef(false)
    const form = useAppForm({
        defaultValues: draftFrom(view),
        validators: { onSubmit: draftSchema },
        onSubmit: async ({ value }) => {
            const defaults: BatchDefaults = {
                category_id: null,
                category_mappings: value.categoryMappings
                    .filter((entry) => entry.categoryId)
                    .map((entry) => ({
                        source_category_id: entry.sourceId || null,
                        category_id: entry.categoryId,
                    })),
                input_tax_rate: value.inputTaxPercentage.trim()
                    ? divideFixed(value.inputTaxPercentage, "100", {
                          numeratorMaxScale: 4,
                          denominatorMaxScale: 0,
                          outputScale: 6,
                      })
                    : null,
                valid_from: value.validFrom || null,
                valid_to: value.validTo || null,
                delivery_template_id: value.deliveryTemplateId || null,
                clear_fields: value.clearFields,
            }
            const request: SaveBatchRequest = {
                expected_version: view.batch.version,
                rules: draftPricing(value),
                inactive_pricing: value.inactivePricing,
                fill_policy: {
                    overwrite: value.overwrite,
                    preserve_manual: value.preserveManual,
                },
                defaults,
                step: nextStep.current,
                items:
                    value.mode === "DROPSHIP_ONLY" &&
                    value.dropshipBulkAction === "CLEAR"
                        ? view.rows
                              .filter(
                                  (row) =>
                                      ["DRAFT", "READY", "FAILED"].includes(
                                          row.status,
                                      ) &&
                                      !row.operation_id &&
                                      row.resolution?.kind !== "EXISTING" &&
                                      (!value.preserveManual ||
                                          ![
                                              row.sales_prices.bulk.origin,
                                              row.sales_prices.minimum_quantity
                                                  .origin,
                                          ].some((origin) =>
                                              [
                                                  "MANUAL",
                                                  "CLEARED",
                                                  "PRESERVED",
                                              ].includes(origin),
                                          )) &&
                                      (row.sales_prices.bulk.value != null ||
                                          row.sales_prices.minimum_quantity
                                              .value != null),
                              )
                              .map((row) => ({
                                  item_id: row.source.id,
                                  sales_edit: {
                                      bulk:
                                          value.preserveManual &&
                                          [
                                              "MANUAL",
                                              "CLEARED",
                                              "PRESERVED",
                                          ].includes(
                                              row.sales_prices.bulk.origin,
                                          )
                                              ? { action: "KEEP" as const }
                                              : { action: "CLEAR" as const },
                                      minimum_quantity:
                                          value.preserveManual &&
                                          [
                                              "MANUAL",
                                              "CLEARED",
                                              "PRESERVED",
                                          ].includes(
                                              row.sales_prices.minimum_quantity
                                                  .origin,
                                          )
                                              ? { action: "KEEP" as const }
                                              : { action: "CLEAR" as const },
                                  },
                              }))
                        : [],
            }
            const saved = await save.mutateAsync({ ...identity, request })
            form.reset(draftFrom(saved))
            if (previewAfterSave.current) {
                const checked = await preview.mutateAsync({
                    ...identity,
                    expected_version: saved.batch.version,
                })
                form.reset(draftFrom(checked))
                setReviewedVersion(checked.batch.preview_version)
            }
            savedSuccessfully.current = true
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const dirty = useStore(form.store, (state) => !state.isDefaultValue)
    const persist = async (step?: BatchStep, check = false) => {
        nextStep.current = step
        previewAfterSave.current = check
        savedSuccessfully.current = false
        try {
            await form.handleSubmit()
        } catch {
            /* 输入保留，错误在当前页面显示。 */
        }
        return savedSuccessfully.current
    }
    const acceptSaved = (saved: BatchView) => form.reset(draftFrom(saved))
    const acceptPreview = (checked: BatchView) => {
        acceptSaved(checked)
        setReviewedVersion(checked.batch.preview_version)
    }
    return {
        form,
        values,
        dirty,
        persist,
        acceptSaved,
        acceptPreview,
        requireReview: () => setReviewedVersion(null),
        hasCurrentReview:
            reviewedVersion != null &&
            reviewedVersion === view.batch.preview_version,
        save,
        preview,
        busy: save.isPending || preview.isPending,
    }
}
