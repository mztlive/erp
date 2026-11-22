"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"

import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { getErrorMessage } from "@/lib/api/errors"
import { responsibilityText } from "@/lib/ui-text"
import {
    PAYMENT_TERM_OPTIONS,
    type PurchaseOrderCenterView,
    type SubmitPurchaseOrderPayload,
} from "@/features/purchase-orders/types"
import {
    positiveDecimal,
    purchaseQuantityValid,
    taxRateValid,
} from "@/features/purchase-orders/lib/purchase-order-validation"
import {
    useAcquireDraftTokenMutation,
    useSavePurchaseOrderDraftMutation,
    useStartPurchaseChangeMutation,
    useSubmitPurchaseOrderMutation,
    useVoidPurchaseOrderMutation,
} from "@/features/purchase-orders/hooks/queries"
import {
    purchaseOrderDetailHref,
    type PurchaseOrderDetailMode,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import {
    draftDeliveryProblem,
    draftLinePayload,
    type DraftLineEdits,
} from "@/features/purchase-orders/lib/draft-line-payload"

export type PurchaseOrderDetailLineEdits = DraftLineEdits

type UsePurchaseOrderDetailEditActionsInput = {
    purchaseOrderId: string
    mode: PurchaseOrderDetailMode
    /** 统一编辑态判定（含 permissions.canEdit）；未传时退回 mode === "edit"。 */
    isEditing?: boolean
    order: PurchaseOrderCenterView | null | undefined
    refetch: () => Promise<{ data?: PurchaseOrderCenterView | null }>
    commandLedger: FormalCommandKeyLedger
    setResult: React.Dispatch<
        React.SetStateAction<PurchaseOrderDetailResult | null>
    >
    getPaymentTermCode?: () => string
    setDraftPaymentTermCode?: (value: string) => void
    /** 嵌入工作台时由调用方收起编辑，不跳转到独立详情页。 */
    onFinished?: () => void
}

function validateDraftLineEdits(
    order: PurchaseOrderCenterView,
    lineEdits: PurchaseOrderDetailLineEdits,
): string | null {
    // 行内即时校验：数量/单价为正数，税率为 0-1 小数；依据待更新时商品行数量可填 0
    const allowZero = order.sourceUpdate?.affected === true
    const invalidLine = order.currentContent.lines.find((line) => {
        const edit = lineEdits[line.lineId]
        if (!edit) return false
        return (
            !purchaseQuantityValid(
                edit.quantity ?? line.quantity,
                allowZero && line.lineType === "ITEM_SERVICE",
            ) ||
            !positiveDecimal(edit.unitCostGross ?? line.unitCostGross) ||
            !taxRateValid(edit.inputTaxRate ?? line.inputTaxRate)
        )
    })
    if (invalidLine) {
        return allowZero
            ? `「${invalidLine.itemName}」数量须为 0 或正数（填 0 表示移除该行），含税单价须为正数，税率须在 0% 到 100% 之间。`
            : `「${invalidLine.itemName}」数量与含税单价须为正数，税率须在 0% 到 100% 之间。`
    }
    const deliveryProblem = draftDeliveryProblem(order, lineEdits)
    if (deliveryProblem) return deliveryProblem
    return null
}

function buildInitialLineEdits(
    order: PurchaseOrderCenterView,
): PurchaseOrderDetailLineEdits {
    const next: PurchaseOrderDetailLineEdits = {}
    for (const line of order.currentContent.lines) {
        next[line.lineId] = {
            quantity: line.quantity,
            unitCostGross: line.unitCostGross,
            inputTaxRate: line.inputTaxRate,
        }
    }
    return next
}

/**
 * 详情页草稿编辑动作：进入编辑取令牌、行级编辑缓存、保存草稿、
 * 提交审批与发起采购变更的正式命令编排。
 */
export function usePurchaseOrderDetailEditActions({
    purchaseOrderId,
    mode,
    isEditing: isEditingProp,
    order,
    refetch,
    commandLedger,
    setResult,
    getPaymentTermCode,
    setDraftPaymentTermCode,
    onFinished,
}: UsePurchaseOrderDetailEditActionsInput) {
    const router = useRouter()
    const searchParams = useSearchParams()
    const acquireToken = useAcquireDraftTokenMutation()
    const saveMutation = useSavePurchaseOrderDraftMutation()
    const submitMutation = useSubmitPurchaseOrderMutation()
    const voidMutation = useVoidPurchaseOrderMutation()
    const changeMutation = useStartPurchaseChangeMutation()

    const activeEditing = isEditingProp ?? mode === "edit"

    const [draftEditToken, setDraftEditToken] = React.useState<string | null>(
        null,
    )
    const [editLockError, setEditLockError] = React.useState<unknown>(null)
    const [lineEdits, setLineEdits] =
        React.useState<PurchaseOrderDetailLineEdits>({})
    const [submitConfirmOpen, setSubmitConfirmOpen] = React.useState(false)
    const [voidConfirmOpen, setVoidConfirmOpen] = React.useState(false)

    const documentReference =
        order?.identity.purchaseNo ?? order?.identity.draftLabel

    const requestEditToken = React.useCallback(
        (targetOrderId: string) => {
            setEditLockError(null)
            void acquireToken
                .mutateAsync(targetOrderId)
                .then((res) => {
                    setDraftEditToken(res.draftEditToken)
                    setEditLockError(null)
                })
                .catch((error: unknown) => {
                    setEditLockError(error)
                    setResult({
                        status: "blocked",
                        title: responsibilityText.cannotEdit,
                        description: getErrorMessage(
                            error,
                            "暂时无法编辑采购单，请刷新后重试。",
                        ),
                    })
                })
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps -- mutation handle is stable in practice
        [setResult],
    )

    React.useEffect(() => {
        if (!activeEditing) {
            setDraftEditToken(null)
            setEditLockError(null)
            return
        }
        if (!order) return
        if (!order.allowedActions.includes("EDIT")) return
        setLineEdits(buildInitialLineEdits(order))
        setDraftPaymentTermCode?.(order.header.paymentTermCode)
        if (!draftEditToken) {
            requestEditToken(purchaseOrderId)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- only re-initialize when entering edit or switching order
    }, [order?.identity.purchaseOrderId, activeEditing])

    const retryAcquireEditToken = React.useCallback(() => {
        if (!order || !activeEditing || acquireToken.isPending) return
        requestEditToken(purchaseOrderId)
    }, [
        acquireToken.isPending,
        activeEditing,
        order,
        purchaseOrderId,
        requestEditToken,
    ])

    const viewHref = purchaseOrderDetailHref(purchaseOrderId, searchParams, {
        mode: null,
    })

    async function handleSave(): Promise<boolean> {
        if (!order || !activeEditing || !draftEditToken) return false
        if (commandLedger.peek("submit")) {
            setResult({
                status: "unknown",
                title: "提交结果待确认",
                description: "请先使用原提交操作确认结果，确认前不能另存草稿。",
                reference: documentReference,
            })
            return false
        }
        const validationError = validateDraftLineEdits(order, lineEdits)
        if (validationError) {
            setResult({
                status: "rejected",
                title: "保存失败",
                description: validationError,
            })
            return false
        }
        const paymentTermCode =
            getPaymentTermCode?.() ?? order.header.paymentTermCode
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
            lines: draftLinePayload(order, lineEdits),
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

    async function handleSubmit(resubmissionReason?: string) {
        if (!order) return
        if (activeEditing && !draftEditToken) return
        let submitCommand =
            commandLedger.peek<SubmitPurchaseOrderPayload>("submit")
        if (!submitCommand && activeEditing) {
            const validationError = validateDraftLineEdits(order, lineEdits)
            if (validationError) {
                setSubmitConfirmOpen(false)
                setResult({
                    status: "rejected",
                    title: "提交失败",
                    description: validationError,
                })
                return
            }
        } else if (!submitCommand) {
            const deliveryProblem = draftDeliveryProblem(order, lineEdits)
            if (deliveryProblem) {
                setSubmitConfirmOpen(false)
                setResult({
                    status: "rejected",
                    title: "提交失败",
                    description: deliveryProblem,
                })
                return
            }
        }
        if (!submitCommand) {
            submitCommand = commandLedger.acquire(
                "submit",
                `purchase:${purchaseOrderId}:submit`,
                {
                    purchaseOrderId,
                    resubmissionReason,
                    expectedLockVersion: order.identity.lockVersion,
                    expectedDraftContentHash:
                        order.currentContent.subjectHash ??
                        `v${order.currentContent.version}`,
                    draftEditToken: draftEditToken ?? "",
                    paymentTermCode: activeEditing
                        ? (getPaymentTermCode?.() ??
                          order.header.paymentTermCode)
                        : order.header.paymentTermCode,
                    lines: draftLinePayload(order, lineEdits),
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
            setEditLockError(null)
            setResult({
                status: "succeeded",
                title: "已提交审批",
                description: "采购单已提交审批，审批期间不可修改。",
                reference: response.reference,
                facts: [
                    { label: "单据编号", value: response.data.purchaseNo },
                    {
                        label: "提交记录",
                        value: `第 ${response.data.submissionNo} 次提交`,
                    },
                ],
            })
            if (onFinished) onFinished()
            else router.replace(viewHref)
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

    async function handleVoid() {
        if (!order) return
        const payload = {
            purchaseOrderId,
            expectedLockVersion: order.identity.lockVersion,
            reason: "采购草稿不再需要",
        }
        const command = commandLedger.acquire(
            "void",
            `purchase:${purchaseOrderId}:void`,
            payload,
        )
        const response = await voidMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        commandLedger.settle("void", response.status)
        setVoidConfirmOpen(false)
        if (response.status === "succeeded") {
            setDraftEditToken(null)
            setEditLockError(null)
            setResult({
                status: "succeeded",
                title: "采购草稿已作废",
                description:
                    "该采购草稿不再占用销售待分配供给数量；如原任务已经完成，系统已建立新的供给分配任务。",
                reference: response.reference,
            })
            if (onFinished) onFinished()
            else router.replace(viewHref)
            await refetch()
        } else if (response.status === "unknown") {
            setResult({
                status: "unknown",
                title: "作废结果待确认",
                description: response.message,
                reference: documentReference,
            })
        } else {
            setResult({
                status: "blocked",
                title: "无法作废采购草稿",
                description: response.message,
            })
        }
    }

    /** 发起改单并返回新改单 ID，由页面直接打开编辑弹窗（与销售改单一致）。 */
    async function handleStartChange(): Promise<string | undefined> {
        if (!order) return undefined
        const command =
            commandLedger.peek<{
                purchaseOrderId: string
                expectedLockVersion: number
            }>("start-change") ??
            commandLedger.acquire(
                "start-change",
                `purchase:${purchaseOrderId}:change`,
                {
                    purchaseOrderId,
                    expectedLockVersion: order.identity.lockVersion,
                },
            )
        const response = await changeMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        commandLedger.settle("start-change", response.status)
        if (response.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "改单已创建",
                description:
                    "请填写调整内容和变更原因后提交审批；生效前采购单仍按当前版本执行。",
                reference: response.reference,
            })
            await refetch()
            return response.data.changeId
        }
        setResult({
            status: response.status === "unknown" ? "unknown" : "blocked",
            title:
                response.status === "unknown"
                    ? "处理结果待确认"
                    : "无法发起改单",
            description:
                response.status === "unknown"
                    ? "请使用本次操作重试；确认前不要重复发起改单。"
                    : response.message,
            reference:
                response.status === "unknown" ? documentReference : undefined,
        })
        return undefined
    }

    return {
        draftEditToken,
        editLockPending: acquireToken.isPending,
        editLockError,
        retryAcquireEditToken,
        lineEdits,
        setLineEdits,
        submitConfirmOpen,
        setSubmitConfirmOpen,
        voidConfirmOpen,
        setVoidConfirmOpen,
        savePending: saveMutation.isPending,
        submitPending: submitMutation.isPending,
        voidPending: voidMutation.isPending,
        changePending: changeMutation.isPending,
        handleSave,
        handleSubmit,
        handleVoid,
        handleStartChange,
    }
}
