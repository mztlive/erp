"use client"

import * as React from "react"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    useAllocationSessionQuery,
    useCreateAllocationSessionMutation,
    useCustomerAccountsDetailQuery,
    useCustomerAccountsListQuery,
} from "@/features/customer-receivables/hooks/queries"
import { useCustomerReceivablesPermissions } from "@/features/customer-receivables/hooks/use-customer-receivables-permissions"
import type {
    AllocationMode,
    CustomerAccountsDetailKind,
    ReceivableAccountRow,
    ReceiptRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { getErrorMessage } from "@/lib/api/errors"
import {
    receivableTargetIds,
    remainingReceivableAmount,
} from "@/features/sales-orders/lib/sales-order-receivable"

export type OrderReceivablePreview = {
    kind: CustomerAccountsDetailKind
    id: string
}

const EMPTY_ACCOUNTS: readonly ReceivableAccountRow[] = []
const EMPTY_RECEIPTS: readonly ReceiptRow[] = []
const EMPTY_INVOICES: readonly SalesInvoiceRow[] = []

/**
 * 销售单票款分区的本单数据与登记会话。只拉本单应收/回款/发票，不进入客户往来工作台状态。
 *
 * @param order 当前销售单详情
 * @param onDataChanged 登记成功后刷新销售单详情
 */
export function useSalesOrderReceivable(
    order: SalesOrderDetailView,
    onDataChanged: () => void,
) {
    const permissions = useCustomerReceivablesPermissions()
    const listQueryInput = React.useMemo(
        () => ({
            view: "receivable" as const,
            page: 1,
            pageSize: 100,
            salesOrderId: order.id,
            customerId: order.customerId,
        }),
        [order.customerId, order.id],
    )
    const listQuery = useCustomerAccountsListQuery(listQueryInput)
    const createSession = useCreateAllocationSessionMutation()

    const [sessionId, setSessionId] = React.useState<string | null>(null)
    const [preview, setPreview] = React.useState<OrderReceivablePreview | null>(
        null,
    )
    const [actionError, setActionError] = React.useState<string | null>(null)

    const sessionQuery = useAllocationSessionQuery(sessionId)
    const detailQuery = useCustomerAccountsDetailQuery(
        preview?.kind ?? null,
        preview?.id ?? null,
    )

    const data = listQuery.data
    const accounts = data?.receivables ?? EMPTY_ACCOUNTS
    const receipts = data?.receipts ?? EMPTY_RECEIPTS
    const invoices = data?.invoices ?? EMPTY_INVOICES
    const targetIds = React.useMemo(
        () => receivableTargetIds(accounts),
        [accounts],
    )
    const uniqueAccount = accounts.length === 1 ? accounts[0] : undefined
    const remaining = remainingReceivableAmount(
        order.amountGross,
        order.receivedAmount,
    )

    const closeSession = React.useCallback(() => {
        setSessionId(null)
    }, [])

    const closePreview = React.useCallback(() => {
        setPreview(null)
    }, [])

    const startSession = React.useCallback(
        async (
            mode: AllocationMode,
            partyId: string,
            existingFactId?: string,
            target?: { salesOrderId?: string; receivableAccountId?: string },
        ) => {
            const allowed =
                mode === "receipt"
                    ? permissions.canRegisterReceipt
                    : permissions.canRegisterInvoice
            if (!allowed) {
                setActionError(permissions.reason)
                return
            }
            setActionError(null)
            try {
                const session = await createSession.mutateAsync({
                    mode,
                    counterpartyPartyId: partyId,
                    counterpartyPartyName: order.settlementEntity,
                    customerId: order.customerId,
                    customerName: order.customerName,
                    existingFactId,
                    salesOrderId: target?.salesOrderId ?? order.id,
                    receivableAccountId:
                        target?.receivableAccountId ?? uniqueAccount?.accountId,
                    from: "W05",
                })
                setPreview(null)
                setSessionId(session.draftSessionId)
            } catch (error) {
                setActionError(getErrorMessage(error, "创建本次核销失败"))
            }
        },
        [
            createSession,
            order.customerId,
            order.customerName,
            order.id,
            order.settlementEntity,
            permissions.canRegisterInvoice,
            permissions.canRegisterReceipt,
            permissions.reason,
            uniqueAccount?.accountId,
        ],
    )

    const openRegister = React.useCallback(
        (mode: AllocationMode) => {
            const partyId =
                order.settlementPartyId ??
                uniqueAccount?.counterpartyPartyId ??
                accounts[0]?.counterpartyPartyId
            if (!partyId) {
                setActionError(
                    "当前销售单缺少结算主体，无法在本页登记票款。请先补齐销售单结算信息。",
                )
                return
            }
            void startSession(mode, partyId)
        },
        [accounts, order.settlementPartyId, startSession, uniqueAccount],
    )

    const handlePosted = React.useCallback(() => {
        setSessionId(null)
        void listQuery.refetch()
        onDataChanged()
    }, [listQuery, onDataChanged])

    const canRegister = Boolean(
        order.settlementPartyId ||
        uniqueAccount?.counterpartyPartyId ||
        accounts[0]?.counterpartyPartyId,
    )

    return {
        permissions,
        listQuery,
        data,
        accounts,
        receipts,
        invoices,
        targetIds,
        remaining,
        sessionId,
        sessionQuery,
        preview,
        detailQuery,
        actionError,
        createPending: createSession.isPending,
        canRegister,
        openPreview: setPreview,
        closePreview,
        closeSession,
        startSession,
        openRegister,
        handlePosted,
        setActionError,
    }
}
