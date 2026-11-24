"use client"

import * as React from "react"
import { flushSync } from "react-dom"
import { EllipsisIcon } from "lucide-react"
import {
    WorkspaceTaskFooter,
    type ValidationIssue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { LoadingButton } from "@/components/ui/loading-button"
import { cn } from "@/lib/utils"
import {
    FulfillmentDraftForm,
    FulfillmentDraftReadOnly,
} from "../../components/forms/fulfillment-draft-form"
import { OPERATION_ACTION_LABEL } from "../../types"
import { useFulfillmentFieldLocation } from "../hooks/use-fulfillment-field-location"
import {
    FulfillmentBatchTable,
    type FulfillmentBatchTableHandle,
} from "./fulfillment-batch-table"
import { FulfillmentTaskFields } from "./fulfillment-task-fields"
import {
    FulfillmentBatchSummary,
    FulfillmentRiskStrip,
    FulfillmentSourceContext,
    FulfillmentTaskKpis,
    type FulfillmentTaskContext,
} from "./fulfillment-task-overview"
import { fulfillmentBatchQuantities } from "./fulfillment-task-quantities"
import type { FulfillmentWorkSurfaceProps } from "./fulfillment-work-surface"

export function FulfillmentTaskWorkSurface({
    operation,
    draft,
    taskContext: task,
    actionError,
    validationIssues,
    saveMessage,
    canExecute,
    canPost,
    formalPending,
    savePending,
    supportsSave,
    dirty,
    readOnlyNote,
    currentUrl,
    headingRef,
    resultUnknown,
    recovery,
    shortcutsOpen,
    onDraftChange,
    onDiscard,
    onSave,
    onConfirm,
}: FulfillmentWorkSurfaceProps & { taskContext: FulfillmentTaskContext }) {
    const table = React.useRef<FulfillmentBatchTableHandle>(null)
    const { deliveryRef, locateIssue } = useFulfillmentFieldLocation()
    const [logisticsOpen, setLogisticsOpen] = React.useState(false)
    const highlight = React.useRef<{ timer: number; element: HTMLElement }>(
        null,
    )
    const isDelivery =
        draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT"
    const addressPacking =
        isDelivery && Boolean(draft.packing && draft.addressSnapshot)
    const quantities = fulfillmentBatchQuantities(
        operation,
        canExecute ? draft : operation.draft,
    )
    const fieldIssues = validationIssues.filter(({ id }) => id !== "gate")
    const firstIssue =
        fieldIssues.find((issue) => issue.targetId) ?? fieldIssues[0]
    const editingDisabled = formalPending || resultUnknown
    const detailsId = `${task.id}-logistics`
    const tableId = `${task.id}-lines`
    const destinations = isDelivery
        ? (draft.addressSnapshot?.destinations ?? [])
        : []
    const pendingAddresses = destinations.filter(
        (destination) => destination.pending_address,
    ).length

    React.useEffect(
        () => () => {
            if (highlight.current) window.clearTimeout(highlight.current.timer)
        },
        [],
    )

    function focusIssue(issue?: ValidationIssue) {
        let targetId: string | undefined
        flushSync(() => {
            targetId = table.current?.reveal(issue?.targetId)
            if (!targetId && isDelivery && issue?.targetId)
                setLogisticsOpen(true)
        })

        const showTarget = () => {
            const fallback = document.getElementById(`${task.id}-detail`)
            const requestedId = targetId ?? issue?.targetId
            const node =
                (requestedId && document.getElementById(requestedId)) ||
                fallback
            if (!node) return

            node.scrollIntoView({ block: "center", behavior: "smooth" })
            const element = node.closest("tr") ?? node
            const control = node.querySelector<HTMLElement>(
                "input:not(:disabled), button:not(:disabled), textarea:not(:disabled)",
            )
            const focusTarget =
                control ?? (node.matches(":disabled") ? element : node)
            if (targetId || !node.contains(document.activeElement))
                focusTarget.focus({ preventScroll: true })

            const classes = [
                "bg-info-soft/50",
                "ring-1",
                "ring-inset",
                "ring-info-border",
                "motion-safe:animate-pulse",
            ]
            if (highlight.current) {
                window.clearTimeout(highlight.current.timer)
                highlight.current.element.classList.remove(...classes)
            }
            element.classList.add(...classes)
            highlight.current = {
                element,
                timer: window.setTimeout(() => {
                    element.classList.remove(...classes)
                    highlight.current = null
                }, 2200),
            }
        }

        if (!targetId && issue?.targetId && isDelivery) {
            // 定位包裹字段前，先由物流表单展开对应地址或包裹。
            locateIssue(issue)
            requestAnimationFrame(() =>
                requestAnimationFrame(() => requestAnimationFrame(showTarget)),
            )
        } else {
            showTarget()
        }
    }

    const actions =
        resultUnknown && recovery ? (
            <>
                {recovery.onRetry ? (
                    <LoadingButton
                        id={`${task.id}-retry`}
                        type="button"
                        size="sm"
                        variant="outline"
                        loading={recovery.retrying}
                        disabled={recovery.resolving}
                        onClick={recovery.onRetry}
                    >
                        使用本次操作重试
                    </LoadingButton>
                ) : null}
                <LoadingButton
                    id={`${task.id}-resolve`}
                    type="button"
                    size="sm"
                    variant="default"
                    loading={recovery.resolving}
                    disabled={recovery.retrying}
                    onClick={recovery.onResolve}
                >
                    核对提交结果
                </LoadingButton>
            </>
        ) : canExecute ? (
            <div className="flex min-w-0 flex-wrap items-center justify-end gap-1.5">
                {firstIssue ? (
                    <Button
                        id={`${task.id}-locate`}
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={editingDisabled}
                        onClick={() => focusIssue(firstIssue)}
                    >
                        {fieldIssues.length} 项待完善 · 定位
                    </Button>
                ) : null}
                {dirty ? (
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id={`${task.id}-more`}
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    aria-label="更多操作"
                                    disabled={editingDisabled || savePending}
                                />
                            }
                        >
                            <EllipsisIcon aria-hidden="true" />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                            <DropdownMenuItem
                                id={`${task.id}-discard`}
                                onClick={onDiscard}
                            >
                                放弃修改
                            </DropdownMenuItem>
                        </DropdownMenuContent>
                    </DropdownMenu>
                ) : null}
                {supportsSave && draft.type !== "RECEIPT" ? (
                    <LoadingButton
                        id={`${task.id}-save`}
                        type="button"
                        variant="outline"
                        size="sm"
                        loading={savePending}
                        disabled={editingDisabled || !dirty}
                        onClick={onSave}
                    >
                        保存草稿
                    </LoadingButton>
                ) : null}
                <LoadingButton
                    id={`${task.id}-confirm`}
                    type="button"
                    variant="default"
                    size="sm"
                    loading={formalPending}
                    disabled={editingDisabled || savePending || !canPost}
                    aria-describedby={
                        firstIssue && !canPost
                            ? `${task.id}-first-issue`
                            : undefined
                    }
                    onClick={onConfirm}
                >
                    {OPERATION_ACTION_LABEL[operation.operationType]}
                </LoadingButton>
            </div>
        ) : (
            <p className="text-xs text-muted-foreground">{readOnlyNote}</p>
        )
    const summary = (
        <FulfillmentBatchSummary
            quantities={quantities}
            operation={operation}
        />
    )

    return (
        <div className="min-w-0 space-y-3 bg-card py-3">
            <FulfillmentRiskStrip
                task={task}
                operation={operation}
                actionError={actionError}
                pendingAddresses={pendingAddresses}
                onLocate={() => focusIssue()}
            />
            <FulfillmentTaskKpis
                task={task}
                operation={operation}
                pending={quantities.pending}
                progress={quantities.progress}
            />
            <FulfillmentSourceContext
                operation={operation}
                currentUrl={currentUrl}
                customerInTitle
                task={task}
            />
            <h3
                id={`${task.id}-detail-heading`}
                tabIndex={-1}
                ref={headingRef}
                className="sr-only"
            >
                {OPERATION_ACTION_LABEL[operation.operationType]}
            </h3>
            <div
                id={`${task.id}-detail`}
                tabIndex={-1}
                className="space-y-3 outline-none"
            >
                {canExecute ? (
                    <>
                        <FulfillmentBatchTable
                            id={tableId}
                            ref={table}
                            operation={operation}
                            draft={draft}
                            quantities={quantities}
                            disabled={editingDisabled}
                            issues={fieldIssues}
                            onChange={onDraftChange}
                        />
                        {isDelivery ? (
                            <div
                                className={cn(
                                    "min-w-0",
                                    addressPacking &&
                                        "space-y-2 border-t border-border/60 pt-2",
                                )}
                            >
                                {addressPacking ? (
                                    <Button
                                        id={`${detailsId}-toggle`}
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        aria-expanded={logisticsOpen}
                                        aria-controls={detailsId}
                                        onClick={() =>
                                            setLogisticsOpen(!logisticsOpen)
                                        }
                                    >
                                        {logisticsOpen
                                            ? "收起收货地址与物流"
                                            : `填写收货地址与物流（${destinations.length} 个地址）`}
                                    </Button>
                                ) : null}
                                <div
                                    id={detailsId}
                                    hidden={addressPacking && !logisticsOpen}
                                    className="min-w-0"
                                >
                                    <FulfillmentDraftForm
                                        deliveryRef={deliveryRef}
                                        operation={operation}
                                        draft={draft}
                                        onChange={onDraftChange}
                                        disabled={editingDisabled}
                                    />
                                </div>
                            </div>
                        ) : (
                            <FulfillmentTaskFields
                                operation={operation}
                                draft={draft}
                                onChange={onDraftChange}
                                disabled={editingDisabled}
                            />
                        )}
                        {firstIssue && !resultUnknown ? (
                            <p
                                id={`${task.id}-first-issue`}
                                className="wrap-anywhere text-xs text-muted-foreground"
                            >
                                <span className="font-medium">
                                    {firstIssue.label}
                                </span>{" "}
                                · {firstIssue.message}
                            </p>
                        ) : null}
                    </>
                ) : (
                    <div className="[&>section>ul]:rounded-none [&>section>ul]:border-0 [&>section>ul>li]:px-0 [&>section>section]:rounded-none [&>section>section]:border-0 [&>section>section]:border-t [&>section>section]:border-border/60 [&>section>section]:px-0">
                        <FulfillmentDraftReadOnly
                            operation={operation}
                            draft={operation.draft}
                        />
                    </div>
                )}
            </div>
            {saveMessage ? (
                <p role="status" className="text-xs text-muted-foreground">
                    {saveMessage}
                </p>
            ) : null}
            {shortcutsOpen && canExecute ? (
                <p className="text-xs text-muted-foreground">
                    Ctrl/⌘ + Enter 确认
                    {supportsSave && draft.type !== "RECEIPT"
                        ? " · Ctrl/⌘ + S 保存草稿"
                        : ""}
                </p>
            ) : null}
            <WorkspaceTaskFooter
                summary={summary}
                fallback={
                    <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
                        {summary}
                        {actions}
                    </div>
                }
            >
                {actions}
            </WorkspaceTaskFooter>
        </div>
    )
}
