"use client"

import { MoneyValue, PaperDocument, QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  NATURE_LABEL,
  ORIGIN_LABEL,
} from "@/mock/sales-orders"
import type {
  SalesOrderLineItem,
  SalesOrderListItem,
} from "@/features/sales-orders/types"
import { XIcon } from "lucide-react"

type SalesOrderPaperDialogProps = {
  order: SalesOrderListItem | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * 列表行纸质预览：透明壳 + PaperDocument，弱化 Dialog 边框/标题栏/页脚痕迹。
 * 点击遮罩或右上角关闭；不提供打印入口。
 */
export function SalesOrderPaperDialog({
  order,
  open,
  onOpenChange,
}: SalesOrderPaperDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="flex max-h-[min(96vh,56rem)] w-full max-w-[calc(100%-1.5rem)] flex-col gap-0 overflow-hidden border-0 bg-transparent p-0 shadow-none ring-0 sm:max-w-5xl dark:ring-0"
      >
        <DialogTitle className="sr-only">
          {order
            ? `销售单 ${order.documentNumber} 纸质预览`
            : "销售单纸质预览"}
        </DialogTitle>
        <DialogDescription className="sr-only">
          系统业务数据的打印件；金额与状态以系统记录为准。按 Esc 或点击遮罩关闭。
        </DialogDescription>

        <div className="relative min-h-0 flex-1">
          <DialogClose
            render={
              <Button
                type="button"
                variant="secondary"
                size="icon-sm"
                className="absolute top-3 right-3 z-10 rounded-full border border-border/60 bg-card/95 shadow-md backdrop-blur-sm print:hidden"
              />
            }
          >
            <XIcon aria-hidden="true" />
            <span className="sr-only">关闭预览</span>
          </DialogClose>

          <div className="max-h-[min(96vh,56rem)] overflow-y-auto overscroll-contain">
            {order ? <SalesOrderPaperDocument order={order} /> : null}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function SalesOrderPaperDocument({ order }: { order: SalesOrderListItem }) {
  const isCard = order.nature === "card_voucher"

  return (
    <PaperDocument<SalesOrderLineItem>
      frame="bare"
      issuer={order.sellerEntity}
      title="销售单"
      subtitle={NATURE_LABEL[order.nature]}
      documentNumber={order.documentNumber}
      status={order.primaryStatus}
      version={order.version}
      parties={[
        {
          id: "seller",
          label: "销售方",
          name: order.sellerEntity,
          reference: "结算主体",
          fields: [
            { id: "owner", label: "业务负责人", value: order.ownerName },
            {
              id: "submitted",
              label: "提交时间",
              value: order.submittedAt,
              numeric: true,
            },
          ],
        },
        {
          id: "buyer",
          label: "客户",
          name: order.customerName,
          reference: order.contractNumber,
          fields: [
            {
              id: "settlement",
              label: "结算主体",
              value: order.settlementEntity,
            },
            {
              id: "contact",
              label: "联系人",
              value: order.customerContact ?? "—",
            },
          ],
        },
      ]}
      metadata={[
        {
          id: "payment",
          label: "付款条件",
          value: order.paymentTerms,
        },
        {
          id: "deadline",
          label: isCard ? "卡券履约期限" : "履约期限摘要",
          value: order.fulfillmentDeadline,
          numeric: true,
        },
        {
          id: "scene",
          label: "福利场景",
          value: order.welfareScene,
        },
        {
          id: "origin-system",
          label: "来源",
          value: ORIGIN_LABEL[order.originSystem],
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
                    <div>{row.name}</div>
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
                  <QuantityValue value={row.quantity} unit={row.unit} />
                ),
              },
              {
                id: "form",
                header: "形态",
                cell: (row) => row.cardForm ?? "—",
              },
              {
                id: "gift",
                header: "配赠率",
                align: "end",
                numeric: true,
                cell: (row) =>
                  row.giftRate != null ? `${row.giftRate}%` : "—",
              },
              {
                id: "amount",
                header: "成交金额（含税）",
                align: "end",
                numeric: true,
                cell: (row) => <MoneyValue value={row.amountGross} />,
              },
            ]
          : [
              {
                id: "name",
                header: "项目",
                cell: (row) => (
                  <div>
                    <div>{row.name}</div>
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
                cell: (row) => row.fulfillmentMode ?? "—",
              },
              {
                id: "due",
                header: "履约期限",
                numeric: true,
                cell: (row) => row.dueDate ?? "—",
              },
              {
                id: "qty",
                header: "数量",
                align: "end",
                numeric: true,
                cell: (row) => (
                  <QuantityValue value={row.quantity} unit={row.unit} />
                ),
              },
              {
                id: "price",
                header: "单价（含税）",
                align: "end",
                numeric: true,
                cell: (row) => <MoneyValue value={row.unitPriceGross} />,
              },
              {
                id: "amount",
                header: "小计（含税）",
                align: "end",
                numeric: true,
                cell: (row) => <MoneyValue value={row.amountGross} />,
              },
            ]
      }
      rows={order.lineItems}
      getRowId={(row) => row.id}
      totals={[
        {
          id: "net",
          label: "不含税金额",
          value: <MoneyValue value={order.amountNet} />,
        },
        {
          id: "tax",
          label: "税额",
          value: <MoneyValue value={order.taxAmount} />,
        },
        {
          id: "gross",
          label: "成交金额（含税）",
          value: <MoneyValue value={order.amountGross} />,
          emphasized: true,
        },
        {
          id: "received",
          label: "已回款（含税）",
          value: <MoneyValue value={order.receivedAmount} />,
          description: `回款进度：${order.collection.label}`,
        },
        {
          id: "invoiced",
          label: "已开票",
          value: <MoneyValue value={order.invoicedAmount} />,
          description: `开票进度：${order.invoicing.label}`,
        },
      ]}
      remarks={
        order.remark ??
        (isCard
          ? "卡券履约在福利商城执行；本单据仅展示系统内的销售数据。"
          : undefined)
      }
      signature={
        <div className="space-y-8 text-sm">
          <div>
            <div className="text-muted-foreground">业务负责人</div>
            <div className="mt-6 border-b border-dashed border-border pb-1">
              {order.ownerName}
            </div>
          </div>
          <div>
            <div className="text-muted-foreground">日期</div>
            <div className="num mt-6 border-b border-dashed border-border pb-1">
              {order.submittedAt.slice(0, 10)}
            </div>
          </div>
        </div>
      }
      seal={
        <div className="flex h-28 w-28 items-center justify-center rounded-full border-2 border-dashed border-muted-foreground/40 text-center text-xs text-muted-foreground">
          公司签章
          <br />
          签章位
        </div>
      }
    />
  )
}
