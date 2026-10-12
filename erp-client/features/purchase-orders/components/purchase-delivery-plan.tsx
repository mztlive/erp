"use client"

import {
    Table,
    TableHeader,
    TableBody,
    TableRow,
    TableHead,
    TableCell,
} from "@/components/ui/table"
import { QuantityValue } from "@/components/business"
import type { PurchaseDeliveryAllocation } from "../types"

/** 对外单据只使用本采购行已经确认的交付安排。 */
export function PurchaseDeliveryPlan({
    lines,
    warehouse,
    warehouseName,
}: {
    lines: readonly {
        itemName: string
        itemSku?: string
        unit?: string
        quantity?: string
        deliveryAllocations?: readonly PurchaseDeliveryAllocation[]
    }[]
    warehouse: boolean
    warehouseName?: string
}) {
    return (
        <section aria-label="交付安排" className="mt-8 space-y-3 text-sm">
            <h3 className="font-semibold">交付安排</h3>
            {warehouse ? (
                <p>
                    采购入库目标仓：{warehouseName || "以采购入库安排为准"}
                    。客户配送由仓库另行安排。
                </p>
            ) : (
                <Table className="w-full table-fixed border-collapse text-xs">
                    <TableHeader>
                        <TableRow className="bg-muted/40 text-left">
                            <TableHead className="w-[24%] border border-border p-2 font-medium">
                                商品 / 规格
                            </TableHead>
                            <TableHead className="w-[24%] border border-border p-2 font-medium">
                                收货人 / 电话
                            </TableHead>
                            <TableHead className="border border-border p-2 font-medium">
                                完整地址
                            </TableHead>
                            <TableHead className="w-[14%] border border-border p-2 text-right font-medium">
                                数量
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {lines
                            .filter((line) => line.quantity)
                            .flatMap((line, lineIndex) =>
                                line.deliveryAllocations?.length
                                    ? line.deliveryAllocations.map(
                                          (row, index) => (
                                              <TableRow
                                                  key={`${lineIndex}-${index}`}
                                                  className="break-inside-avoid"
                                              >
                                                  <TableCell className="whitespace-normal break-words border border-border p-2 align-top">
                                                      {line.itemName}
                                                      {line.itemSku ? (
                                                          <div className="mt-1 text-muted-foreground">
                                                              {line.itemSku}
                                                          </div>
                                                      ) : null}
                                                  </TableCell>
                                                  <TableCell className="whitespace-normal break-words border border-border p-2 align-top">
                                                      {row.recipient}
                                                      <div className="mt-1 num">
                                                          {row.phone}
                                                      </div>
                                                  </TableCell>
                                                  <TableCell className="whitespace-normal break-words border border-border p-2 align-top">
                                                      {row.address}
                                                  </TableCell>
                                                  <TableCell className="border border-border p-2 text-right align-top">
                                                      <QuantityValue
                                                          value={row.quantity}
                                                          unit={line.unit ?? ""}
                                                      />
                                                  </TableCell>
                                              </TableRow>
                                          ),
                                      )
                                    : [
                                          <TableRow key={lineIndex}>
                                              <TableCell className="border border-border p-2">
                                                  {line.itemName}
                                              </TableCell>
                                              <TableCell
                                                  colSpan={3}
                                                  className="whitespace-normal border border-border p-2 text-muted-foreground"
                                              >
                                                  未指定交付地址，履约前补齐
                                              </TableCell>
                                          </TableRow>,
                                      ],
                            )}
                    </TableBody>
                </Table>
            )}
        </section>
    )
}
