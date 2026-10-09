"use client"

import {
    MoneyValue,
    PaperDocument,
    QuantityValue,
    RateValue,
} from "@/components/business"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { displayBusinessText } from "@/lib/display-name"
import type {
    ApprovalSalesSubmission,
    ApprovalSalesRevision,
} from "../api/materials"
import { displayActorName, displayReadableName } from "../display"

const dateLabel = (value: number | null) =>
    value == null ? "—" : new Date(value * 1000).toLocaleDateString("zh-CN")

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
    return (
        <PaperDocument<ApprovalSalesSubmission["lines"][number]>
            frame="bare"
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
                            value: new Date(
                                (isSubmission
                                    ? submission.submitted_at
                                    : submission.effective_at) * 1000,
                            ).toLocaleString("zh-CN", { hour12: false }),
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
                            {row.service_region && (
                                <div className="mt-1 text-xs text-muted-foreground">
                                    服务区域：{row.service_region}
                                </div>
                            )}
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
                            unit={
                                displayReadableName(row.unit_snapshot) ||
                                (isCard ? "张" : undefined)
                            }
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
                                      <MoneyValue value={row.face_value} />
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
                              id: "price",
                              header: "含税单价",
                              align: "end" as const,
                              cell: (
                                  row: ApprovalSalesSubmission["lines"][number],
                              ) =>
                                  row.unit_price_gross ? (
                                      <MoneyValue
                                          value={row.unit_price_gross}
                                      />
                                  ) : (
                                      "—"
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
                    cell: (row) => <MoneyValue value={row.tax_amount} />,
                },
                {
                    id: "gross",
                    header: "含税金额",
                    align: "end",
                    cell: (row) => <MoneyValue value={row.gross_amount} />,
                },
            ]}
            rows={lines}
            getRowId={(row) => row.id}
            totals={[
                {
                    id: "net",
                    label: "不含税金额",
                    value: <MoneyValue value={submission.net_amount} />,
                },
                {
                    id: "tax",
                    label: "税额",
                    value: <MoneyValue value={submission.tax_amount} />,
                },
                {
                    id: "gross",
                    label: "含税金额",
                    value: <MoneyValue value={submission.gross_amount} />,
                    emphasized: true,
                },
            ]}
            remarks={submission.business_remark || undefined}
            footer={
                isSubmission
                    ? "本预览保留此次审批提交的销售内容及全部明细。"
                    : "本预览展示本次采购审批锁定的销售版本及全部明细。"
            }
        />
    )
}
