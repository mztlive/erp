"use client"

import { DetailWorkspaceSection } from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderContractSupplement } from "@/features/sales-orders/components/sales-order-contract-supplement"
import { SalesOrderEvidenceAttachments } from "@/features/sales-orders/components/sales-order-evidence-attachments"

function ContractField({
    label,
    value,
    numeric,
}: {
    label: string
    value: string
    numeric?: boolean
}) {
    return (
        <div className="min-w-0 space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
                {label}
            </dt>
            <dd
                className={
                    numeric
                        ? "num break-words text-body-compact text-foreground"
                        : "break-words text-body-compact text-foreground"
                }
            >
                {value || "—"}
            </dd>
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
        <>
            <DetailWorkspaceSection
                width="form"
                title="销售合同"
                subtitle={
                    hasContract
                        ? "合同条款随销售版本锁定，补录合同不会改写历史版本。"
                        : evidenceFiles.length
                          ? "本单未关联合同，以开单凭证作为开单依据。"
                          : "本单尚未关联合同或归档开单凭证。"
                }
                actions={<SalesOrderContractSupplement order={order} />}
            >
                {hasContract ? (
                    <dl className="grid gap-x-8 gap-y-4 sm:grid-cols-2">
                        <ContractField
                            label="合同编号"
                            value={
                                order.contractNumber ||
                                order.contractRevisionLabel
                            }
                            numeric
                        />
                        <ContractField
                            label="合同修订"
                            value={order.contractRevisionLabel}
                            numeric
                        />
                        <ContractField
                            label="合同客户"
                            value={
                                order.contractCompanyName || order.customerName
                            }
                        />
                    </dl>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        当前销售版本未关联合同。
                    </p>
                )}
            </DetailWorkspaceSection>

            <SalesOrderEvidenceAttachments
                salesOrderId={order.id}
                files={evidenceFiles}
            />
        </>
    )
}
