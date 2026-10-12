"use client"
import { useState } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { ReceiptDetailBody } from "@/features/customer-receivables/components/detail-bodies"
import { projectReceipt } from "@/features/customer-receivables/api/mappers"
import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"

export default function ReceiptPreviewCheck() {
    const [mode, setMode] = useState("pending")
    const [client] = useState(() => {
        const cache = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } })
        cache.setQueryData(["customer-receivables", "source-documents", {entryIds:["preview-entry"],counterpartyPartyId:"preview-party"}], {documents:[{id:"preview-order",label:"XS-验收示例",href:"/sales/orders/preview-order"}],unresolved:0})
        return cache
    })
    const row = projectReceipt({id:"preview-receipt", receipt_no:"SK-验收示例", status: mode === "draft" ? "draft" : "in_approval",counterparty_party_id:"preview-party",counterparty_party_name:"回款展示验收（模拟数据）",customer_name:"回款展示验收（模拟数据）",received_at:1791766980,amount:"165.00",bank_reference:null,version:1,created_at:1791766980,allocated_total:"0.00",unallocated_amount:"165.00",allocations:[],pending_allocations:mode === "restricted" ? undefined : mode === "empty" ? [] : [{receivable_entry_id:"preview-entry",allocated_amount:"165.00"}]})
    return <QueryClientProvider client={client}><QuickPreviewSheet id="receipt-preview-check" open onOpenChange={()=>{}} size="preview" title={row.counterpartyPartyName} identity={`回款单：${row.receiptNo}`} summary={<span>{row.statusLabel}</span>} footer={<div className="flex gap-2">{[["pending","审批中"],["draft","草稿"],["empty","未安排"],["restricted","受限"]].map(([id,label])=><Button id={`receipt-check-${id}`} key={id} onClick={()=>setMode(id)}>{label}</Button>)}</div>}><ReceiptDetailBody row={row} readOnly className="space-y-6 text-sm" /></QuickPreviewSheet></QueryClientProvider>
}
