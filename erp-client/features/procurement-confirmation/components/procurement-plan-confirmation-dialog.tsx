"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    BusinessFailureState,
    OptionCombobox,
    ValidationSummary,
} from "@/components/business"
import type { ValidationIssue } from "@/components/business/feedback"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    CheckIcon,
    CircleCheckIcon,
    PlusIcon,
    TriangleAlertIcon,
} from "lucide-react"

import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import {
    type ConfirmationLineDraft,
    type FulfillmentMode,
    type ProcurementConfirmationTask,
    type ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"
import { money } from "@/features/procurement-confirmation/lib/format"
import { capabilityCodeForMode } from "@/features/procurement-confirmation/lib/supply-cost"

type SelectionOption = {
    value: string
    label: string
}

type PlanCoverage = {
    submissionLineId: string
    itemName: string
    confirmed: string
    required: string
    complete: boolean
    gap: string
}

type ProcurementPlanConfirmationDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    task: ProcurementConfirmationTask
    recommendation: ProcurementRecommendation | undefined
    recommendationPending: boolean
    recommendationFailed: boolean
    recommendationError: unknown
    onRetryRecommendation: () => void
    currentPlanSummary: {
        purchaseGross: number
        grossMargin: number
        orderCount: number
    }
    dirty: boolean
    lineDrafts: readonly ConfirmationLineDraft[]
    coverage: readonly PlanCoverage[]
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
    updatePlanLine: (
        lineKey: string,
        patch: Partial<ConfirmationLineDraft>,
    ) => void
    addSplitLine: (submissionLineId: string) => void
    removeLine: (lineKey: string) => void
    clientBlocking: readonly ValidationIssue[]
    allCovered: boolean
    formalPending: boolean
    isSubmitting: boolean
    advanceAfterConfirm: boolean
    onApprove: () => Promise<void>
}

export function ProcurementPlanConfirmationDialog({
    open,
    onOpenChange,
    task,
    recommendation,
    recommendationPending,
    recommendationFailed,
    recommendationError,
    onRetryRecommendation,
    currentPlanSummary,
    dirty,
    lineDrafts,
    coverage,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    updatePlanLine,
    addSplitLine,
    removeLine,
    clientBlocking,
    allCovered,
    formalPending,
    isSubmitting,
    advanceAfterConfirm,
    onApprove,
}: ProcurementPlanConfirmationDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-h-[88vh] w-[calc(100vw-2rem)] overflow-x-hidden overflow-y-auto sm:max-w-6xl">
                <DialogHeader>
                    <DialogTitle>确认采购方案</DialogTitle>
                    <DialogDescription>
                        销售单 {task.salesSubmission.salesOrderNo}
                        。系统按采购数量匹配报价档位，并在可供范围内组合预计成本最低的采购方案。
                    </DialogDescription>
                </DialogHeader>

                {recommendationPending ? (
                    <div className="rounded-lg border border-border bg-muted/30 p-6 text-center text-sm text-muted-foreground">
                        正在核对供应商报价、起订量和可供数量…
                    </div>
                ) : recommendationFailed ? (
                    <BusinessFailureState
                        title="采购方案计算失败"
                        error={recommendationError}
                        onRetry={() => onRetryRecommendation()}
                    />
                ) : recommendation?.ready ? (
                    <div className="space-y-4">
                        <Alert variant="success">
                            <CircleCheckIcon aria-hidden="true" />
                            <AlertTitle>
                                当前方案预计形成 {currentPlanSummary.orderCount}{" "}
                                组采购创建依据
                            </AlertTitle>
                            <AlertDescription>
                                当前采购含税{" "}
                                {money.format(currentPlanSummary.purchaseGross)}
                                ，预计毛利{" "}
                                {money.format(currentPlanSummary.grossMargin)}。
                            </AlertDescription>
                        </Alert>

                        <div className="grid gap-3 sm:grid-cols-3">
                            <div className="rounded-lg border border-border p-3">
                                <p className="text-xs text-muted-foreground">
                                    销售含税金额
                                </p>
                                <p className="num mt-1 font-semibold">
                                    {money.format(
                                        Number(recommendation.salesGross),
                                    )}
                                </p>
                            </div>
                            <div className="rounded-lg border border-border p-3">
                                <p className="text-xs text-muted-foreground">
                                    预计采购金额
                                </p>
                                <p className="num mt-1 font-semibold">
                                    {money.format(
                                        currentPlanSummary.purchaseGross,
                                    )}
                                </p>
                            </div>
                            <div className="rounded-lg border border-border p-3">
                                <p className="text-xs text-muted-foreground">
                                    预计毛利
                                </p>
                                <p className="num mt-1 font-semibold">
                                    {money.format(
                                        currentPlanSummary.grossMargin,
                                    )}
                                </p>
                            </div>
                        </div>

                        <section
                            className="space-y-3"
                            aria-labelledby="editable-plan-title"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div>
                                    <h3
                                        id="editable-plan-title"
                                        className="font-semibold"
                                    >
                                        当前采购安排
                                    </h3>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        可以调整供应商、数量、交付方式和交期；采购价会按该供应商承接的合计数量自动重算。
                                    </p>
                                </div>
                                <Badge
                                    variant={dirty ? "secondary" : "outline"}
                                >
                                    {dirty ? "已人工调整" : "系统初始方案"}
                                </Badge>
                            </div>

                            {task.salesSubmission.lines.map((subLine) => {
                                const planLines = lineDrafts.filter(
                                    (line) =>
                                        line.submissionLineId ===
                                        subLine.submissionLineId,
                                )
                                const lineCoverage = coverage.find(
                                    (item) =>
                                        item.submissionLineId ===
                                        subLine.submissionLineId,
                                )
                                return (
                                    <div
                                        key={subLine.submissionLineId}
                                        className="overflow-hidden rounded-lg border border-border"
                                    >
                                        <div className="flex flex-wrap items-start justify-between gap-2 bg-muted/40 px-4 py-3">
                                            <div>
                                                <p className="font-medium">
                                                    {subLine.itemName}
                                                </p>
                                                <p className="mt-1 text-xs text-muted-foreground">
                                                    需要采购{" "}
                                                    {subLine.committedQuantity}{" "}
                                                    {subLine.unit} · 客户交期{" "}
                                                    {
                                                        subLine.requestedDeliveryDate
                                                    }
                                                </p>
                                            </div>
                                            <Badge
                                                variant={
                                                    lineCoverage?.complete
                                                        ? "secondary"
                                                        : "destructive"
                                                }
                                            >
                                                已安排{" "}
                                                {lineCoverage?.confirmed ?? "0"}
                                                /
                                                {lineCoverage?.required ??
                                                    subLine.committedQuantity}{" "}
                                                {subLine.unit}
                                            </Badge>
                                        </div>

                                        <div className="space-y-3 p-3">
                                            {planLines.map((line) => {
                                                const offering =
                                                    supplyOptions.find(
                                                        (option) =>
                                                            option.offeringRevisionId ===
                                                            line.offeringRevisionId,
                                                    )
                                                const allocatedQuantity =
                                                    lineDrafts
                                                        .filter(
                                                            (item) =>
                                                                item.offeringRevisionId ===
                                                                line.offeringRevisionId,
                                                        )
                                                        .reduce(
                                                            (total, item) =>
                                                                total +
                                                                Number(
                                                                    item.confirmedQuantity ||
                                                                        0,
                                                                ),
                                                            0,
                                                        )
                                                const usesBulkPrice = offering
                                                    ? allocatedQuantity >=
                                                      Number(
                                                          offering.bulkMinimumOrderQuantity,
                                                      )
                                                    : false
                                                return (
                                                    <div
                                                        key={line.lineKey}
                                                        className="grid min-w-0 gap-3 rounded-md border border-border p-3 md:grid-cols-2 lg:grid-cols-4"
                                                    >
                                                        <div className="min-w-0 space-y-1.5 md:col-span-2 lg:col-span-2">
                                                            <Label>
                                                                供应商报价
                                                            </Label>
                                                            <OptionCombobox
                                                                value={
                                                                    line.offeringRevisionId ||
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    revisionId,
                                                                ) => {
                                                                    const nextOffering =
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
                                                                                nextOffering?.supplierId,
                                                                        )
                                                                    const matchingCapabilities =
                                                                        nextOffering?.capabilities.filter(
                                                                            (
                                                                                capability,
                                                                            ) =>
                                                                                capability.capabilityCode ===
                                                                                capabilityCodeForMode(
                                                                                    line.fulfillmentMode,
                                                                                ),
                                                                        ) ?? []
                                                                    const capability =
                                                                        matchingCapabilities.length ===
                                                                        1
                                                                            ? matchingCapabilities[0]
                                                                            : undefined
                                                                    updatePlanLine(
                                                                        line.lineKey,
                                                                        {
                                                                            supplierId:
                                                                                nextOffering?.supplierId ??
                                                                                "",
                                                                            supplierName:
                                                                                supplier?.supplierName ??
                                                                                "",
                                                                            offeringRevisionId:
                                                                                nextOffering?.offeringRevisionId ??
                                                                                "",
                                                                            inputTaxRate:
                                                                                nextOffering?.inputTaxRate ??
                                                                                "",
                                                                            capabilityRevisionId:
                                                                                capability?.revisionId ??
                                                                                "",
                                                                            capabilitySummary:
                                                                                capability?.label ??
                                                                                "请选择供应资质",
                                                                            qualificationStatus:
                                                                                capability
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
                                                                className="w-full min-w-0"
                                                                inputClassName="h-9 min-h-9"
                                                            />
                                                        </div>

                                                        <div className="min-w-0 space-y-1.5">
                                                            <Label>
                                                                采购数量
                                                            </Label>
                                                            <Input
                                                                className="num h-9"
                                                                inputMode="decimal"
                                                                value={
                                                                    line.confirmedQuantity
                                                                }
                                                                onChange={(
                                                                    event,
                                                                ) =>
                                                                    updatePlanLine(
                                                                        line.lineKey,
                                                                        {
                                                                            confirmedQuantity:
                                                                                event
                                                                                    .target
                                                                                    .value,
                                                                        },
                                                                    )
                                                                }
                                                                disabled={
                                                                    formalPending
                                                                }
                                                            />
                                                        </div>

                                                        <div className="min-w-0 space-y-1.5">
                                                            <Label>
                                                                含税单价
                                                            </Label>
                                                            <div className="flex h-9 min-w-0 items-center justify-between gap-2 rounded-md border border-border bg-muted/30 px-3 text-sm">
                                                                <span className="num shrink-0 font-medium">
                                                                    {line.latestCostGross
                                                                        ? money.format(
                                                                              Number(
                                                                                  line.latestCostGross,
                                                                              ),
                                                                          )
                                                                        : "—"}
                                                                </span>
                                                                <span className="truncate text-xs text-muted-foreground">
                                                                    {offering
                                                                        ? usesBulkPrice
                                                                            ? "集采价"
                                                                            : "一件代发价"
                                                                        : "选择供应商后计算"}
                                                                </span>
                                                            </div>
                                                        </div>

                                                        <div className="min-w-0 space-y-1.5">
                                                            <Label>
                                                                交付方式
                                                            </Label>
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
                                                                    const capability =
                                                                        capabilities.length ===
                                                                        1
                                                                            ? capabilities[0]
                                                                            : undefined
                                                                    updatePlanLine(
                                                                        line.lineKey,
                                                                        {
                                                                            fulfillmentMode,
                                                                            capabilityRevisionId:
                                                                                capability?.revisionId ??
                                                                                "",
                                                                            capabilitySummary:
                                                                                capability?.label ??
                                                                                "请选择供应资质",
                                                                            qualificationStatus:
                                                                                capability
                                                                                    ? "VALID"
                                                                                    : "INVALID",
                                                                        },
                                                                    )
                                                                }}
                                                                options={fulfillmentOptionsForOffering(
                                                                    line.offeringRevisionId,
                                                                )}
                                                                allowClear={
                                                                    false
                                                                }
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                placeholder="选择交付方式"
                                                                className="w-full min-w-0"
                                                                inputClassName="h-9 min-h-9"
                                                            />
                                                        </div>

                                                        <div className="min-w-0 space-y-1.5">
                                                            <Label>
                                                                供应商交期
                                                            </Label>
                                                            <DatePicker
                                                                value={
                                                                    line.expectedDeliveryDate ||
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    updatePlanLine(
                                                                        line.lineKey,
                                                                        {
                                                                            expectedDeliveryDate:
                                                                                value ??
                                                                                "",
                                                                        },
                                                                    )
                                                                }
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                className="h-9 w-full min-w-0"
                                                            />
                                                        </div>

                                                        <div className="min-w-0 space-y-1.5">
                                                            <Label>
                                                                供应资质
                                                            </Label>
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
                                                                    updatePlanLine(
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
                                                                disabled={
                                                                    formalPending
                                                                }
                                                                placeholder="选择供应资质"
                                                                className="w-full min-w-0"
                                                                inputClassName="h-9 min-h-9"
                                                            />
                                                        </div>

                                                        <div className="flex min-w-0 items-end justify-end">
                                                            <Button
                                                                type="button"
                                                                size="sm"
                                                                variant="ghost"
                                                                disabled={
                                                                    formalPending ||
                                                                    planLines.length <=
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
                                                        </div>
                                                    </div>
                                                )
                                            })}

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
                                                增加供应商
                                            </Button>
                                        </div>
                                    </div>
                                )
                            })}

                            {clientBlocking.length > 0 ? (
                                <ValidationSummary
                                    title="确认采购创建依据前需要补齐"
                                    issues={clientBlocking}
                                />
                            ) : null}
                        </section>

                        {recommendation.warnings.length > 0 ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>确认前请核对</AlertTitle>
                                <AlertDescription>
                                    {recommendation.warnings
                                        .map((item) => item.message)
                                        .join("；")}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </div>
                ) : (
                    <Alert variant="destructive">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertTitle>暂时无法形成完整采购方案</AlertTitle>
                        <AlertDescription>
                            {recommendation?.blockingIssues
                                .map((item) => item.message)
                                .join("；") || "当前供应商报价或可供数量不足。"}
                        </AlertDescription>
                    </Alert>
                )}

                <DialogFooter>
                    <DialogClose
                        render={<Button type="button" variant="outline" />}
                    >
                        返回销售单
                    </DialogClose>
                    <Button
                        type="button"
                        disabled={
                            !recommendation?.ready ||
                            !allCovered ||
                            recommendationPending ||
                            isSubmitting
                        }
                        onClick={() => void onApprove()}
                    >
                        <CheckIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {isSubmitting
                            ? "正在确认采购创建依据…"
                            : advanceAfterConfirm
                              ? "保存调整、确认并打开下一条"
                              : "保存调整并确认"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
