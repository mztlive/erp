"use client"

import { FileTextIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import type { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText, displayName } from "@/lib/display-name"
import {
    isPurchaseCostAbove,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import { WorkspacePurchaseComparisonTable } from "./workspace-purchase-comparison-table"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

/** 决策提示和行标记使用同一采购成本比较规则。 */
export function purchaseApprovalRiskReasons(
    lines?: ApprovalPurchaseLine[] | null,
): string[] {
    if (!lines?.length) return []
    let negativeMarginCount = 0
    let overSupplyCount = 0
    for (const line of lines) {
        if (isPurchaseCostAbove(line, line.source?.unit_price_gross)) {
            negativeMarginCount += 1
        }
        if (
            isPurchaseCostAbove(
                line,
                line.source?.procurement_supply_price_gross,
            )
        ) {
            overSupplyCount += 1
        }
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
    return reasons
}

function purchaseApprovalPrefix(instanceId: string) {
    return `workspace-purchase-approval-${toAutomationIdSegment(instanceId)}`
}

export function WorkspacePurchaseApprovalSources({
    instanceId,
    sources,
    onPreview,
}: {
    instanceId: string
    sources: PurchaseSource[]
    onPreview: (target: WorkspacePaperTarget) => void
}) {
    if (!sources.length) {
        return (
            <p className="text-muted-foreground">
                本次提交未保留来源销售资料，请联系申请人核对。
            </p>
        )
    }
    return (
        <ul className="space-y-2">
            {sources.map((sales) => {
                const documentNo =
                    displayBusinessText(sales.document_no, sales.document_id) ||
                    "未记录销售单号"
                return (
                    <li key={sales.revision_id} className="space-y-1">
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                            <span className="num font-medium">
                                {documentNo}
                            </span>
                            <span className="text-muted-foreground">
                                {displayName(sales.source.customer) ||
                                    "未记录客户名称"}{" "}
                                · V{sales.revision_no}
                            </span>
                            <Button
                                id={`${purchaseApprovalPrefix(instanceId)}-source-${toAutomationIdSegment(sales.revision_id)}`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.subtleButton}
                                onClick={() =>
                                    onPreview({
                                        kind: "approval_snapshot",
                                        objectId: instanceId,
                                        materialsView: "source-sales",
                                        sourceRevisionId: sales.revision_id,
                                    })
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                                预览销售单
                            </Button>
                        </div>
                        {sales.source.extra_sections
                            .filter((field) => field.label === "合同或凭证")
                            .map((field) => (
                                <p
                                    key={field.label}
                                    className="text-xs text-warning-soft-foreground"
                                >
                                    {field.value}
                                </p>
                            ))}
                    </li>
                )
            })}
        </ul>
    )
}

/** 明细与预览均读取审批绑定的采购提交；来源链接固定到冻结销售版本。 */
export function WorkspacePurchaseApproval({
    instanceId,
    materials,
    riskReasons,
    onPreview,
    hideEmbeddedMaterials,
}: {
    instanceId: string
    materials: ReturnType<typeof useApprovalMaterials>
    riskReasons: readonly string[]
    onPreview: (target: WorkspacePaperTarget) => void
    hideEmbeddedMaterials: boolean
}) {
    const submission = materials.data
    const lines = submission?.purchase_lines
    const extraCost = submission?.extra_cost_summary
    const prefix = purchaseApprovalPrefix(instanceId)
    const attachments = submission?.attachments ?? []

    return (
        <>
            <DetailWorkspaceSection
                title="采购明细"
                ariaLabel="采购明细"
                subtitle={lines ? `共 ${lines.length} 行` : undefined}
                actions={
                    <Button
                        id={`${prefix}-submission`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() =>
                            onPreview({
                                kind: "approval_snapshot",
                                objectId: instanceId,
                                title: submission?.document_no,
                            })
                        }
                    >
                        <FileTextIcon aria-hidden="true" />
                        预览采购单
                    </Button>
                }
            >
                {riskReasons.length > 0 ? (
                    <Alert variant="destructive" role="alert">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertTitle>采购价格需复核</AlertTitle>
                        <AlertDescription>
                            {riskReasons.join("。")}。
                        </AlertDescription>
                    </Alert>
                ) : null}
                {materials.isPending ? (
                    <p
                        role="status"
                        className="flex items-center gap-2 text-xs text-muted-foreground"
                    >
                        <Spinner />
                        正在读取采购明细…
                    </p>
                ) : materials.isError ? (
                    <BusinessFailureState
                        id={`${prefix}-retry`}
                        title="采购明细读取失败"
                        error={materials.error}
                        onRetry={() => void materials.refetch()}
                    />
                ) : lines?.length ? (
                    <WorkspacePurchaseComparisonTable
                        lines={lines}
                        sources={submission?.display.source_sales ?? []}
                        prefix={prefix}
                        onPreview={(revisionId) =>
                            onPreview({
                                kind: "approval_snapshot",
                                objectId: instanceId,
                                materialsView: "source-sales",
                                sourceRevisionId: revisionId,
                            })
                        }
                    />
                ) : lines ? (
                    <p className="text-xs text-muted-foreground">
                        本次提交没有采购明细。
                    </p>
                ) : (
                    <Alert variant="warning">
                        <AlertDescription>
                            完整采购明细暂不可用，请通过采购单预览核对提交摘要及附件。
                        </AlertDescription>
                    </Alert>
                )}
                {extraCost ? (
                    <p className="text-xs text-muted-foreground">
                        来源销售单已有附加费用（本次提交之外） · 已发生{" "}
                        <MoneyValue value={extraCost.occurred_gross_total} />
                        {" · 待发生 "}
                        <MoneyValue value={extraCost.pending_gross_total} />
                    </p>
                ) : null}
            </DetailWorkspaceSection>
            {submission && !hideEmbeddedMaterials ? (
                <DetailWorkspaceSection
                    title="合同、凭证及附件"
                    subtitle={
                        attachments.length
                            ? `共 ${attachments.length} 个文件`
                            : undefined
                    }
                    ariaLabel="合同、凭证及附件"
                >
                    <ApprovalMaterialFiles
                        instanceId={instanceId}
                        attachments={attachments}
                        idPrefix={`${prefix}-file`}
                        emptyMessage="本次提交未附带合同、凭证或其他附件。"
                    />
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}
