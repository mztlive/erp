import { useState } from "react"
import { ArrowRightIcon, FileTextIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"

import { isPurchaseFeeLine } from "./workspace-purchase-comparison-model"
import {
    checkPurchaseRequirements,
    type PurchaseRequirementCheck,
} from "./workspace-purchase-requirement-diff"

export type PurchaseSourceComparisonTarget = Readonly<{
    revisionId?: string
    purchaseLineId?: string
}>

const ALL_SOURCES = "all"

export function WorkspacePurchaseSourceComparison({
    idPrefix,
    orders,
    snapshots,
    purchaseLines,
    target,
    onTargetChange,
    onPreview,
}: {
    idPrefix: string
    orders: NonNullable<ApprovalMaterials["source_sales_orders"]>
    snapshots?: ApprovalMaterials["display"]["source_sales"]
    purchaseLines?: ApprovalMaterials["purchase_lines"]
    target?: PurchaseSourceComparisonTarget
    onTargetChange: (target: PurchaseSourceComparisonTarget) => void
    onPreview: (revisionId: string) => void
}) {
    const [showAll, setShowAll] = useState(false)
    const sourceLabels = new Map<string, string>()
    for (const source of snapshots ?? []) {
        const number = displayBusinessText(
            source.document_no,
            source.document_id,
        )
        sourceLabels.set(
            source.revision_id,
            `${number || "未记录销售单号"} · V${source.revision_no}`,
        )
    }
    for (const order of orders) {
        if (sourceLabels.has(order.revision.id)) continue
        const number = displayBusinessText(order.document_no, order.revision.id)
        sourceLabels.set(
            order.revision.id,
            `${number || "未记录销售单号"} · V${order.revision.revision_no}`,
        )
    }

    const revisionId =
        sourceLabels.size > 1 &&
        target?.revisionId &&
        sourceLabels.has(target.revisionId)
            ? target.revisionId
            : undefined
    const preview = revisionId
        ? orders.find((order) => order.revision.id === revisionId)
        : sourceLabels.size === 1
          ? orders[0]
          : undefined
    const rows = (purchaseLines ?? [])
        .filter(
            (line) =>
                !isPurchaseFeeLine(line) &&
                (!revisionId || line.source?.revision_id === revisionId),
        )
        .map((line) => {
            const checks = checkPurchaseRequirements(line)
            return {
                line,
                checks,
                review: checks.some(
                    ({ status }) =>
                        status === "difference" || status === "unavailable",
                ),
                partial: checks.some(({ status }) => status === "partial"),
                selected: line.id === target?.purchaseLineId,
            }
        })
    const reviewCount = rows.filter((row) => row.review).length
    const partialCount = rows.filter((row) => row.partial).length
    const visibleRows = rows
        .filter((row) => showAll || row.review || row.partial || row.selected)
        .toSorted((left, right) => {
            if (left.selected !== right.selected) return left.selected ? -1 : 1
            if (left.review !== right.review) return left.review ? -1 : 1
            if (left.partial !== right.partial) return left.partial ? -1 : 1
            return left.line.line_no - right.line.line_no
        })

    return (
        <div className="space-y-3">
            {sourceLabels.size ? (
                <div className="flex flex-wrap items-center justify-between gap-2">
                    {sourceLabels.size > 1 ? (
                        <Select
                            value={revisionId ?? ALL_SOURCES}
                            onValueChange={(value) => {
                                if (value) {
                                    onTargetChange({
                                        revisionId:
                                            value === ALL_SOURCES
                                                ? undefined
                                                : value,
                                    })
                                }
                            }}
                        >
                            <SelectTrigger
                                id={`${idPrefix}-source-order`}
                                aria-label="筛选来源销售单"
                                size="sm"
                                className="min-w-0 flex-1"
                            >
                                <SelectValue className="min-w-0">
                                    <span className="num truncate">
                                        {revisionId
                                            ? sourceLabels.get(revisionId)
                                            : "全部来源销售单"}
                                    </span>
                                </SelectValue>
                            </SelectTrigger>
                            <SelectContent
                                align="start"
                                alignItemWithTrigger={false}
                            >
                                <SelectItem
                                    idPrefix={`${idPrefix}-source-order`}
                                    value={ALL_SOURCES}
                                >
                                    全部来源销售单
                                </SelectItem>
                                {Array.from(sourceLabels, ([id, label]) => (
                                    <SelectItem
                                        key={id}
                                        idPrefix={`${idPrefix}-source-order`}
                                        value={id}
                                        className="num whitespace-normal"
                                    >
                                        {label}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    ) : (
                        <p className="num text-body-compact font-medium wrap-anywhere">
                            {sourceLabels.values().next().value}
                        </p>
                    )}
                    {preview ? (
                        <Button
                            id={`${idPrefix}-preview-${toAutomationIdSegment(preview.revision.id)}`}
                            type="button"
                            variant="outline"
                            size="xs"
                            onClick={() => onPreview(preview.revision.id)}
                        >
                            <FileTextIcon aria-hidden="true" />
                            完整销售单
                        </Button>
                    ) : null}
                </div>
            ) : null}

            {!rows.length ? (
                <p className="text-body-compact text-muted-foreground">
                    {purchaseLines == null
                        ? "采购明细暂不可用，请核对审批资料。"
                        : revisionId
                          ? "本次采购没有关联该来源的商品明细。"
                          : "本单没有需要采销核对的商品明细。"}
                </p>
            ) : (
                <>
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <div
                            role="status"
                            className="flex flex-wrap items-center gap-1.5"
                        >
                            <Badge
                                variant={reviewCount ? "warning" : "success"}
                            >
                                {reviewCount
                                    ? `${reviewCount} 行待核对`
                                    : "未发现不符"}
                            </Badge>
                            {partialCount > 0 ? (
                                <Badge variant="neutral">
                                    {partialCount} 行部分采购
                                </Badge>
                            ) : null}
                        </div>
                        <div
                            role="group"
                            aria-label="明细显示范围"
                            className={detailWorkspaceStyles.subTabs}
                        >
                            <button
                                id={`${idPrefix}-differences`}
                                type="button"
                                aria-pressed={!showAll}
                                data-active={!showAll || undefined}
                                className={detailWorkspaceStyles.subTab}
                                onClick={() => setShowAll(false)}
                            >
                                差异
                            </button>
                            <button
                                id={`${idPrefix}-all-lines`}
                                type="button"
                                aria-pressed={showAll}
                                data-active={showAll || undefined}
                                className={detailWorkspaceStyles.subTab}
                                onClick={() => setShowAll(true)}
                            >
                                全部 <span className="num">{rows.length}</span>
                            </button>
                        </div>
                    </div>
                    {visibleRows.length ? (
                        <ol className="divide-y divide-border/70 border-y border-border/70">
                            {visibleRows.map(({ line, checks, selected }) => (
                                <li
                                    key={line.id}
                                    className={cn(
                                        "space-y-2 py-2.5",
                                        selected && "bg-info-soft/30",
                                    )}
                                >
                                    <div className="flex flex-wrap items-baseline justify-between gap-x-2 gap-y-0.5 wrap-anywhere">
                                        <p className="text-body-compact font-medium">
                                            <span className="num text-muted-foreground">
                                                第 {line.line_no} 行
                                            </span>
                                            {" · "}
                                            {displayName(line.title) ||
                                                "未记录商品名称"}
                                        </p>
                                        {sourceLabels.size > 1 &&
                                        line.source ? (
                                            <span className="num text-xs text-muted-foreground">
                                                {sourceLabels.get(
                                                    line.source.revision_id,
                                                )}
                                            </span>
                                        ) : null}
                                    </div>
                                    <ul className="space-y-1.5">
                                        {checks
                                            .filter(
                                                (check) =>
                                                    showAll ||
                                                    selected ||
                                                    check.status !== "match",
                                            )
                                            .map((check) => (
                                                <li key={check.field}>
                                                    <PurchaseRequirementEvidence
                                                        check={check}
                                                    />
                                                </li>
                                            ))}
                                    </ul>
                                </li>
                            ))}
                        </ol>
                    ) : (
                        <p className="border-t border-border/70 pt-3 text-body-compact text-muted-foreground">
                            当前范围的商品品规、数量与交期均符合销售要求。
                        </p>
                    )}
                </>
            )}
        </div>
    )
}

const resultTone = {
    difference: "destructive",
    unavailable: "warning",
    partial: "neutral",
    match: "success",
} as const

function PurchaseRequirementEvidence({
    check,
}: {
    check: PurchaseRequirementCheck
}) {
    const salesLabel =
        check.field === "数量"
            ? "销售总需求"
            : check.field === "交期"
              ? "销售承诺"
              : "销售要求"
    const purchaseLabel = check.field === "交期" ? "采购预计" : "本次采购"
    const numeric = check.field === "数量" || check.field === "交期"
    return (
        <div className="space-y-0.5 rounded-md bg-muted/40 px-2.5 py-1.5">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                <span className="font-medium text-foreground">
                    {check.field}
                </span>
                <Badge variant={resultTone[check.status]}>{check.result}</Badge>
            </div>
            {check.sales !== undefined && check.purchase !== undefined ? (
                <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-start gap-x-2 text-body-compact wrap-anywhere">
                    <div>
                        <span className="mr-1.5 text-xs text-muted-foreground">
                            {salesLabel}
                        </span>
                        <span className={numeric ? "num" : undefined}>
                            {check.sales}
                        </span>
                    </div>
                    <ArrowRightIcon
                        className="mt-1 size-3.5 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <div>
                        <span className="mr-1.5 text-xs text-muted-foreground">
                            {purchaseLabel}
                        </span>
                        <span className={cn("font-medium", numeric && "num")}>
                            {check.purchase}
                        </span>
                    </div>
                </div>
            ) : null}
        </div>
    )
}
