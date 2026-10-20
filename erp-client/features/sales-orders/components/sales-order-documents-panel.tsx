"use client"

import { DocumentSection } from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderContractSupplement } from "@/features/sales-orders/components/sales-order-contract-supplement"
import { SalesOrderEvidenceAttachments } from "@/features/sales-orders/components/sales-order-evidence-attachments"

function ContractField({ label, value }: { label: string; value: string }) {
    return (
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="mt-1 break-words text-sm">{value || "—"}</dd>
        </div>
    )
}

/** 销售单「合同凭证」分区：合同条款与随单归档的开单依据集中查阅。 */
export function SalesOrderDocumentsPanel({
    order,
}: {
    order: SalesOrderDetailView
}) {
    const hasContract = Boolean(order.contractId || order.contractRevisionLabel)
    const evidenceFiles = order.evidenceFiles?.length
        ? order.evidenceFiles
        : (order.evidenceFileAssetIds ?? []).map((id) => ({
              id,
              fileName: "开单凭证",
          }))
    return (
        <div className="space-y-6">
            <DocumentSection
                title="销售合同"
                className="border-0 py-0 [&_h2]:text-sm [&>div:last-child]:pt-3"
                description={
                    hasContract
                        ? "合同条款随销售版本锁定，补录合同不会改写历史版本。"
                        : evidenceFiles.length
                          ? "本单未关联合同，以开单凭证作为开单依据。"
                          : "本单尚未关联合同或归档开单凭证。"
                }
                action={<SalesOrderContractSupplement order={order} />}
            >
                {hasContract ? (
                    <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
                        <ContractField
                            label="合同编号"
                            value={
                                order.contractNumber ||
                                order.contractRevisionLabel
                            }
                        />
                        <ContractField
                            label="合同修订"
                            value={order.contractRevisionLabel}
                        />
                        <ContractField
                            label="合同客户"
                            value={
                                order.contractCompanyName || order.customerName
                            }
                        />
                    </dl>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        当前销售版本未关联合同。
                    </p>
                )}
            </DocumentSection>
            <SalesOrderEvidenceAttachments
                salesOrderId={order.id}
                files={evidenceFiles}
            />
        </div>
    )
}
