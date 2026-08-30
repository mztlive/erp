"use client"

import * as React from "react"

import type {
    AllocationTrack,
    ReverseTarget,
} from "@/features/supplier-payables/types"
import { useSupplierAccountsFilters } from "./use-supplier-accounts-filters"
import { useSupplierAccountsNavigation } from "./use-supplier-accounts-navigation"

/** W12 页面组合根；筛选查询与会话导航由独立控制器负责。 */
export function useSupplierAccountsPage() {
    const filters = useSupplierAccountsFilters()
    const navigation = useSupplierAccountsNavigation({
        data: filters.data,
        supplierId: filters.supplierId,
        purchaseOrderId: filters.purchaseOrderId,
        patchUrl: filters.patchUrl,
    })

    const [pickSupplierOpen, setPickSupplierOpen] =
        React.useState<AllocationTrack | null>(null)
    const [pickSupplierId, setPickSupplierId] = React.useState("")
    const [reverseTarget, setReverseTarget] =
        React.useState<ReverseTarget | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [redInvoiceNo, setRedInvoiceNo] = React.useState("")

    return {
        ...filters,
        ...navigation,
        pickSupplierOpen,
        setPickSupplierOpen,
        pickSupplierId,
        setPickSupplierId,
        reverseTarget,
        setReverseTarget,
        reverseReason,
        setReverseReason,
        redInvoiceNo,
        setRedInvoiceNo,
    }
}
