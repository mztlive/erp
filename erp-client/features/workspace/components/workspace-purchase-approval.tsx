"use client"

import { ChevronRightIcon, FileTextIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { WorkspaceSupplierRating } from "./workspace-supplier-rating"
import { WorkspacePurchaseComparisonTable } from "./workspace-purchase-comparison-table"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

/** 采购审批只消费实例授权的提交资料；来源入口保持在同一审批上下文。 */
export function WorkspacePurchaseApproval({
    instanceId,
    onPreview,
}: {
    instanceId: string
    onPreview: (target: WorkspacePaperTarget) => void
}) {
    const query = useApprovalMaterials(instanceId, true)
    const prefix = `workspace-purchase-approval-${toAutomationIdSegment(instanceId)}`
    const sectionClass = cn(
        workspaceTaskSurfacePadClassName,
        "border-b border-grid py-4",
    )
    const preview = (view: "all" | "source-sales", revisionId?: string) =>
        onPreview({
            kind: "approval_snapshot",
            objectId: instanceId,
            materialsView: view,
            sourceRevisionId: revisionId,
        })

    if (query.isPending)
        return (
            <>
                <section
                    className={sectionClass}
                    aria-label="供应商与客户"
                    role="status"
                >
                    <p className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Spinner />
                        正在读取供应商与客户…
                    </p>
                </section>
            </>
        )
    if (query.isError)
        return (
            <>
                <section className={sectionClass}>
                    <BusinessFailureState
                        id={`${prefix}-retry`}
                        title="供应商与客户读取失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                </section>
            </>
        )

    const {
        display,
        purchase_lines: lines,
        extra_cost_summary: extraCost,
    } = query.data
    const sources = display.source_sales ?? []
    return (
        <>
            <section
                className={cn(
                    sectionClass,
                    "grid gap-5 bg-info-soft/30 @min-[720px]/document:grid-cols-2 @min-[720px]/document:gap-6",
                )}
                aria-label="供应商与客户"
            >
                <div className="min-w-0 space-y-3">
                    <h3 className="text-sm font-medium text-muted-foreground">
                        供应商
                    </h3>
                    <p className="text-sm font-semibold wrap-anywhere">
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
                <div className="min-w-0 space-y-3 border-t border-grid pt-4 @min-[720px]/document:border-t-0 @min-[720px]/document:border-l @min-[720px]/document:pt-0 @min-[720px]/document:pl-6">
                    <header className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <h3 className="text-sm font-medium text-muted-foreground">
                            客户与销售单
                        </h3>
                        <p className="text-xs text-muted-foreground">
                            依据本次提交锁定的销售版本
                        </p>
                    </header>
                    {sources.length ? (
                        sources.map((sales) => (
                            <div
                                key={sales.revision_id}
                                className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2"
                            >
                                <p className="min-w-0 basis-full text-sm font-semibold wrap-anywhere">
                                    {displayName(sales.source.customer) ||
                                        "未记录客户名称"}
                                </p>
                                <p className="min-w-0 text-sm wrap-anywhere">
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
                                    variant="outline"
                                    size="sm"
                                    className="ml-auto"
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
                                            className="text-sm text-warning-soft-foreground basis-full"
                                        >
                                            {field.value}
                                        </p>
                                    ))}
                            </div>
                        ))
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            此次提交未保留来源销售资料，请联系申请人核对后再审批。
                        </p>
                    )}
                </div>
            </section>

            <section
                className={cn(sectionClass, "space-y-4")}
                aria-label="采购与销售对照"
            >
                <header className="flex items-baseline gap-2">
                    <h3 className="text-sm font-semibold">采购与销售对照</h3>
                    {lines && (
                        <span className="text-xs text-muted-foreground">
                            共 {lines.length} 项
                        </span>
                    )}
                </header>
                {extraCost ? (
                    <p className="rounded-md bg-muted/50 px-3 py-2 text-sm text-muted-foreground">
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
                        <p className="text-sm text-muted-foreground">
                            此次提交暂无可用的逐行对照，请核对提交资料及附件。
                        </p>
                        {display.source.lines.length > 0 && (
                            <ul className="divide-y text-sm">
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
                        <Button
                            id={`${prefix}-submission`}
                            variant="outline"
                            size="sm"
                            onClick={() => preview("all")}
                        >
                            <FileTextIcon aria-hidden="true" />
                            查看提交资料
                        </Button>
                    </>
                )}
            </section>
        </>
    )
}
