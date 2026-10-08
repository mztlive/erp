"use client"

import * as React from "react"

import type { SalesOrderDraftResumeData } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrderNature } from "@/features/sales-orders/types"
import {
    createEmptyLine,
    type CreateSalesOrderFormValues,
} from "@/features/sales-orders/lib/sales-order-create-model"

export type SalesOrderCreateDefaultsInput = {
    initialCustomerId: string
    initialContractId: string
    initialContractRevisionId: string
    initialNature: SalesOrderNature
    /** 继续编辑 / 驳回改单：已有可编辑内容；新建时为 `null`。 */
    initialDraft: SalesOrderDraftResumeData | null
}

/**
 * 必须稳定：createEmptyLine 每次生成新 rowKey。
 * useAppForm 在 layout effect 里会 deep-compare defaultValues，
 * 未 touch 时若每次渲染都变，会 setState → 重渲染 → 死循环
 *（Maximum update depth exceeded @ Field）。
 */
export function useSalesOrderCreateDefaults({
    initialCustomerId,
    initialContractId,
    initialContractRevisionId,
    initialNature,
    initialDraft,
}: SalesOrderCreateDefaultsInput): CreateSalesOrderFormValues {
    return React.useMemo(() => {
        const nature = initialDraft?.nature ?? initialNature
        const contractId = initialDraft?.contractId || initialContractId
        return {
            orderBasis: initialDraft && !contractId ? "evidence" : "contract",
            contractId,
            requestedContractRevisionId:
                initialDraft?.requestedContractRevisionId ||
                initialContractRevisionId,
            contractRevisionLabel: "",
            customerId: initialDraft?.customerId || initialCustomerId,
            evidenceUploadPending: false,
            evidenceAttachments: initialDraft?.evidenceFiles?.length
                ? initialDraft.evidenceFiles
                : (initialDraft?.evidenceFileAssetIds?.map((id) => ({
                      id,
                      fileName: "已上传开单凭证",
                  })) ?? []),
            customerName: initialDraft?.customerName ?? "",
            settlementPartyId: initialDraft?.settlementPartyId ?? "",
            settlementEntity: initialDraft?.settlementEntity ?? "",
            nature,
            ownerUserId: "",
            ownerName: "",
            welfareScene: initialDraft?.welfareScene ?? "",
            paymentTerms: initialDraft?.paymentTerms ?? "",
            invoiceType: initialDraft?.invoiceType ?? "增值税专用发票",
            fulfillmentDeadline: initialDraft?.fulfillmentDeadline ?? "",
            receivableDueDate: initialDraft?.receivableDueDate ?? "",
            taxRatePercent:
                initialDraft?.taxRatePercent ??
                (nature === "card_voucher" ? "6.00" : "13.00"),
            remark: initialDraft?.remark ?? "",
            lineItems:
                initialDraft && initialDraft.lineItems.length > 0
                    ? initialDraft.lineItems
                    : nature === "card_voucher"
                      ? [createEmptyLine(nature)]
                      : [],
        } satisfies CreateSalesOrderFormValues
        // initialDraft 由外层等查询完成后才挂载本组件，渲染期间引用稳定，可以放心入依赖数组。
    }, [
        initialContractId,
        initialContractRevisionId,
        initialCustomerId,
        initialNature,
        initialDraft,
    ])
}
