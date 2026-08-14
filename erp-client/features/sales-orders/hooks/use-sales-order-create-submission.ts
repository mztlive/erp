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
    prepareProcurementRejectionResolution,
    type ResolveProcurementRejectionPayload,
    type SalesOrderDraftResumeData,
    type SubmitSalesOrderInput,
} from "@/features/sales-orders/api/sales-orders"
import { localOrderNo } from "@/features/sales-orders/api/mappers"
import {
    useCreateSalesOrderMutation,
    useResolveProcurementRejectionMutation,
    useSaveSalesOrderDraftMutation,
    useSubmitSalesOrderMutation,
} from "@/features/sales-orders/hooks/queries"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import type {
    SalesOrderEditorPurpose,
    SalesOrderEditorResult,
} from "@/features/sales-orders/lib/sales-order-create-form-types"
import type {
    CreateSalesOrderInput,
    SalesOrderCreateIntent,
} from "@/features/sales-orders/types"

const parseEvidenceReferenceIds = (value: string): string[] =>
    Array.from(
        new Set(
            value
                .split(/[\s,，;；]+/)
                .map((item) => item.trim())
                .filter(Boolean),
        ),
    )

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
    purpose: SalesOrderEditorPurpose
    commandLedger: FormalCommandKeyLedger
    onResult?: (result: SalesOrderEditorResult) => void
    onSubmitted?: (salesOrderId: string) => void
}

/**
 * 建单表单的提交侧逻辑：新建 / 保存草稿 / 提交既有草稿 / 驳回后再报。
 * 命令账本保证同一业务动作在结果未知后重试时复用原命令身份。
 */
export function useSalesOrderCreateSubmission({
    initialDraft,
    purpose,
    commandLedger,
    onResult,
    onSubmitted,
}: UseSalesOrderCreateSubmissionOptions) {
    const router = useRouter()
    const createMutation = useCreateSalesOrderMutation()
    const saveDraftMutation = useSaveSalesOrderDraftMutation()
    const submitMutation = useSubmitSalesOrderMutation()
    const resubmitMutation = useResolveProcurementRejectionMutation()
    const submitIntentRef = React.useRef<SalesOrderCreateIntent>("SAVE_DRAFT")
    const [resubmitConfirmOpen, setResubmitConfirmOpen] =
        React.useState(false)
    const [resubmitEvidence, setResubmitEvidence] = React.useState("")
    const [formalFailure, setFormalFailure] =
        React.useState<FormalFailure | null>(null)

    /** 继续编辑场景：草稿在后端的身份与乐观锁版本，保存草稿从"新建"切到"更新"。 */
    const [draftIdentity, setDraftIdentity] = React.useState<DraftIdentity | null>(
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
            if (
                purpose === "resubmit" &&
                commandLedger.peek("procurement-rejection-resolution")
            ) {
                setResubmitConfirmOpen(true)
                return
            }

            let command = commandLedger.peek<
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

            if (!command) {
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
                if (purpose === "resubmit") {
                    setResubmitConfirmOpen(true)
                    return
                }
                command = commandLedger.acquire(
                    "submit-existing",
                    `sales:${draftIdentity.salesOrderId}:submit`,
                    {
                        salesOrderId: draftIdentity.salesOrderId,
                        version: saved.version,
                    },
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

        let command = commandLedger.peek<
            Omit<CreateSalesOrderInput, "idempotencyKey">
        >("create")
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
            return
        }
        form.reset()
        onSubmitted?.(result.salesOrderId)
        if (!onSubmitted) {
            router.push(`/sales/orders/${result.salesOrderId}`)
        }
    }

    const confirmResubmit = async () => {
        if (!draftIdentity) return
        const evidenceIds = parseEvidenceReferenceIds(resubmitEvidence)
        if (evidenceIds.length === 0) {
            throw new Error("请至少填写一项客户重新确认依据 ID")
        }
        let command = commandLedger.peek<ResolveProcurementRejectionPayload>(
            "procurement-rejection-resolution",
        )
        if (command && command.payload.action !== "RESUBMIT_CHANGED_TERMS") {
            onResult?.({
                status: "unknown",
                title: "处理结果待确认",
                description: "另一项处理的结果仍待确认，请先使用原操作重试。",
                reference: draftIdentity.documentNumber,
            })
            throw new Error("另一项处理的结果仍待确认，请先使用原操作重试。")
        }
        try {
            if (!command) {
                const payload = await prepareProcurementRejectionResolution({
                    salesOrderId: draftIdentity.salesOrderId,
                    action: "RESUBMIT_CHANGED_TERMS",
                    customerReconfirmationEvidenceIds: evidenceIds,
                })
                command = commandLedger.acquire(
                    "procurement-rejection-resolution",
                    `sales:${draftIdentity.salesOrderId}:procurement-resubmit`,
                    payload,
                )
            }
            if (!command) return
            const outcome = await resubmitMutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle("procurement-rejection-resolution", "succeeded")
            onResult?.({
                status: "succeeded",
                title: "已改完并再报给采购",
                description: outcome.detail,
                reference: outcome.reference,
                nextResponsible: "采购重新确认",
            })
            onSubmitted?.(draftIdentity.salesOrderId)
        } catch (error) {
            const settlement = command
                ? classifyFormalCommandError(error)
                : "failed"
            commandLedger.settle(
                "procurement-rejection-resolution",
                settlement,
            )
            onResult?.({
                status: settlement === "unknown" ? "unknown" : "blocked",
                title:
                    settlement === "unknown"
                        ? "处理结果待确认"
                        : "还不能再报给采购",
                description: getErrorMessage(
                    error,
                    settlement === "unknown"
                        ? "当前输入已保留，请使用本次操作重试。"
                        : "请确认已改商品或价格后再试。",
                ),
                reference: draftIdentity.documentNumber,
            })
            throw error
        }
    }

    return {
        submitIntentRef,
        handleSubmit,
        confirmResubmit,
        draftIdentity,
        setDraftIdentity,
        draftSaved,
        setDraftSaved,
        formalFailure,
        setFormalFailure,
        resubmitConfirmOpen,
        setResubmitConfirmOpen,
        resubmitEvidence,
        setResubmitEvidence,
        createMutation,
        isResubmitting: resubmitMutation.isPending,
    }
}
