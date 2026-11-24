"use client"

import { useEffect, type ReactNode } from "react"
import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { GuardedBusinessAction } from "@/components/business"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { hasPermission } from "@/lib/permissions"

import { clearAddressReveal } from "../api/index"
import {
    useAddNoteMutation,
    useAfterSalesActionMutation,
    useCompleteOrderTaskMutation,
    useQueryResultMutation,
    useReplayOrderMutation,
    useRevealAddressMutation,
} from "../hooks/queries"
import {
    useSupplierOrderCenterActions,
    useSupplierOrderCenterResult,
} from "../hooks/use-supplier-order-center-actions"
import { useSupplierOrderCenterDerivation } from "../hooks/use-supplier-order-center-derivation"
import { useSupplierOrderCenterCommandIdentity } from "../hooks/use-supplier-order-center-identity"
import { useSupplierOrderCenterOrderActions } from "../hooks/use-supplier-order-center-order-actions"
import { useSupplierOrderCenterTaskActions } from "../hooks/use-supplier-order-center-task-actions"
import type { FormalActionResponse, SupplierOrderDetailView } from "../types"

import { SupplierOrderExecutionProgress } from "./supplier-order-execution-progress"
import {
    SupplierOrderExecutionSetupPanel,
    useSupplierOrderExecutionSetup,
} from "./supplier-order-execution-setup"
import { AftersalesSection } from "./supplier-order-preview-center-aftersales-costs"
import { FulfillmentSection } from "./supplier-order-preview-center-fulfillment"
import { useSupplierInvestigationCommands } from "./supplier-order-investigation-command"
import {
    SupplierInvestigationDiagnostics,
    SupplierInvestigationNote,
    investigationLabel,
    remoteInvestigationLabel,
} from "./supplier-order-investigation-diagnostics"
import { SupplierInvestigationCanvas } from "./supplier-order-investigation-canvas"

const investigatePermission = "supplier_fulfillment_order:investigate"
const completePermission = "supplier_fulfillment_order:complete"
const submitPermission = "supplier_fulfillment_order:submit"

export type SupplierOrderInvestigationHost = {
    idPrefix: string
    onOpenDiagnostics: () => void
    render: (slots: {
        body: ReactNode
        order?: SupplierOrderDetailView["order"]
        diagnostics?: ReactNode
        fulfillment?: ReactNode
    }) => ReactNode
}

export function SupplierOrderInvestigation({
    host,
    detail,
    workItemId,
    permissions,
    currentUserId,
    refresh,
    refreshing,
    refreshFailed,
    onTaskCompleted,
}: {
    host: SupplierOrderInvestigationHost
    detail: SupplierOrderDetailView
    workItemId?: string
    permissions: Parameters<typeof hasPermission>[0]
    currentUserId?: string
    refresh: () => Promise<{ isSuccess: boolean }>
    refreshing: boolean
    refreshFailed: boolean
    onTaskCompleted?: (workItemId: string) => void
}) {
    const { idPrefix, onOpenDiagnostics } = host
    const { order } = detail
    const { result, setResult } = useSupplierOrderCenterResult()
    const identity = useSupplierOrderCenterCommandIdentity()
    const commands = useSupplierInvestigationCommands({ permissions, refresh })
    const queryMutation = useQueryResultMutation()
    const replayMutation = useReplayOrderMutation()
    const completeMutation = useCompleteOrderTaskMutation()
    const afterSalesMutation = useAfterSalesActionMutation()
    const revealMutation = useRevealAddressMutation()
    const noteMutation = useAddNoteMutation()
    const execution = useSupplierOrderExecutionSetup(order.id)
    const recoverResult = (label: string, response: FormalActionResponse) => {
        setResult({
            status: response.status === "failed" ? "rejected" : response.status,
            title: label,
            description: response.message,
        })
    }
    const actions = useSupplierOrderCenterActions({
        orderId: order.id,
        workItemId,
        detail,
        currentUserId,
        setResult,
        queryResultMutation: commands.wrap(queryMutation, {
            kind: "query",
            label: "查询原下单结果",
            onSettled: () =>
                identity.forgetCommandIdentity(`query:${order.id}`),
            permissions: [investigatePermission],
            onRecovered: (response) =>
                recoverResult("原下单结果核对", response),
        }),
        replayMutation: commands.wrap(replayMutation, {
            kind: "replay",
            label: "安全重发",
            onSettled: () =>
                identity.forgetCommandIdentity(`replay:${order.id}`),
            permissions: [investigatePermission],
            onRecovered: (response) => recoverResult("重发结果核对", response),
        }),
        ...identity,
    })
    const derivation = useSupplierOrderCenterDerivation({
        detail,
        currentUserId,
        latestInvestigation: actions.latestInvestigation,
    })
    const task = useSupplierOrderCenterTaskActions({
        detail,
        completionEvidence: derivation.completionEvidence,
        refetch: refresh,
        setResult,
        completeTaskMutation: commands.wrap(completeMutation, {
            kind: "complete",
            label: "完成任务",
            onSettled: () => {
                if (workItemId)
                    identity.forgetCommandIdentity(`complete:${workItemId}`)
            },
            permissions: [completePermission],
            onRecovered: (response) => {
                recoverResult("任务完成结果核对", response)
                if (response.status === "succeeded" && response.data) {
                    onTaskCompleted?.(response.data.workItemId)
                }
            },
        }),
        onTaskCompleted,
        ...identity,
    })
    const orderActions = useSupplierOrderCenterOrderActions({
        orderId: order.id,
        detail,
        setResult,
        afterSalesMutation: commands.wrap(afterSalesMutation, {
            kind: "after-sales",
            label: "提交售后",
            permissions: (input) => [
                `supplier_fulfillment_order:${input.action.toLowerCase()}`,
            ],
            onRecovered: (response) => recoverResult("售后结果核对", response),
        }),
        revealMutation,
    })

    useEffect(
        () => () => {
            void clearAddressReveal(order.id)
        },
        [order.id],
    )

    const investigation = [
        actions.latestInvestigation,
        queryMutation.data?.data?.evidence,
        replayMutation.data?.data?.evidence,
        detail.lastInvestigation,
    ]
        .filter((evidence) => evidence !== undefined)
        .sort((left, right) =>
            right.recordedAt.localeCompare(left.recordedAt),
        )[0]
    const remoteEvent = detail.statusHistory
        .filter(
            (event) =>
                event.source === "supplier_callback" &&
                event.track === "fulfillment",
        )
        .sort((left, right) => right.at.localeCompare(left.at))[0]
    const investigationIsLatest = Boolean(
        investigation &&
        (!remoteEvent || investigation.recordedAt >= remoteEvent.at),
    )
    const lastSync = investigationIsLatest
        ? investigation?.recordedAt
        : remoteEvent?.at
    const remoteStatus = investigationIsLatest
        ? remoteInvestigationLabel(investigation)
        : remoteEvent?.toLabel
    const blocker = (action: string) =>
        detail.actionBlockers.find((item) => item.action === action)?.message

    const setupRecovery =
        hasPermission(permissions, submitPermission) &&
        execution.saveOutcomeUnknown &&
        execution.mutation.variables
    const resolving = Boolean(commands.recovery || setupRecovery)
    const executionLocked =
        execution.mutation.isPending ||
        execution.resultQuery.isPending ||
        execution.saveOutcomeUnknown
    const locked =
        commands.pending || resolving || executionLocked || refreshFailed
    const canInvestigate = hasPermission(permissions, investigatePermission)
    const canComplete = hasPermission(permissions, completePermission)
    const ownsTask = derivation.responsibilityStatus === "assigned_to_me"
    const showQuery =
        canInvestigate && (derivation.isResultUnknown || derivation.canQuery)
    const queryAllowed = showQuery && ownsTask && derivation.canQuery
    const replayAllowed = canInvestigate && ownsTask && derivation.canReplay
    const completeAllowed = canComplete && derivation.canCompleteTask
    const afterSales = orderActions.afterSalesConfirm
    const canConfirmAfterSales = Boolean(
        afterSales &&
        hasPermission(
            permissions,
            `supplier_fulfillment_order:${afterSales.action.toLowerCase()}`,
        ) &&
        detail.afterSales
            .find((entry) => entry.requestId === afterSales.requestId)
            ?.allowedActions.includes(afterSales.action),
    )
    const confirming =
        !resolving &&
        !refreshFailed &&
        ((actions.replayOpen && replayAllowed) ||
            (task.completeOpen && completeAllowed) ||
            canConfirmAfterSales)
    const confirmationId = `${idPrefix}-confirmation`
    useEffect(() => {
        if (!confirming) return
        const element = document.getElementById(confirmationId)
        element?.focus({ preventScroll: true })
        element?.scrollIntoView({ block: "nearest" })
    }, [confirmationId, confirming])

    const closeConfirmation = () => {
        actions.setReplayOpen(false)
        task.setCompleteOpen(false)
        orderActions.setAfterSalesConfirm(null)
    }
    const confirmationLabel = actions.replayOpen
        ? "确认安全重发"
        : task.completeOpen
          ? "确认完成任务"
          : afterSales?.action === "CANCEL"
            ? "确认提交取消"
            : "确认提交退款"

    const confirm = async () => {
        if (locked) return
        if (actions.replayOpen && replayAllowed) {
            await actions.handleReplay()
            // 重发收到证据时需要更新后续允许的动作，包括仍需核实的结果。
            await refresh()
        } else if (task.completeOpen && completeAllowed) {
            await task.handleCompleteTask()
        } else if (afterSales && canConfirmAfterSales) {
            await orderActions.handleAfterSales(
                afterSales.action,
                afterSales.requestId,
            )
        }
        closeConfirmation()
    }

    const summary = (
        <dl
            aria-live="polite"
            className="flex min-w-0 flex-wrap items-baseline gap-x-2 text-xs"
        >
            <dt className="text-muted-foreground">核实状态</dt>
            <dd className="font-medium">
                {commands.recovery
                    ? "操作结果待核对"
                    : commands.pending
                      ? "正在处理…"
                      : detail.workItem?.workItemStatus === "COMPLETED"
                        ? "任务已完成"
                        : detail.workItem?.workItemStatus === "CLOSED"
                          ? "任务已关闭"
                          : investigationLabel(investigation)}
            </dd>
        </dl>
    )

    let primary: ReactNode
    if (commands.recovery && commands.canRecover) {
        const recovery = commands.recovery
        primary = (
            <LoadingButton
                id={`${idPrefix}-recover`}
                size="sm"
                loading={commands.pending}
                disabled={commands.pending}
                onClick={() => void recovery.retry()}
            >
                核对提交结果
            </LoadingButton>
        )
    } else if (setupRecovery) {
        primary = (
            <LoadingButton
                id={`${idPrefix}-recover-setup`}
                size="sm"
                loading={execution.mutation.isPending}
                disabled={
                    execution.mutation.isPending || execution.query.isFetching
                }
                onClick={() => {
                    onOpenDiagnostics()
                    void execution.save(setupRecovery)
                }}
            >
                核对提交结果
            </LoadingButton>
        )
    } else if (resolving) {
        primary = null
    } else if (confirming) {
        primary = (
            <LoadingButton
                id={`${idPrefix}-confirm`}
                size="sm"
                loading={commands.pending}
                disabled={locked}
                onClick={() => void confirm()}
            >
                {confirmationLabel}
            </LoadingButton>
        )
    } else if (completeAllowed) {
        primary = (
            <Button
                id={`${idPrefix}-complete`}
                size="sm"
                disabled={locked}
                onClick={() => task.setCompleteOpen(true)}
            >
                完成任务
            </Button>
        )
    } else if (showQuery) {
        primary = (
            <GuardedBusinessAction
                id={`${idPrefix}-query`}
                size="sm"
                loading={queryMutation.isPending}
                disabled={!queryAllowed || locked}
                reason={
                    !ownsTask ? "当前任务由责任人处理" : blocker("QUERY_RESULT")
                }
                onClick={() => {
                    if (queryAllowed && !locked)
                        void actions.handleQueryResult()
                }}
            >
                查询原下单结果
            </GuardedBusinessAction>
        )
    }

    const showEscalation =
        detail.allowedActions.includes("ESCALATE_W29") &&
        hasPermission(permissions, "integration_error_task:list")
    const footerActions = (
        <>
            {confirming ? (
                <Button
                    id={`${idPrefix}-cancel-confirm`}
                    variant="ghost"
                    size="sm"
                    disabled={commands.pending}
                    onClick={closeConfirmation}
                >
                    取消
                </Button>
            ) : !resolving ? (
                <>
                    {showEscalation ? (
                        <Button
                            id={`${idPrefix}-escalate`}
                            variant="ghost"
                            size="sm"
                            render={
                                <Link href="/governance/integration-errors" />
                            }
                        >
                            转接口错误中心{" "}
                            <ExternalLinkIcon aria-hidden="true" />
                        </Button>
                    ) : null}
                    {completeAllowed && queryAllowed ? (
                        <Button
                            id={`${idPrefix}-query-secondary`}
                            variant="ghost"
                            size="sm"
                            disabled={locked}
                            onClick={() => void actions.handleQueryResult()}
                        >
                            查询原下单结果
                        </Button>
                    ) : null}
                    {replayAllowed ? (
                        <Button
                            id={`${idPrefix}-replay`}
                            variant="outline"
                            size="sm"
                            disabled={locked}
                            onClick={() => actions.setReplayOpen(true)}
                        >
                            安全重发
                        </Button>
                    ) : null}
                </>
            ) : null}
            {!confirming &&
            (commands.conflict || !primary || resolving || refreshFailed) ? (
                <LoadingButton
                    id={`${idPrefix}-refresh`}
                    size="sm"
                    variant="ghost"
                    loading={refreshing}
                    disabled={refreshing || commands.pending}
                    onClick={() => {
                        void refresh().then((refreshed) => {
                            if (refreshed.isSuccess) commands.clearConflict()
                        })
                    }}
                >
                    刷新最新数据
                </LoadingButton>
            ) : null}
            {primary}
        </>
    )

    const resultTitle =
        result?.title === "未重发" && replayMutation.data?.status === "unknown"
            ? "重发结果待核实"
            : result?.title

    const body = (
        <>
            <SupplierInvestigationCanvas
                idPrefix={idPrefix}
                detail={detail}
                tree={execution.query.data?.order_tree}
                investigation={investigation}
                lastSync={lastSync}
                remoteStatus={remoteStatus}
                onOpenDiagnostics={onOpenDiagnostics}
                totalQuantity={derivation.totalQuantity}
                totalCostGross={derivation.totalCostGross}
                isResultUnknown={derivation.isResultUnknown}
                queryBlocker={blocker("QUERY_RESULT")}
                notice={
                    commands.recovery ||
                    commands.conflict ||
                    setupRecovery ||
                    refreshFailed ? (
                        <div
                            role="status"
                            className="border-l-2 border-l-warning bg-warning-soft/60 px-3 py-2 text-xs text-warning-soft-foreground"
                        >
                            {commands.recovery ? (
                                <>
                                    <p className="font-semibold">
                                        {commands.recovery.label}结果待核对
                                    </p>
                                    <p>{commands.recovery.message}</p>
                                </>
                            ) : setupRecovery ? (
                                <p>
                                    配送资料保存结果待核对。可使用底栏操作核对原提交。
                                </p>
                            ) : commands.conflict ? (
                                <p>{commands.conflict} 请刷新最新数据。</p>
                            ) : (
                                <p>
                                    最新订单读取失败。当前显示上次读取的数据，请刷新后继续处理。
                                </p>
                            )}
                        </div>
                    ) : null
                }
            >
                {confirming ? (
                    <section
                        id={confirmationId}
                        tabIndex={-1}
                        aria-label={confirmationLabel}
                        aria-live="polite"
                        className="space-y-1 border-l-2 border-l-warning bg-warning-soft/60 px-3 py-2 text-xs text-warning-soft-foreground"
                    >
                        <h2 className="text-body-compact font-semibold">
                            {confirmationLabel}
                        </h2>
                        <p>
                            {actions.replayOpen ? (
                                "将按原下单任务再次向供应商提交，当前任务保持待处理。"
                            ) : task.completeOpen ? (
                                investigation?.summary ||
                                "将保存已核实的业务结果并完成当前任务。"
                            ) : (
                                <>
                                    将向供应商提交售后申请{" "}
                                    <span className="num">
                                        {afterSales?.requestNo}
                                    </span>
                                    。
                                </>
                            )}
                        </p>
                    </section>
                ) : null}

                {result && !resolving ? (
                    <section
                        aria-live="polite"
                        className="flex flex-wrap items-start justify-between gap-2 border-y border-border/60 py-2 text-xs"
                    >
                        <div className="min-w-0 flex-[1_1_12rem]">
                            <h2 className="text-body-compact font-semibold">
                                {resultTitle}
                            </h2>
                            <p className="text-muted-foreground">
                                {result.description}
                            </p>
                            {result.facts?.length ? (
                                <dl className="mt-1 flex flex-wrap gap-x-5 gap-y-1">
                                    {result.facts
                                        .filter(
                                            (fact) =>
                                                fact.value !==
                                                result.description,
                                        )
                                        .map((fact) => (
                                            <div
                                                key={fact.label}
                                                className="flex flex-wrap gap-x-1.5"
                                            >
                                                <dt className="text-muted-foreground">
                                                    {fact.label}
                                                </dt>
                                                <dd>{fact.value}</dd>
                                            </div>
                                        ))}
                                </dl>
                            ) : null}
                        </div>
                        <Button
                            id={`${idPrefix}-dismiss-result`}
                            variant="ghost"
                            size="xs"
                            onClick={() => setResult(null)}
                        >
                            关闭结果
                        </Button>
                    </section>
                ) : null}
            </SupplierInvestigationCanvas>
            <WorkspaceTaskFooter
                summary={summary}
                fallback={
                    <>
                        <div className="mr-auto">{summary}</div>
                        {footerActions}
                    </>
                }
            >
                {footerActions}
            </WorkspaceTaskFooter>
        </>
    )

    return host.render({
        order,
        body,
        diagnostics: (
            <>
                <SupplierInvestigationDiagnostics
                    detail={detail}
                    evidence={investigation}
                    remoteStatus={remoteStatus}
                    lastSync={lastSync}
                />
                <div className="border-b border-border/60 py-3.5">
                    <SupplierOrderExecutionSetupPanel
                        execution={execution}
                        locked={
                            commands.pending ||
                            Boolean(commands.recovery) ||
                            confirming ||
                            refreshFailed
                        }
                        inline
                        idPrefix={`${idPrefix}-execution`}
                        orderId={order.id}
                        addressSummary={
                            detail.address.revealed ?? detail.address.masked
                        }
                        executionNotice={order.executionNotice}
                        canEdit={hasPermission(permissions, submitPermission)}
                    />
                </div>
                {canInvestigate ||
                hasPermission(permissions, submitPermission) ? (
                    <SupplierInvestigationNote
                        idPrefix={idPrefix}
                        detail={detail}
                        noteMutation={commands.wrap(noteMutation, {
                            kind: "note",
                            label: "提交说明",
                            permissions: [
                                investigatePermission,
                                submitPermission,
                            ],
                            onRecovered: (response) =>
                                recoverResult("协同说明结果核对", response),
                        })}
                        setResult={setResult}
                        locked={locked}
                    />
                ) : null}
            </>
        ),
        fulfillment: (
            <>
                <SupplierOrderExecutionProgress orderId={order.id} compact />
                <FulfillmentSection
                    idPrefix={`${idPrefix}-fulfillment`}
                    investigation
                    logistics={detail.logistics}
                    address={detail.address}
                    statusHistory={detail.statusHistory}
                    canReveal={derivation.canReveal && detail.address.canReveal}
                    revealPending={revealMutation.isPending}
                    onReveal={() => void orderActions.handleReveal()}
                    onHide={() =>
                        void clearAddressReveal(order.id).then(refresh)
                    }
                />
                {detail.afterSales.length ? (
                    <AftersalesSection
                        idPrefix={`${idPrefix}-aftersales`}
                        afterSales={detail.afterSales}
                        pending={locked || confirming}
                        onRequest={orderActions.setAfterSalesConfirm}
                    />
                ) : null}
            </>
        ),
    })
}
