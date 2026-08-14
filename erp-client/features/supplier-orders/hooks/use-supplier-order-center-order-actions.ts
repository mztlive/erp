"use client"

import * as React from "react"
import type { UseMutationResult } from "@tanstack/react-query"

import { getErrorMessage } from "@/lib/api/errors"
import type {
    AfterSalesActionInput,
    AfterSalesActionResult,
    FormalActionResponse,
    RevealAddressInput,
    RevealAddressResult,
    SupplierOrderDetailView,
} from "@/features/supplier-orders/types"
import {
    CANCEL_STATUS_LABEL,
    REFUND_STATUS_LABEL,
} from "@/features/supplier-orders/types"
import type {
    AfterSalesConfirmRequest,
    SupplierOrderCenterResult,
} from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"

type SupplierOrderCenterOrderActionsInput = {
    orderId: string
    detail: SupplierOrderDetailView | undefined
    setResult: React.Dispatch<
        React.SetStateAction<SupplierOrderCenterResult | null>
    >
    afterSalesMutation: UseMutationResult<
        FormalActionResponse<AfterSalesActionResult>,
        Error,
        AfterSalesActionInput
    >
    revealMutation: UseMutationResult<
        FormalActionResponse<RevealAddressResult>,
        Error,
        RevealAddressInput
    >
}

/** 售后提交与收货信息短时揭示等订单级命令。 */
export function useSupplierOrderCenterOrderActions(
    input: SupplierOrderCenterOrderActionsInput,
) {
    const { orderId, detail, setResult, afterSalesMutation, revealMutation } =
        input

    const [afterSalesConfirm, setAfterSalesConfirm] =
        React.useState<AfterSalesConfirmRequest | null>(null)

    async function handleAfterSales(
        action: "CANCEL" | "REFUND",
        requestId: string,
    ) {
        if (!detail) return
        try {
            const res = await afterSalesMutation.mutateAsync({
                orderId,
                expectedLockVersion: detail.order.lockVersion,
                action,
                operationId: `op-as-${action}-${Date.now()}`,
                idempotencyKey: `as-${action}-${requestId}`,
                afterSalesRequestId: requestId,
            })
            setAfterSalesConfirm(null)
            setResult({
                status:
                    res.status === "succeeded"
                        ? "succeeded"
                        : res.status === "blocked"
                          ? "blocked"
                          : "rejected",
                title:
                    res.status === "succeeded"
                        ? action === "CANCEL"
                            ? "取消已提交"
                            : "退款已提交"
                        : "售后动作未提交",
                description: res.message,
                reference: res.reference,
                facts: res.data
                    ? [
                          {
                              label: "取消轨",
                              value: CANCEL_STATUS_LABEL[res.data.cancelStatus],
                          },
                          {
                              label: "退款轨",
                              value: REFUND_STATUS_LABEL[res.data.refundStatus],
                          },
                          { label: "说明", value: res.data.note },
                      ]
                    : undefined,
            })
        } catch (error) {
            setResult({
                status: "rejected",
                title: "售后动作未提交",
                description: getErrorMessage(error, "提交失败，请稍后重试"),
            })
        }
    }

    async function handleReveal() {
        if (!detail) return
        try {
            const res = await revealMutation.mutateAsync({
                orderId,
                reason: "履约处理需要核对收货信息",
            })
            setResult({
                status: res.status === "succeeded" ? "succeeded" : "blocked",
                title:
                    res.status === "succeeded" ? "已短时揭示地址" : "无法揭示",
                description: res.message,
            })
        } catch (error) {
            setResult({
                status: "rejected",
                title: "地址揭示失败",
                description: getErrorMessage(error, "操作失败，请稍后重试"),
            })
        }
    }

    return {
        afterSalesConfirm,
        setAfterSalesConfirm,
        handleAfterSales,
        handleReveal,
    }
}
