"use client"

import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import { SalesLinePriceComparison } from "@/components/business/sales-line-price-comparison"

import {
    DeliveryAllocationPrintAppendix,
    MoneyValue,
    PaperDocument,
    QuantityValue,
    RateValue,
} from "@/components/business"
import {
    buildDeliveryAllocationSummary,
    formatLineDeliveryHint,
    type DeliveryAllocationLineInput,
} from "@/lib/delivery-allocation-summary"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { displayBusinessText } from "@/lib/display-name"
import { welfareScenarioLabel } from "@/lib/business-options"
import type {
    ApprovalSalesSubmission,
    ApprovalSalesRevision,
} from "../api/materials"
import {
    approvalSalesTaxPoint,
    approvalSalesUnit,
    displayActorName,
    displayReadableName,
    displayUnixDate,
    displayUnixSeconds,
} from "../display"

const dateLabel = (value: number | null) => displayUnixDate(value)

/** 仅使用审批实例授权返回的不可变提交，不读取当前销售详情。 */
export function ApprovalSalesOrderPaper({
    submission,
    documentNo,
    submitterName,
}: {
    submission: ApprovalSalesSubmission | ApprovalSalesRevision
    documentNo: string
    submitterName: string | null
}) {
    const isSubmission = "submission_no" in submission
    const isCard = isSubmission
        ? submission.business_type === "VOUCHER"
        : !!submission.voucher_category_sku_id
    const lines = isSubmission ? submission.lines : submission.commercial_lines
    const { names } = useProductUnitNames(
        lines.map((line) => line.sku_id),
        !isCard && lines.length > 0,
    )
    const deliveryLines: DeliveryAllocationLineInput[] = lines.map((line) => ({
        id: line.id,
        lineNo: line.line_no,
        itemName:
            displayReadableName(line.item_name_snapshot, line.sku_id) ||
            "未记录商品名称",
        specification: displayBusinessText(line.spec_snapshot, line.sku_id),
        unit: approvalSalesUnit(line, isCard, names),
        quantity:
            line.quantity ??
            (line.card_count == null ? null : String(line.card_count)),
        allocations: line.delivery_allocations,
    }))
    const deliverySummary = buildDeliveryAllocationSummary(deliveryLines)
    const deliveryItemById = new Map(
        deliverySummary.itemGroups.map((group) => [group.lineId, group]),
    )
    return (
        <PaperDocument<ApprovalSalesSubmission["lines"][number]>
            frame="borderless"
            title="销售单"
            documentNumber={documentNo}
            subtitle={isCard ? "卡券" : "实物及服务"}
            version={
                isSubmission
                    ? `提交版本 ${submission.submission_no}`
                    : `销售版本 V${submission.revision_no}`
            }
            parties={[
                {
                    id: "seller",
                    label: "结算主体",
                    name:
                        displayReadableName(submission.settlement_party_name) ||
                        "未记录结算主体名称",
                    fields: [
                        {
                            id: "applicant",
                            label: "申请人",
                            value:
                                displayActorName(
                                    submitterName,
                                    isSubmission
                                        ? submission.submitted_by
                                        : undefined,
                                ) || "未记录姓名",
                        },
                        {
                            id: "submitted",
                            label: isSubmission ? "提交时间" : "生效时间",
                            value: (
                                <span className="num">
                                    {displayUnixSeconds(
                                        isSubmission
                                            ? submission.submitted_at
                                            : submission.effective_at,
                                    )?.label ?? "—"}
                                </span>
                            ),
                        },
                        {
                            id: "invoice",
                            label: "开票要求",
                            value: submission.invoice_type || "—",
                        },
                        {
                            id: "receivable-due",
                            label: "收款到期日",
                            value:
                                (isSubmission
                                    ? submission.receivable_due_date
                                    : null) || "—",
                        },
                    ],
                },
                {
                    id: "customer",
                    label: "客户",
                    name:
                        displayReadableName(submission.customer_name) ||
                        "未记录客户名称",
                    fields: [
                        {
                            id: "contract",
                            label: "合同",
                            value: submission.contract_no || "无合同",
                        },
                        {
                            id: "project",
                            label: "项目",
                            value:
                                displayReadableName(submission.project_name) ||
                                "—",
                        },
                    ],
                },
            ]}
            metadata={[
                {
                    id: "payment",
                    label: "付款条件",
                    value:
                        displayReadableName(submission.payment_term_name) ||
                        "未记录付款条件名称",
                },
                {
                    id: "tax-point",
                    label: "约定税点",
                    value: approvalSalesTaxPoint(submission.tax_point),
                },
                ...(isCard
                    ? [
                          {
                              id: "voucher-due",
                              label: "卡券履约期限",
                              value: dateLabel(submission.voucher_expiry_at),
                          },
                      ]
                    : []),
            ]}
            lineItemLabel={`销售明细 · 共 ${lines.length} 行`}
            columns={[
                {
                    id: "line",
                    header: "序号",
                    cell: (row) => row.line_no,
                    numeric: true,
                },
                {
                    id: "name",
                    header: "商品 / 规格",
                    cell: (row) => (
                        <div>
                            <div>
                                {displayReadableName(
                                    row.item_name_snapshot,
                                    row.sku_id,
                                ) || "未记录商品名称"}
                            </div>
                            {displayBusinessText(
                                row.spec_snapshot,
                                row.sku_id,
                            ) && (
                                <div className="mt-1 text-xs text-muted-foreground">
                                    {displayBusinessText(
                                        row.spec_snapshot,
                                        row.sku_id,
                                    )}
                                </div>
                            )}
                            {row.pricing_mode === "GIFT" && (
                                <div className="mt-1 text-xs text-muted-foreground">
                                    赠品
                                </div>
                            )}
                            {row.welfare_scenario && (
                                <div className="mt-1 text-xs text-muted-foreground">
                                    {welfareScenarioLabel(row.welfare_scenario)}
                                </div>
                            )}
                            {(() => {
                                const hint = formatLineDeliveryHint(
                                    deliveryItemById.get(row.id),
                                    deliverySummary,
                                )
                                return hint ? (
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        收货：{hint}
                                    </p>
                                ) : null
                            })()}
                        </div>
                    ),
                },
                {
                    id: "quantity",
                    header: "数量",
                    align: "end",
                    cell: (row) => (
                        <QuantityValue
                            value={row.quantity ?? String(row.card_count ?? 0)}
                            unit={approvalSalesUnit(row, isCard, names)}
                        />
                    ),
                },
                {
                    id: "price",
                    header: "含税单价",
                    align: "end",
                    cell: (row) => (
                        <MoneyValue
                            display="exact"
                            value={row.unit_price_gross}
                        />
                    ),
                },
                ...(isCard
                    ? [
                          {
                              id: "face",
                              header: "面额",
                              align: "end" as const,
                              cell: (
                                  row: ApprovalSalesSubmission["lines"][number],
                              ) =>
                                  row.face_value ? (
                                      <MoneyValue
                                          display="exact"
                                          value={row.face_value}
                                      />
                                  ) : (
                                      "—"
                                  ),
                          },
                          {
                              id: "form",
                              header: "形态",
                              cell: (
                                  row: ApprovalSalesSubmission["lines"][number],
                              ) =>
                                  row.card_form === "PHYSICAL"
                                      ? "实体卡"
                                      : row.card_form === "ELECTRONIC"
                                        ? "电子券"
                                        : "—",
                          },
                      ]
                    : [
                          {
                              id: "supply-price",
                              header: "采购供给价",
                              align: "end" as const,
                              cell: (
                                  row: ApprovalSalesSubmission["lines"][number],
                              ) => (
                                  <SalesLinePriceComparison
                                      prices={row}
                                      showSalesPrice={false}
                                      unit={approvalSalesUnit(
                                          row,
                                          isCard,
                                          names,
                                      )}
                                  />
                              ),
                          },
                          {
                              id: "due",
                              header: "交付日期",
                              cell: (
                                  row: ApprovalSalesSubmission["lines"][number],
                              ) => dateLabel(row.fulfillment_due_at),
                          },
                      ]),
                {
                    id: "rate",
                    header: "税率",
                    align: "end",
                    cell: (row) => (
                        <RateValue
                            value={multiplyFixed(row.sales_tax_rate, "100", {
                                leftMaxScale: 6,
                                rightMaxScale: 0,
                                outputScale: 2,
                            })}
                            precision={2}
                        />
                    ),
                },
                {
                    id: "tax",
                    header: "税额",
                    align: "end",
                    cell: (row) => (
                        <MoneyValue display="exact" value={row.tax_amount} />
                    ),
                },
                {
                    id: "gross",
                    header: "含税金额",
                    align: "end",
                    cell: (row) => (
                        <div>
                            <MoneyValue
                                display="exact"
                                value={row.gross_amount}
                            />
                            <p className="mt-1 text-xs text-muted-foreground">
                                不含税{" "}
                                <MoneyValue
                                    display="exact"
                                    value={row.net_amount}
                                />
                            </p>
                        </div>
                    ),
                },
            ]}
            rows={lines}
            getRowId={(row) => row.id}
            totals={[
                {
                    id: "net",
                    label: "不含税金额",
                    value: (
                        <MoneyValue
                            display="exact"
                            value={submission.net_amount}
                        />
                    ),
                },
                {
                    id: "tax",
                    label: "税额",
                    value: (
                        <MoneyValue
                            display="exact"
                            value={submission.tax_amount}
                        />
                    ),
                },
                {
                    id: "gross",
                    label: "含税金额",
                    value: (
                        <MoneyValue
                            display="exact"
                            value={submission.gross_amount}
                        />
                    ),
                    emphasized: true,
                },
            ]}
            remarks={submission.business_remark || undefined}
            appendix={<DeliveryAllocationPrintAppendix lines={deliveryLines} />}
            footer={
                isSubmission
                    ? "本预览保留此次审批提交的销售内容及全部明细。"
                    : "本预览展示本次采购审批提交时的销售单内容及全部明细。"
            }
        />
    )
}
