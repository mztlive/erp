"use client"

import { MoneyValue, PaperDocument, QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  NATURE_LABEL,
  OWNER_LABEL,
} from "@/mock/sales-orders"
import type {
  SalesOrderLineItem,
  SalesOrderListItem,
} from "@/features/sales-orders/types"

type SalesOrderPaperDialogProps = {
  order: SalesOrderListItem | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * 正式单据纸质投影：宽对话框承载 PaperDocument，供阅读与打印，不塞进窄侧栏。
 */
export function SalesOrderPaperDialog({
  order,
  open,
  onOpenChange,
}: SalesOrderPaperDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex max-h-[92vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
          <DialogTitle>纸质单据预览</DialogTitle>
          <DialogDescription>
            系统正式数据的打印件。金额与状态均由服务端确认后传入；组件不重新计算。
          </DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto bg-surface-sunken px-3 py-4 sm:px-6">
          {order ? <SalesOrderPaperDocument order={order} /> : null}
        </div>

        <DialogFooter className="shrink-0 border-t border-border px-6 py-4 sm:justify-between">
          <p className="text-xs text-muted-foreground">
            {order
              ? `${order.documentNumber} · ${NATURE_LABEL[order.nature]} · ${OWNER_LABEL[order.ownerSystem]}`
              : null}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              关闭
            </Button>
            <Button
              type="button"
              onClick={() => {
                window.print()
              }}
            >
              打印
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function SalesOrderPaperDocument({ order }: { order: SalesOrderListItem }) {
  const isCard = order.nature === "card_voucher"

  return (
    <PaperDocument<SalesOrderLineItem>
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
          reference: "内部主体",
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
          id: "owner-system",
          label: "主责系统",
          value: OWNER_LABEL[order.ownerSystem],
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
          ? "卡券履约在福利商城执行；本单据仅展示 ERP 商业数据。"
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
