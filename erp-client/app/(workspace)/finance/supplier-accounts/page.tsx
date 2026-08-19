import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { SupplierAccountsPage } from "@/features/supplier-payables/pages/supplier-accounts-page"

export const metadata: Metadata = {
    title: "供应商往来",
}

function SupplierAccountsFallback() {
    return (
        <PageScaffold>
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
                {Array.from({ length: 5 }).map((_, i) => (
                    <div
                        key={i}
                        className="h-20 animate-pulse rounded-lg bg-muted"
                    />
                ))}
            </div>
            <div className="h-12 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </PageScaffold>
    )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * view / supplierId / q / purchaseOrderId / from / returnTo / session /
 * previewKind / detailId / currentWorkItemId。
 * 业务数据不在服务端 fetch。
 *
 * Invoice 为 NO_APPROVAL：本页进项发票详情/预览/登记不显示审批流程选择或审批动作。
 * SupplierRefund 为 PROCESS_REQUIRED：退款详情/预览/提交走通用审批区。
 * PaymentReversal 为 PROCESS_REQUIRED：冲正详情/预览/提交走通用审批区。
 */
export default function Page() {
    return (
        <Suspense fallback={<SupplierAccountsFallback />}>
            <SupplierAccountsPage />
        </Suspense>
    )
}
