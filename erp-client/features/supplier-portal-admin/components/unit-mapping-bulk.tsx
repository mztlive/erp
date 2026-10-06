"use client"

import { useState } from "react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type {
    NewProductInput,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

type Sku = NewProductInput["skus"][number]
type UnitMapping = {
    rowId: string
    unitId: string
    unitVersion: number
    unitSynonymConfirmed: boolean
    unitSynonymReason: string
    target: string
}
type ConfirmedSource = {
    sku: Sku
    unit: PortalDictionary
    unitVersion: number
    unitSynonymConfirmed: boolean
    unitSynonymReason: string
}

function confirmedSource(
    sku: Sku,
    mappings: UnitMapping[],
    units: PortalDictionary[],
): ConfirmedSource | null {
    const mapping = mappings.find((row) => row.rowId === sku.row_id)
    const unit = units.find((item) => item.id === mapping?.unitId)
    if (!mapping || !unit || mapping.unitVersion !== unit.version) return null
    const reason = mapping.unitSynonymReason.trim()
    if (
        (sku.unit.raw_name !== unit.name || mapping.unitSynonymConfirmed) &&
        (!mapping.unitSynonymConfirmed || !reason)
    )
        return null
    if (
        (sku.unit.selected_id || sku.unit.expected_version != null) &&
        (sku.unit.selected_id !== unit.id ||
            sku.unit.expected_version !== mapping.unitVersion)
    )
        return null
    if (
        !sku.unit.raw_name.trim() ||
        (sku.packaging &&
            (!sku.packaging.conversion_confirmed_by_supplier ||
                sku.packaging.base_unit.trim() !== sku.unit.raw_name.trim()))
    )
        return null
    return {
        sku,
        unit,
        unitVersion: mapping.unitVersion,
        unitSynonymConfirmed: mapping.unitSynonymConfirmed,
        unitSynonymReason: mapping.unitSynonymConfirmed ? reason : "",
    }
}

function samePackaging(source: Sku, target: Sku): boolean {
    if (source.quote_basis !== target.quote_basis) return false
    if (!source.packaging || !target.packaging)
        return source.packaging === target.packaging
    return (
        source.packaging.original_unit === target.packaging.original_unit &&
        source.packaging.base_unit === target.packaging.base_unit &&
        source.packaging.units_per_package ===
            target.packaging.units_per_package &&
        source.packaging.original_unit_price ===
            target.packaging.original_unit_price &&
        source.packaging.conversion_confirmed_by_supplier ===
            target.packaging.conversion_confirmed_by_supplier
    )
}

function applicability(
    sku: Sku,
    source: ConfirmedSource | null,
    mappings: UnitMapping[],
): { eligible: boolean; label: string } {
    if (!source) return { eligible: false, label: "请先选择已确认的来源规格" }
    if (sku.row_id === source.sku.row_id)
        return { eligible: false, label: "来源规格，单位已确认" }
    if (sku.unit.raw_name !== source.sku.unit.raw_name)
        return { eligible: false, label: "单位原文不同，请逐行核对" }
    if (!samePackaging(source.sku, sku))
        return { eligible: false, label: "包装或报价口径不同，请逐行核对" }
    if (
        (sku.unit.selected_id || sku.unit.expected_version != null) &&
        (sku.unit.selected_id !== source.unit.id ||
            sku.unit.expected_version !== source.unitVersion)
    )
        return { eligible: false, label: "供应商已选单位不一致，请逐行核对" }
    const mapping = mappings.find((row) => row.rowId === sku.row_id)
    if (!mapping)
        return { eligible: false, label: "规格映射已变化，请重新读取申请" }
    if (mapping.unitId)
        return {
            eligible: false,
            label:
                mapping.unitId === source.unit.id &&
                mapping.unitVersion === source.unitVersion
                    ? "已确认相同单位，无需再次应用"
                    : "已有单位映射，请逐行核对",
        }
    return { eligible: true, label: "可应用，需明确勾选" }
}

function packagingLabel(sku: Sku): string {
    if (!sku.packaging) return sku.quote_basis || "未填写包装关系"
    const packaging = sku.packaging
    return `${packaging.original_unit} → ${packaging.base_unit} · 每包装 ${packaging.units_per_package} ${packaging.base_unit} · ${packaging.conversion_confirmed_by_supplier ? "供应商已确认" : "供应商未确认"}`
}

export function PortalUnitMappingBulk({
    skus,
    units,
    mappings,
    disabled,
    onApply,
}: {
    skus: NewProductInput["skus"]
    units: PortalDictionary[]
    mappings: UnitMapping[]
    disabled: boolean
    onApply: (
        updates: {
            rowId: string
            unitId: string
            unitVersion: number
            unitSynonymConfirmed: boolean
            unitSynonymReason: string
        }[],
    ) => void
}) {
    const [feedback, setFeedback] = useState("")
    const sources = skus.flatMap((sku) => {
        const source = confirmedSource(sku, mappings, units)
        return source ? [source] : []
    })
    const form = useAppForm({
        defaultValues: { sourceRowId: "", selectedRowIds: [] as string[] },
        onSubmit: ({ value }) => {
            if (disabled) return
            const source = sources.find(
                (item) => item.sku.row_id === value.sourceRowId,
            )
            if (!source) {
                setFeedback("来源单位已变化，请重新核对并选择来源规格。")
                return
            }
            const selected = skus.filter((sku) =>
                value.selectedRowIds.includes(sku.row_id),
            )
            if (
                !selected.length ||
                selected.length !== value.selectedRowIds.length ||
                selected.some(
                    (sku) => !applicability(sku, source, mappings).eligible,
                )
            ) {
                setFeedback("所选规格的适用条件已变化，请重新核对并勾选。")
                return
            }
            onApply(
                selected.map((sku) => ({
                    rowId: sku.row_id,
                    unitId: source.unit.id,
                    unitVersion: source.unitVersion,
                    unitSynonymConfirmed: source.unitSynonymConfirmed,
                    unitSynonymReason: source.unitSynonymReason,
                })),
            )
            form.setFieldValue("selectedRowIds", [])
            setFeedback(
                `已将 ${source.unit.name} 应用到 ${selected.length} 个规格。`,
            )
        },
    })
    return (
        <section className="space-y-3 rounded-lg border p-4">
            <h3 className="font-medium">批量确认相同单位</h3>
            <p className="text-sm text-muted-foreground">
                先逐行确认一个来源规格；原始单位与目标单位名称不同时，必须勾选同义确认并填写核对依据。再明确勾选单位原文、包装和报价口径完全一致的未匹配规格，回填单位引用及已确认的同义依据，保留供应商原稿、包装、价格及数量。
            </p>
            <form.AppField name="sourceRowId">
                {(field) => (
                    <field.SelectField
                        id="supplier-portal-review-unit-bulk-source"
                        label="已确认的来源规格及目标单位"
                        options={sources.map((source) => ({
                            value: source.sku.row_id,
                            label: `${source.sku.name} · ${source.sku.ordering_code} → ${source.unit.name}`,
                        }))}
                        disabled={disabled || !sources.length}
                        placeholder="请选择已确认单位的来源规格"
                        emptyLabel="请先逐行确认有效单位及包装口径"
                        onValueChange={() => {
                            form.setFieldValue("selectedRowIds", [])
                            setFeedback("")
                        }}
                    />
                )}
            </form.AppField>
            {!sources.length && (
                <p className="text-sm text-muted-foreground">
                    暂无可用来源。请先逐行确认有效单位及必要的同义依据；单位版本变化、同义核对不完整或包装换算未经供应商确认时，不能批量应用。
                </p>
            )}
            <form.Subscribe selector={(state) => state.values}>
                {(value) => {
                    const source =
                        sources.find(
                            (item) => item.sku.row_id === value.sourceRowId,
                        ) ?? null
                    const eligibleSelection = value.selectedRowIds.filter(
                        (rowId) => {
                            const sku = skus.find((row) => row.row_id === rowId)
                            return (
                                !!sku &&
                                applicability(sku, source, mappings).eligible
                            )
                        },
                    )
                    return (
                        <>
                            <form.Field name="selectedRowIds">
                                {(field) => (
                                    <Table>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead>选择</TableHead>
                                                <TableHead>
                                                    规格及订货编码
                                                </TableHead>
                                                <TableHead>单位原文</TableHead>
                                                <TableHead>包装口径</TableHead>
                                                <TableHead>适用情况</TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {skus.map((sku) => {
                                                const state = applicability(
                                                    sku,
                                                    source,
                                                    mappings,
                                                )
                                                const checkboxId = `supplier-portal-review-unit-bulk-select-${toAutomationIdSegment(sku.row_id)}`
                                                return (
                                                    <TableRow key={sku.row_id}>
                                                        <TableCell>
                                                            <NativeCheckbox
                                                                id={checkboxId}
                                                                aria-label={`将目标单位应用到${sku.name}，订货编码${sku.ordering_code}`}
                                                                aria-describedby={`${checkboxId}-status`}
                                                                checked={
                                                                    state.eligible &&
                                                                    field.state.value.includes(
                                                                        sku.row_id,
                                                                    )
                                                                }
                                                                disabled={
                                                                    disabled ||
                                                                    !state.eligible
                                                                }
                                                                onCheckedChange={(
                                                                    checked,
                                                                ) => {
                                                                    field.handleChange(
                                                                        checked
                                                                            ? [
                                                                                  ...field.state.value.filter(
                                                                                      (
                                                                                          id,
                                                                                      ) =>
                                                                                          id !==
                                                                                          sku.row_id,
                                                                                  ),
                                                                                  sku.row_id,
                                                                              ]
                                                                            : field.state.value.filter(
                                                                                  (
                                                                                      id,
                                                                                  ) =>
                                                                                      id !==
                                                                                      sku.row_id,
                                                                              ),
                                                                    )
                                                                    setFeedback(
                                                                        "",
                                                                    )
                                                                }}
                                                            />
                                                        </TableCell>
                                                        <TableCell>
                                                            <label
                                                                htmlFor={
                                                                    checkboxId
                                                                }
                                                            >
                                                                {sku.name} ·{" "}
                                                                {
                                                                    sku.ordering_code
                                                                }
                                                            </label>
                                                        </TableCell>
                                                        <TableCell>
                                                            {sku.unit.raw_name}
                                                        </TableCell>
                                                        <TableCell className="max-w-64 whitespace-normal">
                                                            {packagingLabel(
                                                                sku,
                                                            )}
                                                        </TableCell>
                                                        <TableCell
                                                            id={`${checkboxId}-status`}
                                                            className="max-w-64 whitespace-normal text-muted-foreground"
                                                        >
                                                            {state.label}
                                                        </TableCell>
                                                    </TableRow>
                                                )
                                            })}
                                        </TableBody>
                                    </Table>
                                )}
                            </form.Field>
                            <Button
                                id="supplier-portal-review-unit-bulk-apply"
                                type="button"
                                variant="outline"
                                disabled={
                                    disabled ||
                                    !source ||
                                    !eligibleSelection.length ||
                                    eligibleSelection.length !==
                                        value.selectedRowIds.length
                                }
                                onClick={() => void form.handleSubmit()}
                            >
                                应用到已勾选规格
                                {eligibleSelection.length > 0 &&
                                    `（${eligibleSelection.length}）`}
                            </Button>
                        </>
                    )
                }}
            </form.Subscribe>
            {feedback && (
                <p role="status" className="text-sm text-muted-foreground">
                    {feedback}
                </p>
            )}
        </section>
    )
}
