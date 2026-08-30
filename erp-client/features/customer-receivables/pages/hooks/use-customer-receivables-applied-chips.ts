"use client"

import * as React from "react"

import {
    businessLabelOrPlaceholder,
    MISSING_COUNTERPARTY_NAME,
    MISSING_CUSTOMER_NAME,
} from "@/features/customer-receivables/lib/display-labels"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import {
    DUE_LABEL,
    RECEIVABLE_STATUS_LABEL,
    REVIEW_STATUS_LABEL,
} from "@/features/customer-receivables/types"
import type { ReceivableAppliedChip } from "../components/customer-receivables-toolbar"
import type { useCustomerReceivablesUrlState } from "./use-customer-receivables-url-state"

type Options = {
    data: CustomerAccountsListView | undefined
    urlState: ReturnType<typeof useCustomerReceivablesUrlState>
    embedded: boolean
    embeddedCounterpartyId: string | undefined
    embeddedCounterpartyName: string | undefined
}

/** 将全部已生效客户往来筛选投影为可移除 chip。 */
export function useCustomerReceivablesAppliedChips({
    data,
    urlState,
    embedded,
    embeddedCounterpartyId,
    embeddedCounterpartyName,
}: Options): readonly ReceivableAppliedChip[] {
    const lockedCustomerName = React.useMemo(
        () =>
            data?.counterparties.find(
                (counterparty) =>
                    counterparty.customerId === urlState.customerId,
            )?.customerName,
        [data?.counterparties, urlState.customerId],
    )

    return React.useMemo(() => {
        const chips: ReceivableAppliedChip[] = []
        const queryText = urlState.qParam.trim()
        if (queryText) {
            chips.push({ key: "q", label: `搜索：${queryText}` })
        }
        if (urlState.counterpartyPartyId) {
            const counterparty = data?.counterparties.find(
                (candidate) =>
                    candidate.counterpartyPartyId ===
                    urlState.counterpartyPartyId,
            )
            const embeddedName =
                embedded &&
                embeddedCounterpartyId === urlState.counterpartyPartyId
                    ? embeddedCounterpartyName
                    : undefined
            chips.push({
                key: "counterpartyId",
                label: `往来主体：${businessLabelOrPlaceholder(
                    counterparty?.counterpartyPartyName ?? embeddedName,
                    urlState.counterpartyPartyId,
                    MISSING_COUNTERPARTY_NAME,
                )}`,
            })
        }
        if (urlState.customerId) {
            chips.push({
                key: "customerId",
                label: `经营客户：${businessLabelOrPlaceholder(
                    lockedCustomerName,
                    urlState.customerId,
                    MISSING_CUSTOMER_NAME,
                )}`,
            })
        }
        if (urlState.due && urlState.due !== "all") {
            chips.push({
                key: "due",
                label: `到期：${DUE_LABEL[urlState.due]}`,
            })
        }
        if (urlState.status) {
            chips.push({
                key: "status",
                label: `状态：${RECEIVABLE_STATUS_LABEL[urlState.status]}`,
            })
        }
        if (urlState.reviewStatus) {
            chips.push({
                key: "reviewStatus",
                label: `复核状态：${REVIEW_STATUS_LABEL[urlState.reviewStatus]}`,
            })
        }
        if (urlState.salesOrderId && !embedded) {
            const receivable = data?.receivables.find(
                (candidate) => candidate.salesOrderId === urlState.salesOrderId,
            )
            chips.push({
                key: "salesOrderId",
                label: receivable?.salesOrderNo
                    ? `销售单：${receivable.salesOrderNo}`
                    : "已限定销售单",
            })
        }
        if (urlState.receivableAccountId) {
            const receivable = data?.receivables.find(
                (candidate) =>
                    candidate.accountId === urlState.receivableAccountId,
            )
            chips.push({
                key: "receivableAccountId",
                label:
                    receivable?.accountSeq != null
                        ? `往来子账：${receivable.accountSeq}`
                        : "已限定往来子账",
            })
        }
        return chips
    }, [
        data?.counterparties,
        data?.receivables,
        embedded,
        embeddedCounterpartyId,
        embeddedCounterpartyName,
        lockedCustomerName,
        urlState.counterpartyPartyId,
        urlState.customerId,
        urlState.due,
        urlState.qParam,
        urlState.receivableAccountId,
        urlState.reviewStatus,
        urlState.salesOrderId,
        urlState.status,
    ])
}
