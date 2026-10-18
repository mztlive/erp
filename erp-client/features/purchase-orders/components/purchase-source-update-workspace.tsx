"use client"

import {
    PurchaseSourceReturnPanel,
    PurchaseSourceReturnAction,
} from "./purchase-source-return-panel"
import { sourceReturnProgress } from "../lib/source-return-progress"

import { useState } from "react"
import Link from "next/link"
import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    WorkspaceTaskFooter,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePurchaseOrderCenterQuery } from "../hooks/queries"
import { usePurchaseOrderDetailCommandState } from "../hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditActions } from "../hooks/use-purchase-order-detail-edit-actions"
import { usePurchaseOrderDetailPermissions } from "../hooks/use-purchase-order-detail-permissions"
import { purchaseChangeOrderApprovalPhase } from "../lib/purchase-change-order-approval"
import {
    PurchaseOrderSourceUpdateNotice,
    MaintainSourceButton,
} from "./purchase-order-source-update-notice"
import { PurchaseOrderDetailDialogs } from "./purchase-order-detail-dialogs"
import { PurchaseChangeOrderEditDialog } from "./purchase-change-order-edit-dialog"
import { PurchaseChangeOrderApprovalSection } from "./purchase-change-order-approval-section"
import { PurchaseSourceUpdateDraftDialog } from "./purchase-source-update-draft-dialog"
import { PurchaseSourceUpdateLines } from "./purchase-source-update-lines"

/** 依据更新作业面：影响、原单明细、改单进度；处理入口固定在工作台底栏。 */
export function PurchaseSourceUpdateWorkspace({
    purchaseOrderId,
    onRefreshTask,
    returnTo,
}: {
    purchaseOrderId: string
    onRefreshTask: () => Promise<unknown>
    returnTo: string
}) {
    const query = usePurchaseOrderCenterQuery(purchaseOrderId)
    const order = query.data
    const permissions = usePurchaseOrderDetailPermissions(order)
    const { commandLedger, result, setResult } =
        usePurchaseOrderDetailCommandState(purchaseOrderId)
    const [draftOpen, setDraftOpen] = useState(false)
    const [editingChangeId, setEditingChangeId] = useState<string | null>(null)
    const actions = usePurchaseOrderDetailEditActions({
        purchaseOrderId,
        order,
        mode: "view",
        refetch: query.refetch,
        commandLedger,
        setResult,
        getPaymentTermCode: () => order?.header.paymentTermCode ?? "",
        setDraftPaymentTermCode: () => undefined,
        onFinished: () => undefined,
    })
    if (query.isPending)
        return (
            <div
                className="space-y-4 p-5"
                aria-busy="true"
                aria-label="正在加载采购依据"
            >
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </div>
        )
    if (query.isError && !order)
        return (
            <BusinessFailureState
                title="采购依据加载失败"
                error={query.error}
                action={
                    <Button
                        id="purchase-source-update-retry"
                        variant="outline"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        )
    if (!order)
        return (
            <BusinessEmptyState
                kind="no-data"
                title="暂时无法查看采购单"
                description="该采购单不存在或不在当前账号的数据范围内，请核对采购单与查看权限。"
            />
        )

    const returnProgress = sourceReturnProgress(order)
    const update = order.sourceUpdate
    const activeChange = order.activeChangeOrder
    const phase = activeChange
        ? purchaseChangeOrderApprovalPhase(
              activeChange.approval,
              activeChange.statusCode,
          )
        : null
    const continueChange = Boolean(
        activeChange &&
        phase === "draft" &&
        activeChange.approval?.allowedActions.includes("SUBMIT"),
    )
    const awaitingApproval = phase === "runtime"
    const needsUpdate = Boolean(update?.affected || update?.awaitingSubmission)
    const refresh = () => {
        void Promise.all([query.refetch(), onRefreshTask()])
    }
    const openChange = () => {
        if (actions.changePending) return
        if (continueChange && activeChange) {
            setEditingChangeId(activeChange.id)
            return
        }
        void actions.handleStartChange().then((id) => {
            if (id) setEditingChangeId(id)
        })
    }

    return (
        <div className="min-w-0 space-y-5 px-5 py-5 @container/document">
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
                <StatusBadge
                    label={order.identity.statusLabel}
                    tone={order.identity.statusTone}
                />
                <span>
                    来源销售：
                    <Link
                        id={`purchase-source-update-sales-${toAutomationIdSegment(purchaseOrderId)}`}
                        href={`/sales/orders/${encodeURIComponent(order.header.salesOrderId)}?${new URLSearchParams({ from: "workspace", returnTo })}`}
                        className="num break-all underline decoration-border underline-offset-4 hover:decoration-foreground focus-visible:outline-2 focus-visible:outline-offset-4"
                        aria-label={`打开销售单 ${order.header.salesOrderNo}`}
                    >
                        {order.header.salesOrderNo}
                    </Link>
                </span>
                <span className="text-muted-foreground">
                    {order.header.paymentTermLabel}
                </span>
            </div>
            {query.isError ? (
                <Alert variant="warning">
                    <AlertTitle>最新采购情况读取失败</AlertTitle>
                    <AlertDescription>
                        当前保留上次内容和已输入的修改，请刷新后再处理。
                    </AlertDescription>
                </Alert>
            ) : null}
            {needsUpdate ? (
                <PurchaseOrderSourceUpdateNotice
                    order={order}
                    showActions={false}
                    showGuidance={false}
                    canEdit={false}
                    canVoid={false}
                    canChange={false}
                    onEdit={() => undefined}
                    onRequestVoid={() => undefined}
                    onRequestChange={() => undefined}
                />
            ) : null}
            {!update ? (
                <Alert variant="warning">
                    <AlertTitle>暂时无法确认采购依据</AlertTitle>
                    <AlertDescription>
                        未读取到采购依据更新情况，请刷新后再处理。
                    </AlertDescription>
                </Alert>
            ) : !needsUpdate ? (
                <Alert>
                    <AlertTitle>本采购已无需更新依据</AlertTitle>
                    <AlertDescription>
                        {update.lastResolutionLabel ||
                            "当前采购已不受来源销售变更影响。"}{" "}
                        {update.lastMaintainNote
                            ? `维持原因：${update.lastMaintainNote}。`
                            : null}
                        待办状态由系统同步。
                    </AlertDescription>
                </Alert>
            ) : awaitingApproval ? (
                <Alert>
                    <AlertTitle>采购改单审批中</AlertTitle>
                    <AlertDescription>
                        改单生效后系统会重新核对采购依据，符合新销售要求后自动结束待办。当前无需重复发起改单。
                    </AlertDescription>
                </Alert>
            ) : null}
            {needsUpdate && !awaitingApproval ? (
                <PurchaseSourceReturnPanel
                    order={order}
                    showAction={false}
                    disabled={query.isError || awaitingApproval}
                />
            ) : null}
            {result ? <FormalActionResult {...result} /> : null}
            {activeChange ? (
                <PurchaseChangeOrderApprovalSection
                    readonlyApproval
                    purchaseOrderId={purchaseOrderId}
                    changeOrder={activeChange}
                    baseRevisionNo={order.identity.revisionNo}
                    paidAmount={order.payableSummary?.paidAllocatedAmount}
                    onResult={setResult}
                />
            ) : null}
            <section className="min-w-0 space-y-3" aria-label="当前采购明细">
                <h3 className="text-sm font-medium">当前采购明细</h3>
                <PurchaseSourceUpdateLines order={order} />
            </section>
            <WorkspaceTaskFooter>
                <p className="mr-auto text-xs text-muted-foreground">
                    {update?.awaitingSubmission
                        ? "已保存修改，提交审批后完成待办。"
                        : awaitingApproval
                          ? "待改单生效后，系统自动核对处理结果。"
                          : needsUpdate
                            ? returnProgress.physicalPending
                                ? "核实供应商实际收货数量后登记。"
                                : returnProgress.blocksChange
                                  ? returnProgress.decisionCurrent
                                      ? "财务正在处理，完成后可继续改单。"
                                      : "先与供应商确定款项处理方式，再交接财务。"
                                  : "请根据影响原因调整采购；无法调整时填写维持原因。"
                            : "按最新采购情况核对待办状态。"}
                </p>
                <Button
                    id="purchase-source-update-refresh"
                    variant="ghost"
                    disabled={
                        query.isFetching ||
                        actions.changePending ||
                        actions.voidPending
                    }
                    onClick={refresh}
                >
                    刷新
                </Button>
                {needsUpdate && !query.isError ? (
                    <>
                        {update?.affected &&
                        !returnProgress.hasOpenReturns &&
                        !awaitingApproval &&
                        !actions.changePending &&
                        !actions.voidPending ? (
                            <MaintainSourceButton
                                purchaseOrderId={purchaseOrderId}
                                update={update}
                            />
                        ) : null}
                        {!awaitingApproval ? (
                            <PurchaseSourceReturnAction
                                order={order}
                                disabled={
                                    actions.changePending || actions.voidPending
                                }
                            />
                        ) : null}
                        {permissions.canVoid ? (
                            <Button
                                id="purchase-source-update-void"
                                variant="outline"
                                disabled={actions.voidPending}
                                onClick={() => actions.setVoidConfirmOpen(true)}
                            >
                                作废草稿
                            </Button>
                        ) : null}
                        {permissions.canEdit ? (
                            <Button
                                id="purchase-source-update-edit"
                                disabled={actions.voidPending}
                                onClick={() => setDraftOpen(true)}
                            >
                                {update?.awaitingSubmission
                                    ? "核对并提交审批"
                                    : "修改原单"}
                            </Button>
                        ) : !awaitingApproval &&
                          !returnProgress.blocksChange &&
                          (permissions.canChange || continueChange) ? (
                            <LoadingButton
                                id="purchase-source-update-change"
                                loading={actions.changePending}
                                onClick={openChange}
                            >
                                {continueChange ? "继续改单" : "发起改单"}
                            </LoadingButton>
                        ) : null}
                        {!returnProgress.blocksChange &&
                        !permissions.canEdit &&
                        !permissions.canChange &&
                        !continueChange &&
                        !awaitingApproval ? (
                            <span className="text-xs text-muted-foreground">
                                当前单据状态或权限不支持修改采购。
                            </span>
                        ) : null}
                    </>
                ) : null}
            </WorkspaceTaskFooter>
            <PurchaseOrderDetailDialogs
                order={order}
                submitConfirmOpen={false}
                onSubmitConfirmOpenChange={() => undefined}
                voidConfirmOpen={actions.voidConfirmOpen}
                onVoidConfirmOpenChange={actions.setVoidConfirmOpen}
                leaveGuardOpen={false}
                onLeaveGuardOpenChange={() => undefined}
                submitPending={false}
                savePending={false}
                voidPending={actions.voidPending}
                onConfirmSubmit={() => undefined}
                onConfirmVoid={() => void actions.handleVoid()}
                onSaveAndLeave={() => undefined}
                onDiscardAndLeave={() => undefined}
            />
            {draftOpen ? (
                <PurchaseSourceUpdateDraftDialog
                    order={order}
                    onClose={() => setDraftOpen(false)}
                />
            ) : null}
            {editingChangeId ? (
                <PurchaseChangeOrderEditDialog
                    open
                    purchaseOrderId={purchaseOrderId}
                    changeOrderId={editingChangeId}
                    approval={
                        activeChange?.id === editingChangeId
                            ? activeChange.approval
                            : undefined
                    }
                    paidAmount={order.payableSummary?.paidAllocatedAmount}
                    onOpenChange={(open) => {
                        if (!open) setEditingChangeId(null)
                    }}
                    onResult={setResult}
                />
            ) : null}
        </div>
    )
}
