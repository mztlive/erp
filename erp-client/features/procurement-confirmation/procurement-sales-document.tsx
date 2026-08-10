"use client"

import { Button } from "@/components/ui/button"
import { MoneyValue, PaperDocument, QuantityValue } from "@/components/business"
import type {
    ProcurementConfirmationTask,
    SubmissionLineView,
} from "@/features/procurement-confirmation/types"
import { FULFILLMENT_MODE_LABEL } from "@/features/procurement-confirmation/types"
import { formatDateTime } from "@/lib/datetime"

type ProcurementSalesDocumentProps = {
    task: ProcurementConfirmationTask
    onOpenContract?: () => void
}

function taxRateLabel(value: string) {
    const rate = Number(value)
    if (!Number.isFinite(rate)) return value
    return `${new Intl.NumberFormat("zh-CN", {
        maximumFractionDigits: 4,
    }).format(rate * 100)}%`
}

/** 过滤历史数据中误写入规格字段的内部标识，只向采购展示业务规格。 */
function businessSpecification(line: SubmissionLineView) {
    const value = line.specification?.trim()
    if (!value || value === line.itemSku || /^[a-f\d]{24,}$/i.test(value)) {
        return "未填写规格"
    }
    return value
}

/** 采购确认使用的不可变销售提交单据投影。 */
export function ProcurementSalesDocument({
    task,
    onOpenContract,
}: ProcurementSalesDocumentProps) {
    const submission = task.salesSubmission
    const contractReference = submission.contractSnapshot ? (
        onOpenContract ? (
            <Button
                type="button"
                variant="link"
                size="sm"
                className="h-auto p-0 text-xs"
                onClick={onOpenContract}
            >
                查看合同 · {submission.contractSnapshot}
            </Button>
        ) : (
            submission.contractSnapshot
        )
    ) : (
        "未关联合同"
    )

    return (
        <PaperDocument<SubmissionLineView>
            aria-label={`销售单 ${submission.salesOrderNo}`}
            issuer={submission.settlementPartySnapshot ?? "本公司"}
            title="销售单"
            subtitle="采购二次确认所依据的原始提交快照"
            documentNumber={submission.salesOrderNo}
            version={`第 ${submission.submissionNo} 次提交`}
            status={{ label: "待采购确认", tone: "warning" }}
            parties={[
                {
                    id: "seller",
                    label: "销售方",
                    name: submission.settlementPartySnapshot ?? "本公司",
                    reference: contractReference,
                    fields: [
                        {
                            id: "submitted-by",
                            label: "提交人",
                            value: submission.submittedByLabel,
                        },
                    ],
                },
                {
                    id: "customer",
                    label: "客户",
                    name: submission.customerSnapshot,
                    fields: [
                        {
                            id: "payment-term",
                            label: "付款条件",
                            value: submission.paymentTermLabel,
                        },
                    ],
                },
            ]}
            metadata={[
                {
                    id: "submitted-at",
                    label: "提交时间",
                    value: formatDateTime(submission.submittedAt, "default"),
                },
                {
                    id: "project",
                    label: "项目",
                    value: submission.projectName ?? "—",
                },
                {
                    id: "origin",
                    label: "提交来源",
                    value:
                        submission.origin === "INITIAL"
                            ? "首次提交"
                            : "条款调整后重提",
                },
                {
                    id: "line-count",
                    label: "明细数",
                    value: `${submission.lines.length} 项`,
                    numeric: true,
                },
            ]}
            lineItemLabel="销售明细"
            columns={[
                {
                    id: "line-no",
                    header: "序号",
                    cell: (_line, index) => index + 1,
                    align: "center",
                    numeric: true,
                },
                {
                    id: "item",
                    header: "商品",
                    cell: (line) => (
                        <div>
                            <p className="font-medium">{line.itemName}</p>
                            <p className="mt-1 text-xs text-muted-foreground">
                                {businessSpecification(line)}
                            </p>
                        </div>
                    ),
                },
                {
                    id: "quantity",
                    header: "数量",
                    cell: (line) => (
                        <QuantityValue
                            value={line.committedQuantity}
                            unit={line.unit}
                        />
                    ),
                    align: "end",
                    numeric: true,
                },
                {
                    id: "unit-price",
                    header: "销售单价",
                    cell: (line) => (
                        <div className="space-y-1">
                            <MoneyValue
                                value={line.unitPriceGross}
                                taxBasis="gross"
                            />
                            {line.salesTaxRate ? (
                                <p className="text-xs text-muted-foreground">
                                    税率 {taxRateLabel(line.salesTaxRate)}
                                </p>
                            ) : null}
                        </div>
                    ),
                    align: "end",
                    numeric: true,
                },
                {
                    id: "amount",
                    header: "销售金额",
                    cell: (line) => (
                        <MoneyValue
                            value={line.salesAmountGross}
                            taxBasis="gross"
                        />
                    ),
                    align: "end",
                    numeric: true,
                },
                {
                    id: "delivery",
                    header: "交付",
                    cell: (line) => (
                        <div className="whitespace-nowrap">
                            <p>
                                {line.fulfillmentMode
                                    ? FULFILLMENT_MODE_LABEL[
                                          line.fulfillmentMode
                                      ]
                                    : "未指定方式"}
                            </p>
                            <p className="mt-1 text-xs text-muted-foreground">
                                {line.requestedDeliveryDate || "未填写日期"}
                            </p>
                        </div>
                    ),
                },
            ]}
            rows={submission.lines}
            getRowId={(line) => line.submissionLineId}
            totals={[
                {
                    id: "net",
                    label: "不含税金额",
                    value: (
                        <MoneyValue
                            value={submission.netAmount}
                            taxBasis="net"
                        />
                    ),
                },
                {
                    id: "tax",
                    label: "税额",
                    value: <MoneyValue value={submission.taxAmount} />,
                },
                {
                    id: "gross",
                    label: "价税合计",
                    value: (
                        <MoneyValue
                            value={submission.grossAmount}
                            taxBasis="gross"
                        />
                    ),
                    emphasized: true,
                },
            ]}
            remarks={submission.businessRemark ?? "无业务备注"}
            footer={
                <div className="flex items-center justify-between gap-6">
                    <span>该单据为本次采购二次确认的完整原始销售提交。</span>
                    <span>提交后不可修改；采购结论绑定本版本。</span>
                </div>
            }
        />
    )
}
