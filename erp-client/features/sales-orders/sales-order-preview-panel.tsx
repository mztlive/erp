"use client"

import {
  DocumentTotals,
  MoneyValue,
  QuantityValue,
  RateValue,
  StatusTrackSummary,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
} from "@/components/ui/description-list"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  NATURE_LABEL,
  ORIGIN_LABEL,
} from "@/mock/sales-orders"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { cn } from "@/lib/utils"
import { sumFixed } from "@/lib/fixed-decimal"

type SalesOrderPreviewPanelProps = {
  order: SalesOrderListItem
}

/**
 * 半屏详情预览（方案 A）：左右分栏，各自滚动，减少整页长卷。
 * 左：进度、商务、关联；右：明细、金额汇总。
 */
export function SalesOrderPreviewPanel({ order }: SalesOrderPreviewPanelProps) {
  const isCard = order.nature === "card_voucher"
  const receivableRemaining = formatRemaining(
    order.amountGross,
    order.receivedAmount
  )

  return (
    <div
      data-slot="sales-order-detail-preview"
      className="flex min-h-0 flex-1 flex-col lg:flex-row"
    >
      {/* 左栏：上下文 */}
      <ScrollArea className="min-h-0 max-h-[40vh] border-b border-border lg:max-h-none lg:w-[min(20rem,38%)] lg:shrink-0 lg:border-r lg:border-b-0">
        <div className="space-y-4 p-4 md:p-5">
          <section className="space-y-2" aria-label="进度">
            <SectionTitle>业务进度</SectionTitle>
            <StatusTrackSummary
              variant="table"
              className="sm:grid-cols-1 lg:grid-cols-1"
              tracks={[
                {
                  id: "fulfillment",
                  label: "履约",
                  status: order.fulfillment,
                },
                {
                  id: "collection",
                  label: "回款",
                  status: order.collection,
                },
                {
                  id: "invoicing",
                  label: "开票",
                  status: order.invoicing,
                },
              ]}
            />
          </section>

          <Separator />

          <section className="space-y-2" aria-label="表头信息">
            <SectionTitle>商务信息</SectionTitle>
            <div className="mb-2 flex flex-wrap gap-1.5">
              <Badge variant="secondary">{NATURE_LABEL[order.nature]}</Badge>
              <Badge variant="outline">{ORIGIN_LABEL[order.originSystem]}</Badge>
            </div>
            <DescriptionList columns="one" className="gap-y-2.5">
              <CompactField
                label="合同修订"
                value={order.contractRevisionLabel}
                numeric
              />
              <CompactField label="结算主体" value={order.settlementEntity} />
              <CompactField label="付款条件" value={order.paymentTerms} />
              <CompactField
                label="履约期限"
                value={order.fulfillmentDeadline}
                numeric
              />
              <CompactField label="福利场景" value={order.welfareScene} />
              <CompactField
                label="负责人"
                value={
                  <>
                    {order.ownerName}
                    {order.customerContact ? (
                      <span className="mt-0.5 block text-xs font-normal text-muted-foreground">
                        {order.customerContact}
                      </span>
                    ) : null}
                  </>
                }
              />
              <CompactField
                label="提交时间"
                value={order.submittedAt}
                numeric
              />
              {order.remark ? (
                <CompactField label="备注" value={order.remark} />
              ) : null}
            </DescriptionList>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="关联单据">
            <SectionTitle>关联进度</SectionTitle>
            <div className="flex flex-wrap gap-1.5">
              <RelatedPill
                label="采购"
                count={order.related.purchaseOrders}
                muted={isCard}
              />
              <RelatedPill
                label="履约"
                count={order.related.fulfillments}
                muted={isCard}
              />
              <RelatedPill label="回款" count={order.related.receipts} />
              <RelatedPill label="发票" count={order.related.invoices} />
            </div>
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              {isCard
                ? "卡券不在采购环节履约；回款与开票在详情页处理。不展示玩法、卡号与卡密。"
                : "履约登记、票款核销、变更请在详情页处理；已生效单无直接编辑或人工关闭。"}
            </p>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="关闭条件摘要">
            <SectionTitle>关闭条件</SectionTitle>
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              {order.closeEligibility.note}
            </p>
            <div className="flex flex-wrap gap-1.5">
              <Badge
                variant={
                  order.closeEligibility.fulfillmentComplete
                    ? "success"
                    : "secondary"
                }
              >
                履约
                {order.closeEligibility.fulfillmentComplete ? "已完成" : "未完成"}
              </Badge>
              <Badge
                variant={
                  order.closeEligibility.receivableSettled
                    ? "success"
                    : "secondary"
                }
              >
                应收
                {order.closeEligibility.receivableSettled ? "已结清" : "未结清"}
              </Badge>
              <Badge variant="outline">开票不阻塞</Badge>
            </div>
          </section>
        </div>
      </ScrollArea>

      {/* 右栏：明细 + 汇总（主阅读区） */}
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-4 p-4 md:p-5">
          <section className="space-y-2" aria-label="销售明细">
            <div className="flex items-center justify-between gap-2">
              <SectionTitle>销售明细</SectionTitle>
              <span className="text-xs text-muted-foreground">
                {order.lineItems.length} 行
                {isCard ? " · 唯一卡券明细" : null}
              </span>
            </div>
            <div className="overflow-hidden rounded-lg border border-border">
              <Table data-density="compact">
                <TableHeader>
                  <TableRow>
                    <TableHead>项目</TableHead>
                    {!isCard ? (
                      <TableHead className="hidden xl:table-cell">
                        履约
                      </TableHead>
                    ) : (
                      <TableHead className="hidden md:table-cell">
                        面额 / 形态
                      </TableHead>
                    )}
                    <TableHead data-align="end">数量</TableHead>
                    <TableHead
                      data-align="end"
                      className="hidden sm:table-cell"
                    >
                      单价
                    </TableHead>
                    <TableHead data-align="end">小计</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {order.lineItems.map((line) => (
                    <TableRow key={line.id}>
                      <TableCell className="max-w-[14rem] whitespace-normal">
                        <div className="font-medium text-foreground">
                          {line.name}
                        </div>
                        {line.sku ? (
                          <div className="num mt-0.5 text-xs text-muted-foreground">
                            {line.sku}
                          </div>
                        ) : null}
                        {isCard && line.giftRate != null ? (
                          <div className="mt-0.5 text-xs text-muted-foreground">
                            配赠率{" "}
                            <RateValue value={line.giftRate} precision={2} />
                          </div>
                        ) : null}
                      </TableCell>
                      {!isCard ? (
                        <TableCell className="hidden whitespace-normal text-xs text-muted-foreground xl:table-cell">
                          <div>{line.fulfillmentMode ?? "—"}</div>
                          {line.dueDate ? (
                            <div className="num mt-0.5">{line.dueDate}</div>
                          ) : null}
                        </TableCell>
                      ) : (
                        <TableCell className="hidden whitespace-normal text-xs md:table-cell">
                          {line.faceValue ? (
                            <div>
                              <MoneyValue value={line.faceValue} />
                            </div>
                          ) : (
                            "—"
                          )}
                          {line.cardForm ? (
                            <div className="mt-0.5 text-muted-foreground">
                              {line.cardForm}
                            </div>
                          ) : null}
                        </TableCell>
                      )}
                      <TableCell data-align="end">
                        <QuantityValue
                          value={line.quantity}
                          unit={line.unit}
                        />
                      </TableCell>
                      <TableCell
                        data-align="end"
                        className="hidden sm:table-cell"
                      >
                        <MoneyValue value={line.unitPriceGross} />
                      </TableCell>
                      <TableCell data-align="end">
                        <MoneyValue
                          value={line.amountGross}
                          taxBasis="gross"
                        />
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </section>

          <DocumentTotals
            title="金额与票款"
            className="max-w-md self-end"
            items={[
              {
                id: "gross",
                label: "成交金额",
                value: <MoneyValue value={order.amountGross} />,
                basis: "含税",
              },
              {
                id: "net",
                label: "不含税金额",
                value: <MoneyValue value={order.amountNet} />,
                basis: "不含税",
              },
              {
                id: "tax",
                label: "税额",
                value: <MoneyValue value={order.taxAmount} />,
              },
              {
                id: "received",
                label: "已回款",
                value: <MoneyValue value={order.receivedAmount} />,
                basis: "含税",
              },
              {
                id: "receivable",
                label: "应收余额",
                value: <MoneyValue value={receivableRemaining} />,
                basis: "含税",
                warning:
                  order.collection.label === "待复核"
                    ? "票款复核未完成，余额仅供参考"
                    : undefined,
              },
              {
                id: "invoiced",
                label: "已开票",
                value: <MoneyValue value={order.invoicedAmount} />,
              },
            ]}
            warning={
              order.commercialReadOnly
                ? order.commercialReadOnlyReason ??
                  "商业字段只读；变更须走销售变更单。"
                : undefined
            }
          />
        </div>
      </ScrollArea>
    </div>
  )
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="font-heading text-sm font-semibold text-foreground">
      {children}
    </h3>
  )
}

function CompactField({
  label,
  value,
  numeric,
}: {
  label: string
  value: React.ReactNode
  numeric?: boolean
}) {
  return (
    <DescriptionItem className="gap-0.5">
      <DescriptionTerm className="text-xs">{label}</DescriptionTerm>
      <DescriptionDetails
        className={cn("text-sm font-medium", numeric && "num")}
      >
        {value}
      </DescriptionDetails>
    </DescriptionItem>
  )
}

function RelatedPill({
  label,
  count,
  muted,
}: {
  label: string
  count: number
  muted?: boolean
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs",
        muted
          ? "border-dashed border-border bg-muted/40 text-muted-foreground"
          : "border-border bg-card text-foreground"
      )}
    >
      <span className="text-muted-foreground">{label}</span>
      <span className="num font-semibold">{count}</span>
    </span>
  )
}

/** 只读展示也沿用定点金额，避免与正式口径出现浮点尾差。 */
function formatRemaining(gross: string, received: string) {
  try {
    return sumFixed([gross, `-${received}`], {
      maxScale: 2,
      outputScale: 2,
      allowNegative: true,
    })
  } catch {
    return gross
  }
}
