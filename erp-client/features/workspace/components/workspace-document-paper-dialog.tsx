"use client"

import type { MaterialReference } from "@/features/approval-workflow/hooks/use-approval-materials"
import { PrinterIcon } from "lucide-react"

import {
    BusinessFailureState,
    detailWorkspaceStyles,
} from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { PurchaseOrderPaperDocument } from "@/features/purchase-orders/components/purchase-order-paper-document"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { type WorkspacePaperKind } from "@/features/workspace/lib/paper-kind"
import { ApprovalCurrentSalesMaterials } from "@/features/approval-workflow/components/approval-current-sales-materials"
import { ApprovalSubmittedMaterials } from "@/features/approval-workflow/components/approval-submitted-materials"

export type WorkspacePaperTarget = Readonly<{
    kind: WorkspacePaperKind | "approval_snapshot"
    objectId: string
    title?: string
    materialsView?:
        | "all"
        | "source-sales"
        | "attachments"
        | "current-source-sales"
    sourceRevisionId?: string
}>

type WorkspaceDocumentPaperDialogProps = {
    reference?: MaterialReference
    target: WorkspacePaperTarget | null
    open: boolean
    onOpenChange: (open: boolean) => void
    closeButtonId?: string
}

/**
 * 工作台单据与提交资料预览：复用标准 `DialogContent size="2xl" scroll="body"` + `DialogBody`，
 * 固定顶部标题与关闭按钮，当前任务单据与来源销售单共用同一套预览。
 */
export function WorkspaceDocumentPaperDialog({
    target,
    reference,
    open,
    onOpenChange,
    closeButtonId = "workspace-document-paper-dialog-close",
}: WorkspaceDocumentPaperDialogProps) {
    const dialogTitle =
        target?.materialsView === "current-source-sales"
            ? "当前生效销售单预览"
            : target?.materialsView === "source-sales"
              ? "销售单预览"
              : target?.materialsView === "attachments"
                ? "合同、凭证及附件"
                : target?.title
                  ? `${target.title} ${target.kind === "approval_snapshot" ? "提交资料" : "纸质预览"}`
                  : "单据纸质预览"

    const dialogDescription =
        target?.materialsView === "current-source-sales"
            ? "当前生效销售版本的完整明细与合同凭证"
            : target?.materialsView === "source-sales"
              ? "本次审批锁定的完整来源销售单及允许读取的附件"
              : target?.materialsView === "attachments"
                ? "本次审批允许读取的合同、业务凭证及附件"
                : target?.kind === "approval_snapshot"
                  ? "此次审批提交时保留的单据及附件，按当前审批访问资格读取"
                  : "系统业务数据的打印件，金额与状态以系统记录为准"

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                size="2xl"
                scroll="body"
                closeButtonId={closeButtonId}
                className="bg-card"
            >
                <DialogHeader className="border-border/60 bg-card py-3 text-left print:hidden">
                    <DialogTitle className="min-w-0 truncate">
                        {dialogTitle}
                    </DialogTitle>
                    <DialogDescription>{dialogDescription}</DialogDescription>
                </DialogHeader>

                <DialogBody className="bg-card px-5 py-4">
                    {target?.kind === "approval_snapshot" &&
                    target.materialsView === "current-source-sales" ? (
                        <ApprovalCurrentSalesMaterials
                            bare
                            instanceId={target.objectId}
                            revisionId={target.sourceRevisionId}
                            enabled={open}
                        />
                    ) : target?.kind === "approval_snapshot" ? (
                        <ApprovalSubmittedMaterials
                            bare
                            instanceId={target.objectId}
                            reference={reference}
                            enabled={open}
                            view={
                                target.materialsView === "current-source-sales"
                                    ? "source-sales"
                                    : target.materialsView
                            }
                            sourceRevisionId={target.sourceRevisionId}
                        />
                    ) : target ? (
                        <WorkspacePaperBody
                            kind={target.kind}
                            reference={reference}
                            objectId={target.objectId}
                            enabled={open}
                        />
                    ) : null}
                </DialogBody>

                <DialogFooter className="items-center border-border/60 bg-card py-2.5 print:hidden">
                    {target?.materialsView !== "attachments" ? (
                        <Button
                            id={`${closeButtonId}-print`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={() => window.print()}
                        >
                            <PrinterIcon data-icon="inline-start" />
                            打印
                        </Button>
                    ) : null}
                    <Button
                        id={`${closeButtonId}-footer`}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => onOpenChange(false)}
                    >
                        关闭
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function WorkspacePaperBody({
    kind,
    objectId,
    enabled,
    reference,
}: {
    reference?: MaterialReference
    kind: WorkspacePaperKind
    objectId: string
    enabled: boolean
}) {
    const salesId = kind === "sales_order" && enabled ? objectId : ""
    const purchaseId = kind === "purchase_order" && enabled ? objectId : ""
    const salesQuery = useSalesOrderDetailQuery(
        salesId,
        true,
        reference?.businessObjectId === objectId
            ? reference.subjectVersion
            : undefined,
    )
    const purchaseQuery = usePurchaseOrderCenterQuery(purchaseId)

    if (kind === "sales_order") {
        if (salesQuery.isPending) return <PaperLoadingState />
        if (salesQuery.isError) {
            return (
                <PaperErrorState
                    error={salesQuery.error}
                    objectId={objectId}
                    onRetry={() => {
                        void salesQuery.refetch()
                    }}
                />
            )
        }
        if (!salesQuery.data) return <PaperMissingState />
        return <SalesOrderPaperDocument order={salesQuery.data} compact />
    }

    if (purchaseQuery.isPending) return <PaperLoadingState />
    if (purchaseQuery.isError) {
        return (
            <PaperErrorState
                error={purchaseQuery.error}
                objectId={objectId}
                onRetry={() => {
                    void purchaseQuery.refetch()
                }}
            />
        )
    }
    if (!purchaseQuery.data) return <PaperMissingState />
    const order = purchaseQuery.data
    if (
        reference?.businessObjectId === objectId &&
        (order.identity.purchaseOrderId !== objectId ||
            ![
                order.identity.currentRevisionId,
                order.identity.currentSubmissionId,
                String(order.currentContent.version),
            ].includes(reference.subjectVersion))
    ) {
        return (
            <PaperErrorState
                objectId={objectId}
                error={new Error("关联采购资料版本已变化，请刷新任务后重试")}
                onRetry={() => void purchaseQuery.refetch()}
            />
        )
    }
    return <PurchaseOrderPaperDocument order={purchaseQuery.data} compact />
}

function PaperLoadingState() {
    return (
        <div className="flex min-h-56 items-center justify-center py-10">
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
                <Spinner aria-label="正在读取单据" />
                正在读取单据…
            </p>
        </div>
    )
}

function PaperErrorState({
    error,
    objectId,
    onRetry,
}: {
    error: unknown
    objectId: string
    onRetry: () => void
}) {
    return (
        <div className="py-2">
            <BusinessFailureState
                id={`workspace-document-paper-dialog-${toAutomationIdSegment(objectId)}`}
                title="单据读取失败"
                error={error}
                onRetry={onRetry}
            />
        </div>
    )
}

function PaperMissingState() {
    return (
        <div className="py-8 text-center text-body-compact text-muted-foreground">
            未找到这张单据，可能已删除或当前角色无权查看。
        </div>
    )
}
