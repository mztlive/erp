"use client"

import * as React from "react"

import type { ResultState as SharedResultState } from "@/components/business/feedback"
import { useSupplierOptionsQuery } from "@/hooks/use-options"
import {
    useCompleteProcurementMutation,
    useProcurementConfirmationQuery,
    useProcurementRecommendationQuery,
    useProcurementSupplyOptionsQuery,
    useSaveProcurementConfirmationMutation,
} from "@/features/procurement-confirmation/hooks/queries"
import type { FormalOutcome } from "@/features/procurement-confirmation/types"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import { useProcurementConfirmationActions } from "./use-procurement-confirmation-actions"
import { useProcurementConfirmationDrafts } from "./use-procurement-confirmation-drafts"
import { useProcurementKeyboardShortcuts } from "./use-procurement-keyboard-shortcuts"
import { useProcurementResponsibilityActions } from "./use-procurement-confirmation-responsibility-actions"
import {
    useProcurementConfirmationQueueUrlSync,
    useProcurementConfirmationUrl,
} from "./use-procurement-confirmation-url"

type ResultState = SharedResultState<FormalOutcome>

/**
 * 采购二次确认页面控制器：URL 状态、队列/方案查询、
 * 分行草稿与全部任务动作都收敛在这里，页面只负责布局。
 * 合同与客户只展示销售提交快照，不预拉合同中心或客户主数据。
 */
export function useProcurementConfirmationController() {
    const url = useProcurementConfirmationUrl()

    const filters = React.useMemo(
        () => ({
            scope: url.scope,
            due: url.due,
            sort: url.sort,
            orderNo: url.orderNo,
            currentWorkItemId: url.currentWorkItemId,
            queueContextId: url.queueContextId,
        }),
        [
            url.scope,
            url.due,
            url.sort,
            url.orderNo,
            url.currentWorkItemId,
            url.queueContextId,
        ],
    )

    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const [contractOpen, setContractOpen] = React.useState(false)
    const [advanceAfterConfirm, setAdvanceAfterConfirm] = React.useState(true)
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    /** 自动下一项跳转后保留的上一条结果（轻量条，可关闭） */
    const [finishedResult, setFinishedResult] =
        React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [saveMessage, setSaveMessage] = React.useState<string | null>(null)
    const headingRef = React.useRef<HTMLHeadingElement>(null)
    const resultRef = React.useRef<HTMLDivElement>(null)

    const queueQuery = useProcurementConfirmationQuery(filters)
    const responsibilityMutation = useWorkItemResponsibilityMutation()
    const saveMutation = useSaveProcurementConfirmationMutation()
    const completeMutation = useCompleteProcurementMutation()
    const { data: supplierOptions } = useSupplierOptionsQuery()

    const view = queueQuery.data
    const tasks = React.useMemo(() => view?.tasks ?? [], [view?.tasks])
    const context = view?.context
    const task =
        tasks.find((t) => t.workItemId === url.currentWorkItemId) ??
        view?.current ??
        tasks[0]
    const recommendationQuery = useProcurementRecommendationQuery(
        task?.confirmation.confirmationId ?? "",
        confirmOpen,
    )
    const recommendation = recommendationQuery.data
    const taskSkuIds = React.useMemo(
        () => task?.salesSubmission.lines.map((line) => line.itemSku) ?? [],
        [task],
    )
    const supplyOptionsQuery = useProcurementSupplyOptionsQuery(taskSkuIds)
    const supplyOptions = React.useMemo(
        () => supplyOptionsQuery.data ?? [],
        [supplyOptionsQuery.data],
    )
    const currentIndex = task
        ? Math.max(
              0,
              tasks.findIndex((t) => t.workItemId === task.workItemId),
          )
        : 0
    const completed = Boolean(view) && tasks.length === 0

    useProcurementConfirmationQueueUrlSync({
        scope: url.scope,
        queueContextId: view?.context.queueContextId ?? url.queueContextId,
        queueReady: !queueQuery.isPending && Boolean(view),
        tasksLength: tasks.length,
        currentTaskWorkItemId: task?.workItemId,
    })

    const drafts = useProcurementConfirmationDrafts({
        task,
        confirmOpen,
        recommendation,
        supplyOptions,
        supplierOptions,
        setSaveMessage,
        setActionError,
    })

    React.useEffect(() => {
        if (lastResult) {
            resultRef.current?.focus()
        } else if (task) {
            headingRef.current?.focus()
        }
    }, [task, lastResult])

    const replaceUrl = url.replaceUrl

    const goToWorkItem = React.useCallback(
        (workItemId: string | undefined | null) => {
            setLastResult(null)
            setActionError(null)
            if (!workItemId) {
                replaceUrl({ currentWorkItemId: null })
                return
            }
            replaceUrl({ currentWorkItemId: workItemId })
        },
        [replaceUrl],
    )

    const neighborId = React.useCallback(
        (delta: number) => {
            const idx = currentIndex + delta
            return tasks[idx]?.workItemId
        },
        [currentIndex, tasks],
    )

    const actions = useProcurementConfirmationActions({
        task,
        tasks,
        lineDrafts: drafts.lineDrafts,
        dirty: drafts.dirty,
        linesValid: drafts.linesValid,
        allCovered: drafts.allCovered,
        autoNext: url.autoNext,
        advanceAfterConfirm,
        recommendation,
        saveMutation,
        completeMutation,
        queueRefetch: queueQuery.refetch,
        replaceUrl,
        neighborId,
        goToWorkItem,
        setDirty: drafts.setDirty,
        setActionError,
        setSaveMessage,
        setConfirmOpen,
        setRejectOpen,
        setLastResult,
        setFinishedResult,
        setAdvanceAfterConfirm,
    })

    const responsibilityActions = useProcurementResponsibilityActions({
        task,
        dirty: drafts.dirty,
        handleSave: actions.handleSave,
        responsibilityMutation,
        queueRefetch: queueQuery.refetch,
        replaceUrl,
        neighborId,
        goToWorkItem,
        assertAllowed: actions.assertAllowed,
        setActionError,
        setLastResult,
    })

    const taskActions = {
        ...actions,
        ...responsibilityActions,
    }

    const handleNavigate = React.useCallback(
        (delta: 1 | -1) => {
            if (drafts.dirty) {
                setActionError("有未保存修改，请先保存后再切换")
                return
            }
            const next = neighborId(delta)
            if (next) goToWorkItem(next)
        },
        [drafts.dirty, goToWorkItem, neighborId],
    )

    useProcurementKeyboardShortcuts({
        allowedActions: task?.allowedActions,
        searchInputRef: url.orderNoInputRef,
        onSave: () => void actions.handleSave(),
        onConfirmApprove: () => {
            setAdvanceAfterConfirm(url.autoNext)
            setConfirmOpen(true)
        },
        onNavigate: handleNavigate,
    })

    const formalPending =
        completeMutation.isPending ||
        saveMutation.isPending ||
        responsibilityMutation.isPending ||
        lastResult?.status === "unknown"

    return {
        url,
        queueQuery,
        view,
        tasks,
        context,
        task,
        currentIndex,
        completed,
        recommendationQuery,
        recommendation,
        contractOpen,
        setContractOpen,
        supplyOptions,
        supplierOptions,
        confirmOpen,
        setConfirmOpen,
        rejectOpen,
        setRejectOpen,
        advanceAfterConfirm,
        lastResult,
        setLastResult,
        finishedResult,
        setFinishedResult,
        actionError,
        saveMessage,
        formalPending,
        headingRef,
        resultRef,
        estimatedPurchase: recommendation?.estimatedPurchaseGross,
        saveMutation,
        completeMutation,
        goToWorkItem,
        neighborId,
        drafts,
        actions: taskActions,
    }
}
