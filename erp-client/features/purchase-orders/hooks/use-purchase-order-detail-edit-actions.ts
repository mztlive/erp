"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { responsibilityText } from "@/lib/ui-text"
import {
    PAYMENT_TERM_OPTIONS,
    type PurchaseOrderCenterView,
    type SubmitPurchaseOrderPayload,
} from "@/features/purchase-orders/types"
import {
    positiveDecimal,
    taxRateValid,
} from "@/features/purchase-orders/lib/purchase-order-validation"
import {
    useAcquireDraftTokenMutation,
    useSavePurchaseOrderDraftMutation,
    useStartPurchaseChangeMutation,
    useSubmitPurchaseOrderMutation,
} from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderDetailMode } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"

export type PurchaseOrderDetailLineEdits = Record<
    string,
    { quantity?: string; unitCostGross?: string; inputTaxRate: string }
>

type UsePurchaseOrderDetailEditActionsInput = {
    purchaseOrderId: string
    mode: PurchaseOrderDetailMode
    order: PurchaseOrderCenterView | null | undefined
    refetch: () => Promise<{ data?: PurchaseOrderCenterView | null }>
    commandLedger: FormalCommandKeyLedger
    setResult: React.Dispatch<
        React.SetStateAction<PurchaseOrderDetailResult | null>
    >
    getPaymentTermCode: () => string
    setDraftPaymentTermCode: (value: string) => void
}

/**
 * 详情页草稿编辑动作：进入编辑取令牌、行级编辑缓存、保存草稿、
 * 提交审批与发起采购变更的正式命令编排。
 */
export function usePurchaseOrderDetailEditActions({
    purchaseOrderId,
    mode,
    order,
    refetch,
    commandLedger,
    setResult,
    getPaymentTermCode,
    setDraftPaymentTermCode,
}: UsePurchaseOrderDetailEditActionsInput) {
    const router = useRouter()
    const acquireToken = useAcquireDraftTokenMutation()
    const saveMutation = useSavePurchaseOrderDraftMutation()
    const submitMutation = useSubmitPurchaseOrderMutation()
    const changeMutation = useStartPurchaseChangeMutation()

    const [draftEditToken, setDraftEditToken] = React.useState<string | null>(
        null,
    )
    const [lineEdits, setLineEdits] =
        React.useState<PurchaseOrderDetailLineEdits>({})
    const [submitConfirmOpen, setSubmitConfirmOpen] = React.useState(false)
    const [changeConfirmOpen, setChangeConfirmOpen] = React.useState(false)

    const documentReference =
        order?.identity.purchaseNo ?? order?.identity.draftLabel

    React.useEffect(() => {
        if (!order || mode !== "edit") return
        if (draftEditToken) return
        if (!order.allowedActions.includes("EDIT")) return
        void acquireToken
            .mutateAsync(purchaseOrderId)
            .then((res) => {
                setDraftEditToken(res.draftEditToken)
            })
            .catch((error: Error) => {
                setResult({
                    status: "blocked",
                    title: responsibilityText.cannotEdit,
                    description: error.message,
                })
            })
        // init line edits
        const next: PurchaseOrderDetailLineEdits = {}
        for (const line of order.currentContent.lines) {
            next[line.lineId] = {
                quantity: line.quantity,
                unitCostGross: line.unitCostGross,
                inputTaxRate: line.inputTaxRate,
            }
        }
        setLineEdits(next)
        setDraftPaymentTermCode(order.header.paymentTermCode)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- only when entering edit
    }, [order?.identity.purchaseOrderId, mode])

    async function handleSave(): Promise<boolean> {
        if (!order || !draftEditToken) return false
        if (commandLedger.peek("submit")) {
            setResult({
                status: "unknown",
                title: "提交结果待确认",
                description: "请先使用原提交操作确认结果，确认前不能另存草稿。",
                reference: documentReference,
            })
            return false
        }
        // 行内即时校验：数量/单价为正数，税率为 0-1 小数
        const invalidLine = order.currentContent.lines.find((line) => {
            const edit = lineEdits[line.lineId]
            if (!edit) return false
            return (
                !positiveDecimal(edit.quantity ?? line.quantity) ||
                !positiveDecimal(edit.unitCostGross ?? line.unitCostGross) ||
                !taxRateValid(edit.inputTaxRate)
            )
        })
        if (invalidLine) {
            setResult({
                status: "rejected",
                title: "保存失败",
                description: `「${invalidLine.itemName}」数量与含税单价须为正数，税率须为 0 到 1 的十进制数（如 0.13 表示 13%）。`,
            })
            return false
        }
        const paymentTermCode = getPaymentTermCode()
        const paymentTermLabel =
            PAYMENT_TERM_OPTIONS.find(
                (option) => option.value === paymentTermCode,
            )?.label ?? order.header.paymentTermLabel

        const payload = {
            purchaseOrderId,
            expectedLockVersion: order.identity.lockVersion,
            draftEditToken,
            paymentTermCode,
            paymentTermLabel,
            lines: order.currentContent.lines.map((line) => ({
                lineId: line.lineId,
                lineType: line.lineType,
                quantity: lineEdits[line.lineId]?.quantity ?? line.quantity,
                unitCostGross:
                    lineEdits[line.lineId]?.unitCostGross ?? line.unitCostGross,
                inputTaxRate:
                    lineEdits[line.lineId]?.inputTaxRate ?? line.inputTaxRate,
                logisticsFeeReason: line.logisticsFeeReason,
            })),
        }
        const command = commandLedger.acquire(
            "save-draft",
            `purchase:${purchaseOrderId}:save`,
            payload,
        )
        const response = await saveMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        commandLedger.settle("save-draft", response.status)

        if (response.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "草稿已保存",
                description: `金额已按系统规范计算：含税 ${response.data.totals.gross} / 不含税 ${response.data.totals.net} / 税额 ${response.data.totals.tax}`,
                reference: response.reference,
                facts: [
                    {
                        label: "数据版本",
                        value: `v${response.data.lockVersion}`,
                    },
                ],
            })
            await refetch()
        } else if (response.status === "unknown") {
            setResult({
                status: "unknown",
                title: "保存结果未知",
                description: `${response.message} 输入已保留，未切换状态。`,
                reference: documentReference,
            })
        } else {
            setResult({
                status: "rejected",
                title: "保存失败",
                description: `${response.message} 输入已保留。`,
            })
        }
        return response.status === "succeeded"
    }

    async function handleSubmit() {
        if (!order || !draftEditToken) return
        let submitCommand =
            commandLedger.peek<SubmitPurchaseOrderPayload>("submit")
        if (!submitCommand) {
            const savePayload = {
                purchaseOrderId,
                expectedLockVersion: order.identity.lockVersion,
                draftEditToken,
                paymentTermCode: getPaymentTermCode(),
                paymentTermLabel: order.header.paymentTermLabel,
                lines: order.currentContent.lines.map((line) => ({
                    lineId: line.lineId,
                    lineType: line.lineType,
                    quantity: lineEdits[line.lineId]?.quantity ?? line.quantity,
                    unitCostGross:
                        lineEdits[line.lineId]?.unitCostGross ??
                        line.unitCostGross,
                    inputTaxRate:
                        lineEdits[line.lineId]?.inputTaxRate ??
                        line.inputTaxRate,
                })),
            }
            const saveCommand = commandLedger.acquire(
                "save-before-submit",
                `purchase:${purchaseOrderId}:save-before-submit`,
                savePayload,
            )
            const saveRes = await saveMutation.mutateAsync({
                ...saveCommand.payload,
                idempotencyKey: saveCommand.idempotencyKey,
            })
            commandLedger.settle("save-before-submit", saveRes.status)
            if (saveRes.status !== "succeeded") {
                setSubmitConfirmOpen(false)
                setResult({
                    status:
                        saveRes.status === "unknown" ? "unknown" : "rejected",
                    title: "提交前保存未成功",
                    description: saveRes.message,
                    reference:
                        saveRes.status === "unknown"
                            ? documentReference
                            : undefined,
                })
                return
            }

            const refreshed = await refetch()
            const lockVersion =
                refreshed.data?.identity.lockVersion ?? saveRes.data.lockVersion
            submitCommand = commandLedger.acquire(
                "submit",
                `purchase:${purchaseOrderId}:submit`,
                {
                    purchaseOrderId,
                    expectedLockVersion: lockVersion,
                    expectedDraftContentHash: saveRes.data.draftContentHash,
                    draftEditToken,
                },
            )
        }
        if (!submitCommand) return
        const response = await submitMutation.mutateAsync({
            ...submitCommand.payload,
            idempotencyKey: submitCommand.idempotencyKey,
        })
        commandLedger.settle("submit", response.status)
        setSubmitConfirmOpen(false)
        if (response.status === "succeeded") {
            setDraftEditToken(null)
            setResult({
                status: "succeeded",
                title: "已提交审批",
                description: "已形成不可修改的采购提交并进入审批；编辑已结束。",
                reference: response.reference,
                facts: [
                    { label: "单据编号", value: response.data.purchaseNo },
                    {
                        label: "提交记录",
                        value: `第 ${response.data.submissionNo} 次提交`,
                    },
                    {
                        label: "数据版本",
                        value: `v${response.data.lockVersion}`,
                    },
                ],
            })
            router.replace(`/procurement/orders/${purchaseOrderId}`)
        } else if (response.status === "unknown") {
            setResult({
                status: "unknown",
                title: "提交结果未知",
                description: response.message,
                reference: documentReference,
            })
        } else {
            setResult({
                status: "rejected",
                title: "提交失败",
                description: response.message,
            })
        }
    }

    async function handleStartChange() {
        if (!order) return
        const payload = {
            purchaseOrderId,
            expectedLockVersion: order.identity.lockVersion,
        }
        const command = commandLedger.acquire(
            "start-change",
            `purchase:${purchaseOrderId}:change`,
            payload,
        )
        const response = await changeMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        commandLedger.settle("start-change", response.status)
        setChangeConfirmOpen(false)
        if (response.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "已创建采购变更工作副本",
                description:
                    "生效字段锁定；不覆盖已发生付款、发票或履约记录。变更以基准版本创建目标提交。",
                reference: response.reference,
                facts: [
                    { label: "变更记录", value: "已创建" },
                    {
                        label: "基准版本",
                        value: `v${response.data.baseRevisionNo}`,
                    },
                ],
            })
            await refetch()
        } else if (response.status === "unknown") {
            setResult({
                status: "unknown",
                title: "变更结果待确认",
                description: response.message,
                reference: documentReference,
            })
        } else {
            setResult({
                status: "blocked",
                title: "无法发起变更",
                description: response.message,
            })
        }
    }

    return {
        draftEditToken,
        lineEdits,
        setLineEdits,
        submitConfirmOpen,
        setSubmitConfirmOpen,
        changeConfirmOpen,
        setChangeConfirmOpen,
        savePending: saveMutation.isPending,
        submitPending: submitMutation.isPending,
        changePending: changeMutation.isPending,
        handleSave,
        handleSubmit,
        handleStartChange,
    }
}
