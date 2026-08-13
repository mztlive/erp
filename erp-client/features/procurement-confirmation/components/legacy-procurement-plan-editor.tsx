"use client"

import { CircleCheckIcon, PlusIcon, TriangleAlertIcon } from "lucide-react"

import { BusinessStatusBadge, OptionCombobox } from "@/components/business"
import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import type {
    ConfirmationLineDraft,
    CoverageByLine,
    FulfillmentMode,
    ProcurementConfirmationTask,
    ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"
import { money } from "@/features/procurement-confirmation/lib/format"
import {
    capabilityCodeForMode,
    supplyCostForQuantity,
} from "@/features/procurement-confirmation/lib/supply-cost"

type SelectionOption = {
    value: string
    label: string
}

type LegacyProcurementPlanEditorProps = {
    task: ProcurementConfirmationTask
    recommendation: ProcurementRecommendation | undefined
    recommendationPending: boolean
    recommendationFailed: boolean
    lineDrafts: readonly ConfirmationLineDraft[]
    coverage: readonly CoverageByLine[]
    supplyOptions: readonly ProcurementSupplyOption[]
    supplierOptions: readonly SupplierComboboxItem[] | undefined
    offeringOptionsForSku: (skuId: string) => readonly SelectionOption[]
    capabilityOptionsForOffering: (
        offeringRevisionId: string,
        fulfillmentMode: FulfillmentMode,
    ) => readonly SelectionOption[]
    fulfillmentOptionsForOffering: (
        offeringRevisionId: string,
    ) => readonly SelectionOption[]
    formalPending: boolean
    onApplyRecommendation: () => void
    onUpdateLine: (
        lineKey: string,
        patch: Partial<ConfirmationLineDraft>,
    ) => void
    onAddSplitLine: (submissionLineId: string) => void
    onRemoveLine: (lineKey: string) => void
    saveMessage: string | null
    dirty: boolean
}

export function LegacyProcurementPlanEditor({
    task,
    recommendation,
    recommendationPending,
    recommendationFailed,
    lineDrafts,
    coverage,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    formalPending,
    onApplyRecommendation,
    onUpdateLine,
    onAddSplitLine,
    onRemoveLine,
    saveMessage,
    dirty,
}: LegacyProcurementPlanEditorProps) {
    const recommendationQuery = {
        isPending: recommendationPending,
        isError: recommendationFailed,
    }
    const applyRecommendation = onApplyRecommendation
    const updateLine = onUpdateLine
    const addSplitLine = onAddSplitLine
    const removeLine = onRemoveLine

    return (
        <Card className="hidden min-w-0" size="sm">
            <CardHeader className="border-b">
                <div className="flex flex-wrap items-center gap-2">
                    <CardTitle>
                        <h2
                            tabIndex={-1}
                            className="outline-none"
                            aria-live="polite"
                        >
                            最低成本采购方案
                        </h2>
                    </CardTitle>
                    <BusinessStatusBadge
                        context="list"
                        label={recommendation?.ready ? "可执行" : "待补齐"}
                        tone={recommendation?.ready ? "success" : "warning"}
                    />
                    <Badge variant="secondary">系统推荐 · 可人工调整</Badge>
                </div>
                <CardDescription>
                    按供应商当前有效供给、能力、起订量、可供量、采购价及费用组合；审批时系统重新校验。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-5">
                <Alert
                    variant={
                        recommendationQuery.isError
                            ? "destructive"
                            : recommendation?.ready
                              ? "success"
                              : "warning"
                    }
                >
                    {recommendation?.ready ? (
                        <CircleCheckIcon aria-hidden="true" />
                    ) : (
                        <TriangleAlertIcon aria-hidden="true" />
                    )}
                    <AlertTitle>
                        {recommendationQuery.isPending
                            ? "正在计算最低成本方案"
                            : recommendationQuery.isError
                              ? "最低成本方案计算失败"
                              : recommendation?.ready
                                ? `已组合 ${recommendation.purchaseOrders.length} 张采购单草稿`
                                : "当前无法形成完整采购方案"}
                    </AlertTitle>
                    <AlertDescription>
                        {recommendation?.ready
                            ? `预计采购含税 ${money.format(Number(recommendation.estimatedPurchaseGross))}，预计毛利 ${money.format(Number(recommendation.estimatedGrossMargin))}。交期仍需采购核对。`
                            : recommendation?.blockingIssues
                                  .map((issue) => issue.message)
                                  .join("；") ||
                              "请等待系统完成计算，或刷新后重试。"}
                    </AlertDescription>
                </Alert>

                <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">
                        推荐策略：
                        {recommendation?.policyVersion ?? "正在读取"}
                    </p>
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                            formalPending ||
                            recommendationQuery.isPending ||
                            !recommendation?.ready
                        }
                        onClick={applyRecommendation}
                    >
                        重新载入最低成本方案
                    </Button>
                </div>

                <Separator />

                <section aria-labelledby="confirm-lines-title">
                    <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                        <h3
                            id="confirm-lines-title"
                            className="text-sm font-semibold"
                        >
                            采购明细
                        </h3>
                        <span className="text-xs text-muted-foreground">
                            同一商品可向多个供应商采购；每条销售明细分别核对数量
                        </span>
                    </div>

                    <div className="space-y-5">
                        {task.salesSubmission.lines.map((subLine) => {
                            const lines = lineDrafts.filter(
                                (l) =>
                                    l.submissionLineId ===
                                    subLine.submissionLineId,
                            )
                            const cov = coverage.find(
                                (c) =>
                                    c.submissionLineId ===
                                    subLine.submissionLineId,
                            )
                            return (
                                <div
                                    key={subLine.submissionLineId}
                                    id={`submission-line-${subLine.submissionLineId}`}
                                    className="rounded-xl border border-border"
                                    tabIndex={-1}
                                >
                                    <div className="flex flex-wrap items-start justify-between gap-2 border-b border-border bg-muted/40 px-3 py-2">
                                        <div>
                                            <p className="text-sm font-medium">
                                                {subLine.itemName}
                                            </p>
                                            <p className="text-xs text-muted-foreground">
                                                承诺{" "}
                                                <span className="num">
                                                    {subLine.committedQuantity}{" "}
                                                    {subLine.unit}
                                                </span>{" "}
                                                · 客户期望{" "}
                                                {subLine.requestedDeliveryDate}
                                                {subLine.referenceSupplier
                                                    ? ` · 参考 ${subLine.referenceSupplier} / ${money.format(Number(subLine.referenceCost))}`
                                                    : null}
                                            </p>
                                        </div>
                                        <div
                                            className="text-right text-xs"
                                            aria-live="polite"
                                        >
                                            <Badge
                                                variant={
                                                    cov?.complete
                                                        ? "secondary"
                                                        : "destructive"
                                                }
                                            >
                                                覆盖 {cov?.confirmed}/
                                                {cov?.required} {subLine.unit}
                                                {cov && !cov.complete
                                                    ? ` · 缺口 ${cov.gap} ${subLine.unit}`
                                                    : " · 完整"}
                                            </Badge>
                                        </div>
                                    </div>

                                    <div className="overflow-x-auto">
                                        <table className="w-full min-w-[40rem] text-sm">
                                            <caption className="sr-only">
                                                {subLine.itemName} 采购明细
                                            </caption>
                                            <thead>
                                                <tr className="border-b border-border text-left text-xs text-muted-foreground">
                                                    <th className="px-3 py-2 font-medium">
                                                        供应商
                                                    </th>
                                                    <th className="px-3 py-2 font-medium num">
                                                        确认数量
                                                    </th>
                                                    <th className="px-3 py-2 font-medium num">
                                                        含税成本
                                                    </th>
                                                    <th className="hidden px-3 py-2 font-medium num md:table-cell">
                                                        进项税率
                                                    </th>
                                                    <th className="hidden px-3 py-2 font-medium sm:table-cell">
                                                        预计交期
                                                    </th>
                                                    <th className="px-3 py-2 font-medium">
                                                        交付方式
                                                    </th>
                                                    <th className="hidden px-3 py-2 font-medium lg:table-cell">
                                                        供应资质
                                                    </th>
                                                    <th className="px-3 py-2 font-medium text-right">
                                                        操作
                                                    </th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {lines.map((line) => (
                                                    <tr
                                                        key={line.lineKey}
                                                        className="border-b border-border last:border-0"
                                                    >
                                                        <td className="px-3 py-2">
                                                            <OptionCombobox
                                                                value={
                                                                    line.offeringRevisionId ||
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    revisionId,
                                                                ) => {
                                                                    const offering =
                                                                        supplyOptions.find(
                                                                            (
                                                                                option,
                                                                            ) =>
                                                                                option.offeringRevisionId ===
                                                                                revisionId,
                                                                        )
                                                                    const supplier =
                                                                        supplierOptions?.find(
                                                                            (
                                                                                option,
                                                                            ) =>
                                                                                option.supplierId ===
                                                                                offering?.supplierId,
                                                                        )
                                                                    const onlyCapability =
                                                                        offering?.capabilities.filter(
                                                                            (
                                                                                capability,
                                                                            ) =>
                                                                                capability.capabilityCode ===
                                                                                capabilityCodeForMode(
                                                                                    line.fulfillmentMode,
                                                                                ),
                                                                        )
                                                                            .length ===
                                                                        1
                                                                            ? offering.capabilities.find(
                                                                                  (
                                                                                      capability,
                                                                                  ) =>
                                                                                      capability.capabilityCode ===
                                                                                      capabilityCodeForMode(
                                                                                          line.fulfillmentMode,
                                                                                      ),
                                                                              )
                                                                            : undefined
                                                                    updateLine(
                                                                        line.lineKey,
                                                                        {
                                                                            supplierId:
                                                                                offering?.supplierId ??
                                                                                "",
                                                                            supplierName:
                                                                                supplier?.supplierName ??
                                                                                "",
                                                                            offeringRevisionId:
                                                                                offering?.offeringRevisionId ??
                                                                                "",
                                                                            latestCostGross:
                                                                                offering
                                                                                    ? supplyCostForQuantity(
                                                                                          offering,
                                                                                          line.confirmedQuantity,
                                                                                      )
                                                                                    : "",
                                                                            inputTaxRate:
                                                                                offering?.inputTaxRate ??
                                                                                "",
                                                                            capabilityRevisionId:
                                                                                onlyCapability?.revisionId ??
                                                                                "",
                                                                            capabilitySummary:
                                                                                onlyCapability?.label ??
                                                                                "请选择有效供应资质",
                                                                            qualificationStatus:
                                                                                onlyCapability
                                                                                    ? "VALID"
                                                                                    : "INVALID",
                                                                        },
                                                                    )
                                                                }}
                                                                options={offeringOptionsForSku(
                                                                    subLine.itemSku,
                                                                )}
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                placeholder="选择供应商报价"
                                                                className="min-w-[12rem]"
                                                            />
                                                        </td>
                                                        <td className="px-3 py-2">
                                                            <Input
                                                                className="num w-20"
                                                                inputMode="decimal"
                                                                value={
                                                                    line.confirmedQuantity
                                                                }
                                                                onChange={(
                                                                    e,
                                                                ) => {
                                                                    const confirmedQuantity =
                                                                        e.target
                                                                            .value
                                                                    const offering =
                                                                        supplyOptions.find(
                                                                            (
                                                                                option,
                                                                            ) =>
                                                                                option.offeringRevisionId ===
                                                                                line.offeringRevisionId,
                                                                        )
                                                                    updateLine(
                                                                        line.lineKey,
                                                                        {
                                                                            confirmedQuantity,
                                                                            latestCostGross:
                                                                                offering
                                                                                    ? supplyCostForQuantity(
                                                                                          offering,
                                                                                          confirmedQuantity,
                                                                                      )
                                                                                    : line.latestCostGross,
                                                                        },
                                                                    )
                                                                }}
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                aria-label={`${line.supplierName} 确认数量`}
                                                            />
                                                        </td>
                                                        <td className="px-3 py-2">
                                                            <Input
                                                                className="num w-24"
                                                                inputMode="decimal"
                                                                value={
                                                                    line.latestCostGross
                                                                }
                                                                onChange={(e) =>
                                                                    updateLine(
                                                                        line.lineKey,
                                                                        {
                                                                            latestCostGross:
                                                                                e
                                                                                    .target
                                                                                    .value,
                                                                        },
                                                                    )
                                                                }
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                aria-label="最新含税成本"
                                                            />
                                                        </td>
                                                        <td className="hidden px-3 py-2 md:table-cell">
                                                            <Input
                                                                className="num w-16"
                                                                inputMode="decimal"
                                                                value={
                                                                    line.inputTaxRate
                                                                }
                                                                onChange={(e) =>
                                                                    updateLine(
                                                                        line.lineKey,
                                                                        {
                                                                            inputTaxRate:
                                                                                e
                                                                                    .target
                                                                                    .value,
                                                                        },
                                                                    )
                                                                }
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                aria-label="进项税率"
                                                            />
                                                        </td>
                                                        <td className="hidden px-3 py-2 sm:table-cell">
                                                            <DatePicker
                                                                className="w-[9.5rem]"
                                                                value={
                                                                    line.expectedDeliveryDate ||
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    next,
                                                                ) =>
                                                                    updateLine(
                                                                        line.lineKey,
                                                                        {
                                                                            expectedDeliveryDate:
                                                                                next ??
                                                                                "",
                                                                        },
                                                                    )
                                                                }
                                                                disabled={
                                                                    formalPending
                                                                }
                                                            />
                                                        </td>
                                                        <td className="px-3 py-2">
                                                            <OptionCombobox
                                                                value={
                                                                    line.fulfillmentMode
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) => {
                                                                    if (!value)
                                                                        return
                                                                    const fulfillmentMode =
                                                                        value as FulfillmentMode
                                                                    const offering =
                                                                        supplyOptions.find(
                                                                            (
                                                                                option,
                                                                            ) =>
                                                                                option.offeringRevisionId ===
                                                                                line.offeringRevisionId,
                                                                        )
                                                                    const capabilities =
                                                                        offering?.capabilities.filter(
                                                                            (
                                                                                capability,
                                                                            ) =>
                                                                                capability.capabilityCode ===
                                                                                capabilityCodeForMode(
                                                                                    fulfillmentMode,
                                                                                ),
                                                                        ) ?? []
                                                                    const onlyCapability =
                                                                        capabilities.length ===
                                                                        1
                                                                            ? capabilities[0]
                                                                            : undefined
                                                                    updateLine(
                                                                        line.lineKey,
                                                                        {
                                                                            fulfillmentMode,
                                                                            capabilityRevisionId:
                                                                                onlyCapability?.revisionId ??
                                                                                "",
                                                                            capabilitySummary:
                                                                                onlyCapability?.label ??
                                                                                "请选择有效供应资质",
                                                                            qualificationStatus:
                                                                                onlyCapability
                                                                                    ? "VALID"
                                                                                    : "INVALID",
                                                                        },
                                                                    )
                                                                }}
                                                                options={fulfillmentOptionsForOffering(
                                                                    line.offeringRevisionId,
                                                                )}
                                                                size="sm"
                                                                allowClear={
                                                                    false
                                                                }
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                aria-label="交付方式"
                                                                placeholder="交付方式"
                                                                className="min-w-[8rem]"
                                                            />
                                                        </td>
                                                        <td className="hidden px-3 py-2 lg:table-cell">
                                                            <OptionCombobox
                                                                value={
                                                                    line.capabilityRevisionId ||
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    revisionId,
                                                                ) => {
                                                                    const capability =
                                                                        capabilityOptionsForOffering(
                                                                            line.offeringRevisionId,
                                                                            line.fulfillmentMode,
                                                                        ).find(
                                                                            (
                                                                                option,
                                                                            ) =>
                                                                                option.value ===
                                                                                revisionId,
                                                                        )
                                                                    updateLine(
                                                                        line.lineKey,
                                                                        {
                                                                            capabilityRevisionId:
                                                                                revisionId ??
                                                                                "",
                                                                            capabilitySummary:
                                                                                capability?.label ??
                                                                                "",
                                                                            qualificationStatus:
                                                                                revisionId
                                                                                    ? "VALID"
                                                                                    : "INVALID",
                                                                        },
                                                                    )
                                                                }}
                                                                options={capabilityOptionsForOffering(
                                                                    line.offeringRevisionId,
                                                                    line.fulfillmentMode,
                                                                )}
                                                                size="sm"
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                placeholder="选择供应资质"
                                                                className="min-w-[8rem]"
                                                            />
                                                        </td>
                                                        <td className="px-3 py-2 text-right">
                                                            <Button
                                                                type="button"
                                                                size="sm"
                                                                variant="ghost"
                                                                disabled={
                                                                    formalPending ||
                                                                    lines.length <=
                                                                        1
                                                                }
                                                                onClick={() =>
                                                                    removeLine(
                                                                        line.lineKey,
                                                                    )
                                                                }
                                                            >
                                                                删除
                                                            </Button>
                                                        </td>
                                                    </tr>
                                                ))}
                                            </tbody>
                                        </table>
                                    </div>

                                    <div className="border-t border-border px-3 py-2">
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={formalPending}
                                            onClick={() =>
                                                addSplitLine(
                                                    subLine.submissionLineId,
                                                )
                                            }
                                        >
                                            <PlusIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            拆分供应商
                                        </Button>
                                    </div>
                                </div>
                            )
                        })}
                    </div>
                </section>

                {saveMessage ? (
                    <p className="text-xs text-muted-foreground" role="status">
                        {saveMessage}
                        {dirty ? " · 之后有未保存修改" : null}
                    </p>
                ) : dirty ? (
                    <p
                        className="text-xs text-warning-soft-foreground"
                        role="status"
                    >
                        有未保存的确认分行修改（⌘S 保存）
                    </p>
                ) : null}
            </CardContent>
        </Card>
    )
}
