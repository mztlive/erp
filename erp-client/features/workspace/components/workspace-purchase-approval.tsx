"use client"

import {
    CheckCircle2Icon,
    ChevronRightIcon,
    FileTextIcon,
    TriangleAlertIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { purchaseComparison } from "./workspace-purchase-comparison-model"
import { WorkspaceSupplierRating } from "./workspace-supplier-rating"
import { WorkspacePurchaseComparisonTable } from "./workspace-purchase-comparison-table"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

function isLineNegativeMargin(line: ApprovalPurchaseLine): boolean {
    const cost = line.unit_cost_gross
    const salesPrice = line.source?.unit_price_gross
    if (cost == null || cost === "" || salesPrice == null || salesPrice === "")
        return false
    const unitMismatch = Boolean(
        line.unit?.trim() &&
        line.source?.unit?.trim() &&
        line.unit.trim() !== line.source.unit.trim(),
    )
    if (unitMismatch) return false
    try {
        return compareDecimal(cost, salesPrice, 6) > 0
    } catch {
        return false
    }
}

function isLineOverSupplyPrice(line: ApprovalPurchaseLine): boolean {
    const cost = line.unit_cost_gross
    const supplyPrice = line.source?.procurement_supply_price_gross
    if (
        cost == null ||
        cost === "" ||
        supplyPrice == null ||
        supplyPrice === ""
    )
        return false
    const unitMismatch = Boolean(
        line.unit?.trim() &&
        line.source?.unit?.trim() &&
        line.unit.trim() !== line.source.unit.trim(),
    )
    if (unitMismatch) return false
    try {
        return compareDecimal(cost, supplyPrice, 6) > 0
    } catch {
        return false
    }
}

/** 判定采购审批明细是否存在负毛利或采购价超预估供给价等高危复核信号（严格定点十进制）。 */
export function assessPurchaseApprovalRisk(
    lines?: ApprovalPurchaseLine[] | null,
): { highRisk: boolean; reasons: string[] } {
    if (!lines?.length) return { highRisk: false, reasons: [] }
    let negativeMarginCount = 0
    let overSupplyCount = 0
    for (const line of lines) {
        if (isLineNegativeMargin(line)) negativeMarginCount += 1
        if (isLineOverSupplyPrice(line)) overSupplyCount += 1
    }
    const reasons: string[] = []
    if (negativeMarginCount > 0) {
        reasons.push(
            `存在 ${negativeMarginCount} 项采购单价高于销售含税单价（负毛利风险）`,
        )
    }
    if (overSupplyCount > 0) {
        reasons.push(`存在 ${overSupplyCount} 项采购单价高于销售预估采购供给价`)
    }
    return { highRisk: reasons.length > 0, reasons }
}

/** 采购审批只消费实例授权的提交资料；来源入口保持在同一审批上下文。 */
export function WorkspacePurchaseApproval({
    instanceId,
    item,
    onPreview,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onPreview: (target: WorkspacePaperTarget) => void
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
    const query = useApprovalMaterials(instanceId, true, item)
    const prefix = `workspace-purchase-approval-${toAutomationIdSegment(instanceId)}`
    const preview = (view: "all" | "source-sales", revisionId?: string) =>
        onPreview({
            kind: "approval_snapshot",
            objectId: instanceId,
            materialsView: view,
            sourceRevisionId: revisionId,
        })

    if (query.isPending)
        return (
            <DetailWorkspaceSection ariaLabel="供应商与客户">
                <p
                    role="status"
                    className="flex items-center gap-2 text-xs text-muted-foreground"
                >
                    <Spinner />
                    正在读取供应商与客户…
                </p>
            </DetailWorkspaceSection>
        )
    if (query.isError)
        return (
            <DetailWorkspaceSection>
                <BusinessFailureState
                    id={`${prefix}-retry`}
                    title="供应商与客户读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </DetailWorkspaceSection>
        )

    const {
        display,
        attachments,
        purchase_lines: lines,
        extra_cost_summary: extraCost,
    } = query.data
    const sources = display.source_sales ?? []

    const exceptionStats = (lines ?? []).reduce(
        (acc, line) => {
            const comparison = purchaseComparison(line)
            const negativeMargin = isLineNegativeMargin(line)
            const overSupply = isLineOverSupplyPrice(line)
            if (comparison.warnings.length > 0 || negativeMargin) {
                acc.abnormalLines += 1
            }
            if (negativeMargin) acc.negativeMarginCount += 1
            if (overSupply) acc.overSupplyCount += 1
            if (comparison.delayDays > 0) acc.delayedCount += 1
            return acc
        },
        {
            abnormalLines: 0,
            negativeMarginCount: 0,
            overSupplyCount: 0,
            delayedCount: 0,
        },
    )

    return (
        <>
            <DetailWorkspaceSection
                contentClassName="grid gap-4 space-y-0 @min-[720px]/document:grid-cols-2 @min-[720px]/document:gap-6"
                ariaLabel="供应商与客户"
            >
                <div className="min-w-0 space-y-2">
                    <h2 className={detailWorkspaceStyles.sectionTitle}>
                        供应商与评估
                    </h2>
                    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5">
                        <p className="text-body-compact font-semibold wrap-anywhere">
                            {displayName(
                                display.source.extra_sections.find(
                                    (field) => field.label === "供应商",
                                )?.value,
                            ) ||
                                displayName(display.counterparty_label) ||
                                "未记录供应商名称，请联系申请人核对"}
                        </p>
                        <WorkspaceSupplierRating
                            rating={query.data.latest_supplier_rating}
                            compact
                        />
                    </div>
                </div>
                <div className="min-w-0 space-y-2 border-t border-border/60 pt-4 @min-[720px]/document:border-t-0 @min-[720px]/document:border-l @min-[720px]/document:pt-0 @min-[720px]/document:pl-6">
                    <h2 className={detailWorkspaceStyles.sectionTitle}>
                        客户与销售单
                    </h2>
                    {sources.length ? (
                        sources.map((sales) => (
                            <div
                                key={sales.revision_id}
                                className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5"
                            >
                                <p className="min-w-0 basis-full text-body-compact font-semibold wrap-anywhere">
                                    {displayName(sales.source.customer) ||
                                        "未记录客户名称"}
                                </p>
                                <p className="min-w-0 text-body-compact wrap-anywhere">
                                    <span className="num">
                                        {displayBusinessText(
                                            sales.document_no,
                                            sales.document_id,
                                        ) || "未记录销售单号"}
                                    </span>
                                    <span className="ml-2 text-muted-foreground">
                                        · 销售版本 V{sales.revision_no}
                                    </span>
                                </p>
                                <Button
                                    id={`${prefix}-source-${toAutomationIdSegment(sales.revision_id)}`}
                                    variant="ghost"
                                    size="sm"
                                    className={`${detailWorkspaceStyles.subtleButton} ml-auto`}
                                    onClick={() =>
                                        preview(
                                            "source-sales",
                                            sales.revision_id,
                                        )
                                    }
                                >
                                    <FileTextIcon aria-hidden="true" />
                                    预览销售单
                                    <ChevronRightIcon aria-hidden="true" />
                                </Button>
                                {sales.source.extra_sections
                                    .filter(
                                        (field) => field.label === "合同或凭证",
                                    )
                                    .map((field) => (
                                        <p
                                            key={field.label}
                                            className="basis-full text-body-compact text-warning-soft-foreground"
                                        >
                                            {field.value}
                                        </p>
                                    ))}
                            </div>
                        ))
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            此次提交未保留来源销售资料，请联系申请人核对后再审批。
                        </p>
                    )}
                </div>
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title="采购与销售对照"
                subtitle={lines ? `共 ${lines.length} 项` : undefined}
                ariaLabel="采购与销售对照"
                actions={
                    <Button
                        id={`${prefix}-submission`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() => preview("all")}
                    >
                        <FileTextIcon aria-hidden="true" />
                        查看提交资料
                    </Button>
                }
            >
                {lines?.length || extraCost ? (
                    <div
                        data-slot="purchase-exception-strip"
                        role="status"
                        className={cn(
                            "flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border px-3.5 py-2.5 text-xs",
                            exceptionStats.negativeMarginCount > 0
                                ? "border-destructive-border bg-destructive-soft/40 text-destructive-soft-foreground"
                                : exceptionStats.abnormalLines > 0
                                  ? "border-warning-border bg-warning-soft/45 text-warning-soft-foreground"
                                  : "border-success-border bg-success-soft/35 text-success-soft-foreground",
                        )}
                    >
                        {exceptionStats.abnormalLines > 0 ? (
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-4 shrink-0"
                            />
                        ) : (
                            <CheckCircle2Icon
                                aria-hidden="true"
                                className="size-4 shrink-0"
                            />
                        )}
                        <span className="font-medium">
                            {exceptionStats.abnormalLines > 0
                                ? `成本与交期异常摘要：${exceptionStats.abnormalLines} 项需核对`
                                : "成本与交期核对正常，无价格或交期异常项"}
                        </span>
                        <div className="flex flex-wrap items-center gap-1.5">
                            {exceptionStats.negativeMarginCount > 0 ? (
                                <Badge variant="destructive">
                                    高于售价{" "}
                                    <span className="num">
                                        {exceptionStats.negativeMarginCount}
                                    </span>{" "}
                                    项
                                </Badge>
                            ) : null}
                            {exceptionStats.overSupplyCount > 0 ? (
                                <Badge variant="warning">
                                    超预估供给价{" "}
                                    <span className="num">
                                        {exceptionStats.overSupplyCount}
                                    </span>{" "}
                                    项
                                </Badge>
                            ) : null}
                            {exceptionStats.delayedCount > 0 ? (
                                <Badge variant="warning">
                                    交期晚于承诺{" "}
                                    <span className="num">
                                        {exceptionStats.delayedCount}
                                    </span>{" "}
                                    项
                                </Badge>
                            ) : null}
                        </div>
                        {extraCost ? (
                            <p className="basis-full border-t border-border/50 pt-2 text-xs text-muted-foreground">
                                来源销售单已有附加费用（不含本次提交）：已发生{" "}
                                <MoneyValue
                                    value={extraCost.occurred_gross_total}
                                />
                                ，待发生{" "}
                                <MoneyValue
                                    value={extraCost.pending_gross_total}
                                />
                                。附加费用不向客户收费，会直接减少本单利润。
                            </p>
                        ) : null}
                    </div>
                ) : null}
                {lines?.length ? (
                    <WorkspacePurchaseComparisonTable
                        lines={lines}
                        sources={sources}
                        prefix={prefix}
                        onPreview={(revisionId) =>
                            preview("source-sales", revisionId)
                        }
                    />
                ) : (
                    <>
                        <p className="text-xs text-muted-foreground">
                            此次提交暂无可用的逐行对照，请核对提交资料及附件。
                        </p>
                        {display.source.lines.length > 0 && (
                            <ul className="divide-y text-body-compact">
                                {display.source.lines.map((line, index) => (
                                    <li
                                        key={`${line.title}:${index}`}
                                        className="flex flex-wrap justify-between gap-2 py-2"
                                    >
                                        <span>
                                            {displayName(line.title) ||
                                                "未记录商品名称"}
                                        </span>
                                        <span className="text-muted-foreground">
                                            {[line.quantity, line.due_label]
                                                .filter(Boolean)
                                                .join(" · ")}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        )}
                        {display.source.more_count > 0 && (
                            <p className="text-xs text-muted-foreground">
                                另有 {display.source.more_count}{" "}
                                行未包含在摘要中，请核对提交资料及附件。
                            </p>
                        )}
                    </>
                )}
            </DetailWorkspaceSection>

            {!hideEmbeddedMaterials ? (
                <DetailWorkspaceSection
                    title="提交附件及关联销售合同、凭证"
                    subtitle={
                        attachments.length
                            ? `共 ${attachments.length} 个文件`
                            : undefined
                    }
                    ariaLabel="提交附件及关联销售合同、凭证"
                >
                    <ApprovalMaterialFiles
                        instanceId={instanceId}
                        attachments={attachments}
                        idPrefix={`${prefix}-file`}
                        emptyMessage="本次提交未附带合同、凭证或其他附件。"
                        onSelectFile={onSelectFile}
                        activeFileId={activeFileId}
                    />
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}
