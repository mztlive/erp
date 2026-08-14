"use client"

import * as React from "react"
import Link from "next/link"
import type { UseMutationResult } from "@tanstack/react-query"

import { getErrorMessage } from "@/lib/api/errors"
import type {
    FormalActionResponse,
    QueryResultData,
    QueryResultInput,
    ReplayInput,
    ReplayResultData,
    SupplierOrderDetailView,
} from "@/features/supplier-orders/types"
import {
    FULFILLMENT_STATUS_LABEL,
    WORK_ITEM_STATUS_LABEL,
} from "@/features/supplier-orders/types"
import {
    responsibilityOf,
} from "@/features/supplier-orders/hooks/use-supplier-order-center-derivation"
import type { CommandIdentity } from "@/features/supplier-orders/hooks/use-supplier-order-center-identity"

export type SupplierOrderCenterResult = {
    status: "succeeded" | "rejected" | "blocked" | "unknown"
    title: string
    description: string
    reference?: string
    facts?: { label: string; value: React.ReactNode }[]
}

export type AfterSalesConfirmRequest = {
    requestId: string
    requestNo: string
    mallRequestRef: string
    action: "CANCEL" | "REFUND"
}

export function useSupplierOrderCenterResult() {
    const [result, setResult] = React.useState<SupplierOrderCenterResult | null>(
        null,
    )
    const clearResult = React.useCallback(() => setResult(null), [])
    return { result, setResult, clearResult }
}

type SupplierOrderCenterActionsInput = {
    orderId: string
    workItemId?: string
    detail: SupplierOrderDetailView | undefined
    currentUserId?: string
    setResult: React.Dispatch<
        React.SetStateAction<SupplierOrderCenterResult | null>
    >
    queryResultMutation: UseMutationResult<
        FormalActionResponse<QueryResultData>,
        Error,
        QueryResultInput
    >
    replayMutation: UseMutationResult<
        FormalActionResponse<ReplayResultData>,
        Error,
        ReplayInput
    >
    commandIdentity: (kind: string, objectId: string) => CommandIdentity
    forgetCommandIdentity: (key: string) => void
}

/** 查询原结果与安全重发；共享结果面板、重发弹窗与最近一次调查证据。 */
export function useSupplierOrderCenterActions(
    input: SupplierOrderCenterActionsInput,
) {
    const {
        orderId,
        workItemId,
        detail,
        currentUserId,
        setResult,
        queryResultMutation,
        replayMutation,
        commandIdentity,
        forgetCommandIdentity,
    } = input

    const [replayOpen, setReplayOpen] = React.useState(false)
    const [latestInvestigation, setLatestInvestigation] = React.useState<
        NonNullable<SupplierOrderDetailView["lastInvestigation"]> | undefined
    >()

    async function handleQueryResult() {
        if (!detail) return
        if (workItemId && !detail.workItem) {
            setResult({
                status: "blocked",
                title: "正式任务不可处理",
                description:
                    detail.workItemBlocker?.message ??
                    "未查询到正式任务，禁止改走订单直接动作。",
            })
            return
        }
        if (
            detail.workItem &&
            responsibilityOf(detail.workItem, currentUserId) !==
                "assigned_to_me"
        ) {
            setResult({
                status: "blocked",
                title: "当前没有处理权",
                description: "请先开始处理，或刷新查看当前处理人。",
            })
            return
        }
        if (!detail.allowedActions.includes("QUERY_RESULT")) {
            const blocker = detail.actionBlockers.find(
                (b) => b.action === "QUERY_RESULT",
            )
            setResult({
                status: "blocked",
                title: "无法查询原结果",
                description: blocker?.message ?? "当前不可查询",
                facts: blocker?.destinationWorkspaceId
                    ? [
                          {
                              label: "去向",
                              value: (
                                  <Link
                                      href="/governance/integration-errors"
                                      className="text-primary underline-offset-2 hover:underline"
                                  >
                                      接口错误与对账中心
                                  </Link>
                              ),
                          },
                      ]
                    : undefined,
            })
            return
        }
        try {
            const identity = commandIdentity("query", orderId)
            const res = await queryResultMutation.mutateAsync(
                detail.workItem
                    ? {
                          commandKind: "TASK",
                          workItemId: detail.workItem.workItemId,
                          expectedTaskVersion: detail.workItem.taskVersion,
                          expectedSubjectVersion:
                              detail.workItem.subjectVersion,
                          action: {
                              type: "QUERY_RESULT",
                              orderId,
                              expectedOrderLockVersion:
                                  detail.order.lockVersion,
                              targetSupplierActionId: detail.placeActionId,
                              operationId: identity.operationId,
                          },
                          idempotencyKey: identity.idempotencyKey,
                      }
                    : {
                          commandKind: "OBJECT",
                          orderId,
                          expectedLockVersion: detail.order.lockVersion,
                          action: "QUERY_RESULT",
                          targetSupplierActionId: detail.placeActionId,
                          operationId: identity.operationId,
                          idempotencyKey: identity.idempotencyKey,
                      },
            )
            if (res.status !== "unknown") {
                forgetCommandIdentity(identity.key)
            }
            if (res.data) setLatestInvestigation(res.data.evidence)
            setResult({
                status:
                    res.status === "succeeded"
                        ? "succeeded"
                        : res.status === "unknown"
                          ? "unknown"
                          : res.status === "blocked"
                            ? "blocked"
                            : "rejected",
                title:
                    res.status === "succeeded"
                        ? "查询原结果已完成"
                        : res.status === "unknown"
                          ? "查询结果仍未知"
                          : "查询未成功",
                description: res.message,
                reference: res.reference,
                facts: res.data
                    ? [
                          {
                              label: "证据结论",
                              value: res.data.evidence.outcomeLabel,
                          },
                          {
                              label: "可安全重试",
                              value: res.data.evidence.canSafeRetry
                                  ? "是"
                                  : "否",
                          },
                          {
                              label: "任务状态",
                              value: res.data.workItemStatus
                                  ? (WORK_ITEM_STATUS_LABEL[
                                        res.data.workItemStatus
                                    ] ?? res.data.workItemStatus)
                                  : "（非任务入口）",
                          },
                          {
                              label: "说明",
                              value: res.data.evidence.summary,
                          },
                      ]
                    : undefined,
            })
        } catch (error) {
            setResult({
                status: "rejected",
                title: "查询未完成",
                description: getErrorMessage(error, "查询失败，请稍后重试"),
            })
        }
    }

    async function handleReplay() {
        if (!detail) return
        if (workItemId && !detail.workItem) {
            setReplayOpen(false)
            setResult({
                status: "blocked",
                title: "正式任务不可处理",
                description:
                    detail.workItemBlocker?.message ??
                    "未查询到正式任务，禁止改走订单直接动作。",
            })
            return
        }
        if (
            detail.workItem &&
            responsibilityOf(detail.workItem, currentUserId) !==
                "assigned_to_me"
        ) {
            setReplayOpen(false)
            setResult({
                status: "blocked",
                title: "当前没有处理权",
                description: "请先开始处理，或刷新查看当前处理人。",
            })
            return
        }
        try {
            const identity = commandIdentity("replay", orderId)
            const res = await replayMutation.mutateAsync(
                detail.workItem
                    ? {
                          commandKind: "TASK",
                          workItemId: detail.workItem.workItemId,
                          expectedTaskVersion: detail.workItem.taskVersion,
                          expectedSubjectVersion:
                              detail.workItem.subjectVersion,
                          action: {
                              type: "REPLAY",
                              orderId,
                              expectedOrderLockVersion:
                                  detail.order.lockVersion,
                              targetSupplierActionId: detail.placeActionId,
                              operationId: identity.operationId,
                          },
                          idempotencyKey: identity.idempotencyKey,
                      }
                    : {
                          commandKind: "OBJECT",
                          orderId,
                          expectedLockVersion: detail.order.lockVersion,
                          action: "REPLAY",
                          targetSupplierActionId: detail.placeActionId,
                          operationId: identity.operationId,
                          idempotencyKey: identity.idempotencyKey,
                      },
            )
            if (res.status !== "unknown") {
                forgetCommandIdentity(identity.key)
            }
            if (res.data) setLatestInvestigation(res.data.evidence)
            setReplayOpen(false)
            setResult({
                status:
                    res.status === "succeeded"
                        ? "succeeded"
                        : res.status === "blocked"
                          ? "blocked"
                          : "rejected",
                title: res.status === "succeeded" ? "已安全重发" : "未重发",
                description: res.message,
                reference: res.reference,
                facts: res.data
                    ? [
                          {
                              label: "外部单号",
                              value: res.data.externalOrderNo ?? "—",
                          },
                          {
                              label: "履约状态",
                              value: FULFILLMENT_STATUS_LABEL[
                                  res.data.fulfillmentStatus
                              ],
                          },
                          {
                              label: "任务状态",
                              value: res.data.workItemStatus
                                  ? (WORK_ITEM_STATUS_LABEL[
                                        res.data.workItemStatus
                                    ] ?? res.data.workItemStatus)
                                  : "（非任务入口）",
                          },
                          {
                              label: "证据",
                              value: res.data.evidence.summary,
                          },
                      ]
                    : undefined,
            })
        } catch (error) {
            setResult({
                status: "rejected",
                title: "重发未完成",
                description: getErrorMessage(error, "重发失败，请稍后重试"),
            })
        }
    }

    return {
        replayOpen,
        setReplayOpen,
        latestInvestigation,
        handleQueryResult,
        handleReplay,
    }
}
