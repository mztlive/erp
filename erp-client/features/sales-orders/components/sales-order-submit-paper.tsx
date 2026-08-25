"use client"

import { MoneyValue, PaperDocument, QuantityValue } from "@/components/business"
import { paymentTermLabel, welfareScenarioLabel } from "@/lib/business-options"
import { NATURE_LABEL } from "@/features/sales-orders/lib/labels"
import { deriveVoucherGiftPreview } from "@/features/sales-orders/lib/sales-order-create-model"
import type {
    SalesOrderSubmitLineSnapshot,
    SalesOrderSubmitSnapshot,
} from "@/features/sales-orders/components/sales-order-submit-confirm-summary"

function giftRateLabel(line: SalesOrderSubmitLineSnapshot): string {
    if (line.giftRate.trim()) return `${line.giftRate}%`
    const preview = deriveVoucherGiftPreview(
        line.faceValue,
        line.unitPriceGross,
        line.quantity,
    )
    return preview ? `${preview.giftRatePercent}%` : "—"
}

/**
 * 提交确认用的销售单纸质稿，数据来自当前建单表单，不是已落库单据。
 */
export function SalesOrderSubmitPaper({
    snapshot,
}: {
    snapshot: SalesOrderSubmitSnapshot
}) {
    const isCard = snapshot.nature === "card_voucher"
    const paymentLabel =
        paymentTermLabel(snapshot.paymentTerms) || snapshot.paymentTerms || "—"

    return (
        <PaperDocument<SalesOrderSubmitLineSnapshot>
            frame="bare"
            title="销售单"
            subtitle={NATURE_LABEL[snapshot.nature]}
            documentNumber="提交预览"
            status={{ label: "草稿", tone: "neutral" }}
            version="尚未生效"
            parties={[
                {
                    id: "seller",
                    label: "销售方",
                    name: snapshot.ownerName || "—",
                    fields: [
                        {
                            id: "owner",
                            label: "业务负责人",
                            value: snapshot.ownerName || "—",
                        },
                        {
                            id: "tax",
                            label: "销项税率",
                            value: snapshot.taxRatePercent
                                ? `${snapshot.taxRatePercent}%`
                                : "—",
                            numeric: true,
                        },
                    ],
                },
                {
                    id: "buyer",
                    label: "客户",
                    name: snapshot.customerName || "—",
                    reference: snapshot.contractLabel || "无合同",
                    fields: [
                        {
                            id: "settlement",
                            label: "结算主体",
                            value: snapshot.settlementEntity || "—",
                        },
                        {
                            id: "contract",
                            label: "合同",
                            value: snapshot.contractLabel || "无合同",
                        },
                    ],
                },
            ]}
            metadata={[
                {
                    id: "payment",
                    label: "付款条件",
                    value: paymentLabel,
                },
                {
                    id: "deadline",
                    label: isCard ? "卡券履约期限" : "履约期限摘要",
                    value: snapshot.fulfillmentDeadline || "—",
                    numeric: true,
                },
                {
                    id: "scene",
                    label: "福利场景",
                    value: welfareScenarioLabel(snapshot.welfareScene) || "—",
                },
                {
                    id: "origin",
                    label: isCard ? "目标商城" : "履约方式",
                    value: isCard
                        ? snapshot.targetMallId || "—"
                        : snapshot.fulfillmentMode || "—",
                },
            ]}
            lineItemLabel={isCard ? "卡券明细（唯一）" : "销售明细"}
            columns={
                isCard
                    ? [
                          {
                              id: "name",
                              header: "卡券类目",
                              cell: (row) => (
                                  <div>
                                      <div>{row.name || "—"}</div>
                                      {row.sku ? (
                                          <div className="num mt-1 text-xs text-muted-foreground">
                                              {row.sku}
                                          </div>
                                      ) : null}
                                  </div>
                              ),
                          },
                          {
                              id: "face",
                              header: "面额",
                              align: "end",
                              numeric: true,
                              cell: (row) =>
                                  row.faceValue ? (
                                      <MoneyValue value={row.faceValue} />
                                  ) : (
                                      "—"
                                  ),
                          },
                          {
                              id: "qty",
                              header: "数量",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <QuantityValue
                                      value={row.quantity}
                                      unit={row.unit}
                                  />
                              ),
                          },
                          {
                              id: "form",
                              header: "形态",
                              cell: (row) => row.cardForm || "—",
                          },
                          {
                              id: "gift",
                              header: "配赠率",
                              align: "end",
                              numeric: true,
                              cell: (row) => giftRateLabel(row),
                          },
                          {
                              id: "amount",
                              header: "成交金额（含税）",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <MoneyValue value={row.amountGross} />
                              ),
                          },
                      ]
                    : [
                          {
                              id: "name",
                              header: "项目",
                              cell: (row) => (
                                  <div>
                                      <div>{row.name || "—"}</div>
                                      {row.sku ? (
                                          <div className="num mt-1 text-xs text-muted-foreground">
                                              {row.sku}
                                          </div>
                                      ) : null}
                                  </div>
                              ),
                          },
                          {
                              id: "mode",
                              header: "履约方式",
                              cell: (row) => row.fulfillmentMode || "—",
                          },
                          {
                              id: "due",
                              header: "履约期限",
                              numeric: true,
                              cell: (row) => row.dueDate || "—",
                          },
                          {
                              id: "qty",
                              header: "数量",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <QuantityValue
                                      value={row.quantity}
                                      unit={row.unit}
                                  />
                              ),
                          },
                          {
                              id: "price",
                              header: "单价（含税）",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <MoneyValue value={row.unitPriceGross} />
                              ),
                          },
                          {
                              id: "amount",
                              header: "小计（含税）",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <MoneyValue value={row.amountGross} />
                              ),
                          },
                      ]
            }
            rows={snapshot.lineItems}
            getRowId={(row) => row.rowKey}
            totals={[
                {
                    id: "net",
                    label: "不含税金额",
                    value: <MoneyValue value={snapshot.amountNet} />,
                },
                {
                    id: "tax",
                    label: "税额",
                    value: <MoneyValue value={snapshot.amountTax} />,
                },
                {
                    id: "gross",
                    label: "成交金额（含税）",
                    value: <MoneyValue value={snapshot.amountGross} />,
                    emphasized: true,
                },
            ]}
            remarks={
                snapshot.remark.trim() ||
                (isCard
                    ? "卡券履约在福利商城执行；本预览仅展示当前填写的销售数据。"
                    : "本预览按当前填写内容生成，确认提交后进入审批。")
            }
        />
    )
}
