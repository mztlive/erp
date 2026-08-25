"use client"

import * as React from "react"

import { useRouter } from "next/navigation"

import {
    classifyFormalCommandError,
    type FormalCommandKeyLedger,
} from "@/lib/formal-command"
import { getErrorMessage } from "@/lib/api/errors"
import { paymentTermLabel } from "@/lib/business-options"
import {
    type SalesOrderDraftResumeData,
    type SubmitSalesOrderInput,
} from "@/features/sales-orders/api/sales-orders"
import { localOrderNo } from "@/features/sales-orders/api/mappers"
import {
    useCreateSalesOrderMutation,
    useSaveSalesOrderDraftMutation,
    useSubmitSalesOrderMutation,
} from "@/features/sales-orders/hooks/queries"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type {
    CreateSalesOrderInput,
    SalesOrderCreateIntent,
} from "@/features/sales-orders/types"

type DraftIdentity = {
    salesOrderId: string
    documentNumber: string
    version: number
}

type FormalFailure = {
    unknown: boolean
    description: string
}

export type UseSalesOrderCreateSubmissionOptions = {
    initialDraft: SalesOrderDraftResumeData | null
    commandLedger: FormalCommandKeyLedger
    onSubmitted?: (salesOrderId: string) => void
}

/**
 * 建单表单的提交侧逻辑：新建 / 保存草稿 / 提交既有草稿。
 * 命令账本保证同一业务动作在结果未知后重试时复用原命令身份。
 */
export function useSalesOrderCreateSubmission({
    initialDraft,
    commandLedger,
    onSubmitted,
}: UseSalesOrderCreateSubmissionOptions) {
    const router = useRouter()
    const createMutation = useCreateSalesOrderMutation()
    const saveDraftMutation = useSaveSalesOrderDraftMutation()
    const submitMutation = useSubmitSalesOrderMutation()
    const submitIntentRef = React.useRef<SalesOrderCreateIntent>("SAVE_DRAFT")
    const [formalFailure, setFormalFailure] =
        React.useState<FormalFailure | null>(null)

    /** 继续编辑场景：草稿在后端的身份与乐观锁版本，保存草稿从"新建"切到"更新"。 */
    const [draftIdentity, setDraftIdentity] =
        React.useState<DraftIdentity | null>(
            initialDraft
                ? {
                      salesOrderId: initialDraft.salesOrderId,
                      documentNumber: initialDraft.documentNumber,
                      version: initialDraft.version,
                  }
                : null,
        )
    const [draftSaved, setDraftSaved] = React.useState<{
        documentNumber: string
        savedAt: Date
    } | null>(null)
    const [approval, setApproval] = React.useState<
        DocumentApprovalView | undefined
    >(initialDraft?.approval)
    const [submitConfirmOpen, setSubmitConfirmOpen] = React.useState(false)

    const handleSubmit = async (
        value: CreateSalesOrderFormValues,
        form: { reset: () => void },
    ) => {
        const draftContent = {
            nature: value.nature,
            ownerUserId: value.ownerUserId,
            ownerName: value.ownerName,
            welfareScene: value.welfareScene,
            paymentTerms:
                paymentTermLabel(value.paymentTerms) || value.paymentTerms,
            fulfillmentDeadline: value.fulfillmentDeadline,
            targetMallId: value.targetMallId,
            receivableDueDate: value.receivableDueDate,
            taxRatePercent: value.taxRatePercent,
            remark: value.remark,
            lineItems: value.lineItems,
        }

        // 已经落过库的草稿：后续保存/提交都基于既有记录续接，不再新建。
        if (draftIdentity) {
            let command =
                commandLedger.peek<
                    Omit<SubmitSalesOrderInput, "idempotencyKey">
                >("submit-existing")
            if (submitIntentRef.current === "SAVE_DRAFT" && !command) {
                const saved = await saveDraftMutation.mutateAsync({
                    ...draftContent,
                    salesOrderId: draftIdentity.salesOrderId,
                    version: draftIdentity.version,
                    contract: {
                        contractId: value.contractId,
                        requestedContractRevisionId:
                            value.requestedContractRevisionId,
                    },
                })
                setDraftIdentity({
                    salesOrderId: draftIdentity.salesOrderId,
                    documentNumber: draftIdentity.documentNumber,
                    version: saved.version,
                })
                setDraftSaved({
                    documentNumber: draftIdentity.documentNumber,
                    savedAt: new Date(),
                })
                return
            }

            const completeDraft = {
                ...draftContent,
                salesOrderId: draftIdentity.salesOrderId,
                version: draftIdentity.version,
                contract: {
                    contractId: value.contractId,
                    requestedContractRevisionId:
                        value.requestedContractRevisionId,
                },
            }
            if (!command) {
                command = commandLedger.acquire(
                    "submit-existing",
                    `sales:${draftIdentity.salesOrderId}:submit`,
                    completeDraft,
                )
            }
            if (!command) return
            try {
                await submitMutation.mutateAsync({
                    ...command.payload,
                    idempotencyKey: command.idempotencyKey,
                })
                commandLedger.settle("submit-existing", "succeeded")
                setFormalFailure(null)
            } catch (error) {
                const settlement = classifyFormalCommandError(error)
                commandLedger.settle("submit-existing", settlement)
                setFormalFailure({
                    unknown: settlement === "unknown",
                    description:
                        settlement === "unknown"
                            ? "当前输入已保留，请使用本次操作重试；确认前不要再次提交。"
                            : getErrorMessage(error, "提交未完成，请重试。"),
                })
                throw error
            }
            form.reset()
            onSubmitted?.(draftIdentity.salesOrderId)
            if (!onSubmitted) {
                router.push(`/sales/orders/${draftIdentity.salesOrderId}`)
            }
            return
        }

        let command =
            commandLedger.peek<Omit<CreateSalesOrderInput, "idempotencyKey">>(
                "create",
            )
        if (!command) {
            command = commandLedger.acquire("create", "sales:create", {
                orderNo: localOrderNo(),
                contract: {
                    contractId: value.contractId,
                    requestedContractRevisionId:
                        value.requestedContractRevisionId,
                },
                ...draftContent,
                intent: submitIntentRef.current,
            })
        }
        if (!command) return
        let result
        try {
            result = await createMutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle("create", "succeeded")
            setFormalFailure(null)
        } catch (error) {
            const settlement = classifyFormalCommandError(error)
            commandLedger.settle("create", settlement)
            setFormalFailure({
                unknown: settlement === "unknown",
                description:
                    settlement === "unknown"
                        ? "当前整单输入已保留，请使用本次操作重试；确认前不要再次创建。"
                        : getErrorMessage(error, "销售单未创建，请重试。"),
            })
            throw error
        }
        if (command.payload.intent === "SAVE_DRAFT") {
            setDraftIdentity({
                salesOrderId: result.salesOrderId,
                documentNumber: result.documentNumber,
                version: result.workingCopyVersion ?? 1,
            })
            setDraftSaved({
                documentNumber: result.documentNumber,
                savedAt: new Date(),
            })
            setApproval(result.approval)
            return
        }
        form.reset()
        onSubmitted?.(result.salesOrderId)
        if (!onSubmitted) {
            router.push(`/sales/orders/${result.salesOrderId}`)
        }
    }

    return {
        submitIntentRef,
        handleSubmit,
        draftIdentity,
        setDraftIdentity,
        draftSaved,
        setDraftSaved,
        approval,
        submitConfirmOpen,
        setSubmitConfirmOpen,
        formalFailure,
        setFormalFailure,
        createMutation,
        isSubmitting:
            createMutation.isPending ||
            saveDraftMutation.isPending ||
            submitMutation.isPending,
    }
}
