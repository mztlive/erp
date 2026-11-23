"use client"

import { ChevronRightIcon, FileTextIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { WorkspaceSupplierRating } from "./workspace-supplier-rating"
import { WorkspacePurchaseComparisonTable } from "./workspace-purchase-comparison-table"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

/** 采购审批只消费实例授权的提交资料；来源入口保持在同一审批上下文。 */
export function WorkspacePurchaseApproval({
    instanceId,
    item,
    onPreview,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onPreview: (target: WorkspacePaperTarget) => void
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
    return (
        <>
            <DetailWorkspaceSection
                contentClassName="grid gap-5 space-y-0 @min-[720px]/document:grid-cols-2 @min-[720px]/document:gap-6"
                ariaLabel="供应商与客户"
            >
                <div className="min-w-0 space-y-3">
                    <h2 className={detailWorkspaceStyles.sectionTitle}>
                        供应商
                    </h2>
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
                    />
                </div>
                <div className="min-w-0 space-y-3 border-t border-border/60 pt-4 @min-[720px]/document:border-t-0 @min-[720px]/document:border-l @min-[720px]/document:pt-0 @min-[720px]/document:pl-6">
                    <h2 className={detailWorkspaceStyles.sectionTitle}>
                        客户与销售单
                    </h2>
                    {sources.length ? (
                        sources.map((sales) => (
                            <div
                                key={sales.revision_id}
                                className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2"
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
                {extraCost ? (
                    <p className="rounded-md bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
                        来源销售单已有附加费用（不含本次提交）：已发生{" "}
                        <MoneyValue value={extraCost.occurred_gross_total} />
                        ，待发生{" "}
                        <MoneyValue value={extraCost.pending_gross_total} />
                        。附加费用不向客户收费，会直接减少本单利润。
                    </p>
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
                />
            </DetailWorkspaceSection>
        </>
    )
}
