"use client"

import { useState, type ReactNode } from "react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    WorkspaceTaskFooter,
} from "@/components/business"
import { useWorkspaceInspectorMode } from "@/components/business/workspace-task-pane"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePurchaseOrderCenterQuery } from "../hooks/queries"
import { usePurchaseOrderDetailCommandState } from "../hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditActions } from "../hooks/use-purchase-order-detail-edit-actions"
import { usePurchaseOrderDetailPermissions } from "../hooks/use-purchase-order-detail-permissions"
import { purchaseChangeOrderApprovalPhase } from "../lib/purchase-change-order-approval"
import { sourceReturnProgress } from "../lib/source-return-progress"
import { MaintainSourceButton } from "./purchase-order-source-update-notice"
import { PurchaseOrderDetailDialogs } from "./purchase-order-detail-dialogs"
import { PurchaseChangeOrderEditDialog } from "./purchase-change-order-edit-dialog"
import { PurchaseChangeOrderApprovalSection } from "./purchase-change-order-approval-section"
import { PurchaseSourceUpdateDraftDialog } from "./purchase-source-update-draft-dialog"
import {
    PurchaseSourceUpdateLines,
    type PurchaseSourceLinesView,
} from "./purchase-source-update-lines"
import {
    buildPurchaseSourceImpact,
    PurchaseSourceUpdateContext,
} from "./purchase-source-update-context"
import {
    PurchaseSourceReturnAction,
    PurchaseSourceReturnPanel,
} from "./purchase-source-return-panel"

/** 依据更新作业面：影响、原单明细、改单进度；处理入口固定在工作台底栏。 */
export function PurchaseSourceUpdateWorkspace({
    purchaseOrderId,
    idPrefix = `purchase-source-update-${toAutomationIdSegment(purchaseOrderId)}`,
    onRefreshTask,
    returnTo,
    renderSurface = (body) => body,
}: {
    purchaseOrderId: string
    idPrefix?: string
    onRefreshTask: () => Promise<unknown>
    returnTo: string
    /** 保持工作面状态和待核对命令在切换右轨时不被重建。 */
    renderSurface?: (body: ReactNode) => ReactNode
}) {
    const inspectorMode = useWorkspaceInspectorMode()
    const query = usePurchaseOrderCenterQuery(purchaseOrderId)
    const order = query.data
    const permissions = usePurchaseOrderDetailPermissions(order)
    const { commandLedger, result, setResult } =
        usePurchaseOrderDetailCommandState(purchaseOrderId)
    const [jump, setJump] = useState(0)
    const [linesView, setLinesView] = useState<PurchaseSourceLinesView>({
        filter: "all",
        search: "",
        page: 0,
        jump: 0,
    })
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
        onFinished: () => {
            void onRefreshTask()
        },
    })
    if (query.isPending)
        return renderSurface(
            <div
                className="space-y-4 bg-card py-5"
                aria-busy="true"
                aria-label="正在加载销售变更对比信息"
            >
                <div className="h-24 animate-pulse rounded-lg bg-surface-sunken" />
                <div className="h-40 animate-pulse rounded-lg bg-surface-sunken" />
            </div>,
        )
    if (query.isError && !order)
        return renderSurface(
            <BusinessFailureState
                title="销售变更对比信息加载失败"
                error={query.error}
                action={
                    <Button
                        id={`${idPrefix}-retry`}
                        variant="outline"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                }
            />,
        )
    if (!order)
        return renderSurface(
            <BusinessEmptyState
                kind="no-data"
                title="暂时无法查看采购单"
                description="该采购单不存在或不在当前账号的数据范围内，请核对采购单与查看权限。"
            />,
        )

    const draft = draftOpen ? (
        <PurchaseSourceUpdateDraftDialog
            order={order}
            idPrefix={`${idPrefix}-draft`}
            inline={inspectorMode}
            onClose={() => setDraftOpen(false)}
            onSubmitted={() => void onRefreshTask()}
        />
    ) : null
    if (draft && inspectorMode) return renderSurface(draft)

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
        permissions.canContinueChange &&
        activeChange &&
        phase === "draft" &&
        activeChange.approval?.allowedActions.includes("SUBMIT"),
    )
    const awaitingApproval = phase === "runtime"
    const needsUpdate = Boolean(update?.affected || update?.awaitingSubmission)
    const openChange = () => {
        if (actions.changePending) return
        if (
            continueChange &&
            activeChange &&
            !commandLedger.peek("start-change")
        ) {
            setEditingChangeId(activeChange.id)
            return
        }
        void actions.handleStartChange().then((id) => {
            if (id) setEditingChangeId(id)
        })
    }

    const impact = buildPurchaseSourceImpact(order)
    const uncertainVoid =
        result?.status === "unknown" && Boolean(commandLedger.peek("void"))
    const uncertainChange =
        result?.status === "unknown" &&
        Boolean(commandLedger.peek("start-change"))
    const recovering = uncertainVoid || uncertainChange
    const writePending = actions.voidPending || actions.changePending
    const resolution = awaitingApproval
        ? "改单审批中"
        : update?.awaitingSubmission
          ? "已保存，待提交审批"
          : needsUpdate
            ? returnProgress.physicalPending
                ? "待登记供应商收货"
                : returnProgress.blocksChange
                  ? returnProgress.decisionCurrent
                      ? "财务处理中"
                      : "待确认款项处理方式"
                  : continueChange
                    ? "待提交采购改单"
                    : "待调整采购"
            : update?.lastResolutionLabel || "当前无需调整"
    const canMaintain =
        update?.affected && !returnProgress.hasOpenReturns && !awaitingApproval
    const showReturnAction = returnProgress.blocksChange && !awaitingApproval
    const footerSummary = (
        <dl
            aria-live="polite"
            className="mr-auto flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-1 text-xs"
        >
            <div className="flex items-baseline gap-1.5">
                <dt className="text-muted-foreground">受影响</dt>
                <dd className="num font-medium">{impact.byLine.size} 行</dd>
            </div>
            <div className="flex items-baseline gap-1.5">
                <dt className="text-muted-foreground">处理状态</dt>
                <dd className="font-medium">
                    {recovering ? "结果待核对" : resolution}
                </dd>
            </div>
        </dl>
    )
    const footerActions = recovering ? (
        <LoadingButton
            id={`${idPrefix}-recover`}
            variant="default"
            size="sm"
            loading={writePending}
            onClick={() =>
                uncertainVoid ? void actions.handleVoid() : openChange()
            }
        >
            核对提交结果
        </LoadingButton>
    ) : needsUpdate && !query.isError ? (
        <>
            {impact.byLine.size > 0 || impact.unmatched.length > 0 ? (
                <Button
                    id={`${idPrefix}-footer-locate`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setJump((current) => current + 1)}
                >
                    {impact.byLine.size > 0 ? (
                        <>
                            <span className="num">{impact.byLine.size}</span>{" "}
                            行受影响 · 定位
                        </>
                    ) : (
                        "定位销售变更"
                    )}
                </Button>
            ) : null}
            {canMaintain && !writePending ? (
                <MaintainSourceButton
                    purchaseOrderId={purchaseOrderId}
                    update={update}
                    label="维持原采购"
                    onMaintained={() => {
                        void Promise.all([query.refetch(), onRefreshTask()])
                    }}
                />
            ) : null}
            {permissions.canVoid ? (
                <Button
                    id={`${idPrefix}-void`}
                    variant="outline"
                    size="sm"
                    disabled={writePending}
                    onClick={() => actions.setVoidConfirmOpen(true)}
                >
                    作废草稿
                </Button>
            ) : null}
            {showReturnAction ? (
                <PurchaseSourceReturnAction
                    order={order}
                    disabled={writePending}
                />
            ) : permissions.canEdit ? (
                <Button
                    id={`${idPrefix}-edit`}
                    variant="default"
                    size="sm"
                    disabled={writePending}
                    onClick={() => setDraftOpen(true)}
                >
                    {update?.awaitingSubmission ? "核对并提交审批" : "修改原单"}
                </Button>
            ) : !awaitingApproval &&
              (permissions.canChange || continueChange) ? (
                <LoadingButton
                    id={`${idPrefix}-change`}
                    variant="default"
                    size="sm"
                    loading={actions.changePending}
                    disabled={actions.voidPending}
                    onClick={openChange}
                >
                    {continueChange ? "继续改单" : "发起改单"}
                </LoadingButton>
            ) : !awaitingApproval ? (
                <span className="text-xs text-muted-foreground">
                    当前不可修改采购
                </span>
            ) : null}
        </>
    ) : null

    return (
        <>
            {renderSurface(
                <div className="min-w-0 space-y-4 bg-card py-4 @container/source-update">
                    <PurchaseSourceUpdateContext
                        order={order}
                        idPrefix={idPrefix}
                        impact={impact}
                        resolution={resolution}
                        returnTo={returnTo}
                        onShowImpacted={() => setJump((current) => current + 1)}
                    />
                    {query.isError ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            当前显示上次加载的采购情况，请刷新后再处理。
                        </p>
                    ) : null}
                    {inspectorMode && actions.voidConfirmOpen ? (
                        <Alert variant="destructive" className="border-l-2">
                            <AlertTitle>确认作废采购草稿</AlertTitle>
                            <AlertDescription className="space-y-2">
                                <p>
                                    作废后本采购草稿不可恢复，关联依据变更待办将同步关闭。
                                </p>
                                <div className="flex flex-wrap gap-2">
                                    <Button
                                        id={`${idPrefix}-inline-void-cancel`}
                                        type="button"
                                        size="xs"
                                        variant="ghost"
                                        disabled={actions.voidPending}
                                        onClick={() =>
                                            actions.setVoidConfirmOpen(false)
                                        }
                                    >
                                        取消
                                    </Button>
                                    <LoadingButton
                                        id={`${idPrefix}-inline-void-confirm`}
                                        type="button"
                                        size="xs"
                                        variant="destructive"
                                        loading={actions.voidPending}
                                        onClick={() =>
                                            void actions.handleVoid()
                                        }
                                    >
                                        确认作废草稿
                                    </LoadingButton>
                                </div>
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {result ? <FormalActionResult {...result} /> : null}
                    <PurchaseSourceUpdateLines
                        order={order}
                        idPrefix={idPrefix}
                        impact={impact}
                        jump={jump}
                        view={linesView}
                        onViewChange={setLinesView}
                    />
                    {needsUpdate && showReturnAction ? (
                        <div className="border-t border-border/60 pt-4 [&>section]:rounded-none [&>section]:border-0 [&>section]:p-0">
                            <PurchaseSourceReturnPanel
                                order={order}
                                showAction={false}
                                disabled={query.isError || recovering}
                            />
                        </div>
                    ) : null}
                    {activeChange ? (
                        <div className="border-t border-border/60 pt-4 [&>div>section]:rounded-none [&>div>section]:border-0 [&>div>section]:bg-transparent [&>div>section]:p-0!">
                            <PurchaseChangeOrderApprovalSection
                                readonlyApproval
                                purchaseOrderId={purchaseOrderId}
                                changeOrder={activeChange}
                                baseRevisionNo={order.identity.revisionNo}
                                paidAmount={
                                    order.payableSummary?.paidAllocatedAmount
                                }
                                onResult={setResult}
                            />
                        </div>
                    ) : null}
                    <WorkspaceTaskFooter
                        summary={footerSummary}
                        fallback={
                            <div className="flex w-full min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-2">
                                {footerSummary}
                                <div className="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-2">
                                    {footerActions}
                                </div>
                            </div>
                        }
                    >
                        {footerActions}
                    </WorkspaceTaskFooter>
                </div>,
            )}
            {!inspectorMode ? (
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
            ) : null}
            {draft}
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
        </>
    )
}
