"use client"

import * as React from "react"
import { MoneyValue } from "@/components/business"
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
import type { ContractCenterView, ContractListRow } from "@/features/contracts/types"
import { cn } from "@/lib/utils"

type ContractPreviewPanelProps = {
  row: ContractListRow
  detail: ContractCenterView | null | undefined
  detailLoading?: boolean
}

/**
 * detail 半屏：左右分栏读完客户、结算/开票、有效期、附件与关联销售摘要。
 * 不负责编辑、版本 diff 或纸质投影。
 */
export function ContractPreviewPanel({
  row,
  detail,
  detailLoading,
}: ContractPreviewPanelProps) {
  const rev = detail?.currentRevision
  const payment = rev?.paymentTermSnapshot
  const invoice = rev?.invoiceRequirementSnapshot

  return (
    <div
      data-slot="contract-detail-preview"
      className="flex min-h-0 flex-1 flex-col lg:flex-row"
    >
      <ScrollArea className="min-h-0 max-h-[40vh] border-b border-border lg:max-h-none lg:w-[min(20rem,38%)] lg:shrink-0 lg:border-r lg:border-b-0">
        <div className="space-y-4 p-4 md:p-5">
          <section className="space-y-2" aria-label="签订与有效期">
            <SectionTitle>签订 / 有效期</SectionTitle>
            <DescriptionList columns="one" className="gap-y-2.5">
              <CompactField
                label="签订日"
                value={row.signedAt ?? rev?.signedAt ?? "—"}
                numeric
              />
              <CompactField
                label="有效期"
                value={
                  <span className="num">
                    {row.validFrom} ~ {row.validTo}
                    {row.expiringWithin30Days ? (
                      <span className="mt-0.5 block text-xs font-normal text-warning-foreground">
                        30 日内将到期
                      </span>
                    ) : null}
                  </span>
                }
              />
              <CompactField
                label="当前版本"
                value={`v${row.revisionNo}`}
                numeric
              />
            </DescriptionList>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="客户与负责人">
            <SectionTitle>客户与负责人</SectionTitle>
            <DescriptionList columns="one" className="gap-y-2.5">
              <CompactField label="客户" value={row.customer.displayName} />
              <CompactField
                label="客户编号"
                value={row.customer.customerNo}
                numeric
              />
              <CompactField label="负责人" value={row.ownerLabel} />
            </DescriptionList>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="附件摘要">
            <SectionTitle>附件</SectionTitle>
            {detailLoading ? (
              <p className="text-xs text-muted-foreground">附件加载中…</p>
            ) : detail ? (
              detail.attachments.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无附件</p>
              ) : (
                <ul className="space-y-1.5 text-sm">
                  {detail.attachments.map((file) => (
                    <li
                      key={file.id}
                      className="flex items-start justify-between gap-2"
                    >
                      <span className="min-w-0 truncate">{file.name}</span>
                      <Badge
                        variant={
                          file.securityState === "done"
                            ? "secondary"
                            : file.securityState === "quarantined"
                              ? "destructive"
                              : "outline"
                        }
                        className="shrink-0"
                      >
                        {file.securityState === "done"
                          ? "已通过"
                          : file.securityState === "quarantined"
                            ? "已隔离"
                            : "检查中"}
                      </Badge>
                    </li>
                  ))}
                </ul>
              )
            ) : (
              <p className="text-xs text-muted-foreground">附件分区暂不可用</p>
            )}
            <p className="text-[11px] text-muted-foreground">
              附件数 {detail?.attachments.length ?? "—"}
              {rev ? ` · 版本 v${rev.revisionNo}` : null}
            </p>
          </section>
        </div>
      </ScrollArea>

      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-4 p-4 md:p-5">
          <section className="space-y-2" aria-label="结算与开票">
            <SectionTitle>结算主体与付款 / 开票</SectionTitle>
            {detailLoading && !detail ? (
              <p className="text-sm text-muted-foreground">加载条款摘要…</p>
            ) : (
              <DescriptionList columns="one" className="gap-y-2.5 sm:max-w-xl">
                <CompactField
                  label="结算主体"
                  value={
                    rev?.settlementParty.displayName ??
                    row.settlementParty.displayName
                  }
                />
                <CompactField
                  label="付款条件"
                  value={payment?.label ?? "—"}
                />
                <CompactField
                  label="付款说明"
                  value={payment?.description ?? "—"}
                />
                <CompactField
                  label="开票要求"
                  value={
                    invoice
                      ? `${invoice.titleType} · ${invoice.contentSummary}`
                      : "—"
                  }
                />
                {invoice?.remark ? (
                  <CompactField label="开票备注" value={invoice.remark} />
                ) : null}
              </DescriptionList>
            )}
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              本页不汇总合同金额；金额以各销售单为准。
            </p>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="关联销售单">
            <div className="flex items-center justify-between gap-2">
              <SectionTitle>关联销售单与业务进度</SectionTitle>
              <span className="text-xs text-muted-foreground">
                共 {row.salesOrderCount} 张 · 进行中 {row.activeSalesOrderCount}
              </span>
            </div>
            {detailLoading && !detail ? (
              <p className="text-sm text-muted-foreground">加载关联单据…</p>
            ) : detail && detail.relatedSalesOrders.length > 0 ? (
              <div className="overflow-hidden rounded-lg border border-border">
                <Table data-density="compact">
                  <TableHeader>
                    <TableRow>
                      <TableHead>销售单</TableHead>
                      <TableHead className="hidden sm:table-cell">性质</TableHead>
                      <TableHead>状态</TableHead>
                      <TableHead className="hidden md:table-cell">合同版本</TableHead>
                      <TableHead data-align="end">含税金额</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {detail.relatedSalesOrders.map((so) => (
                      <TableRow key={so.salesOrderId}>
                        <TableCell>
                          <div className="num font-medium">
                            {so.documentNumber}
                          </div>
                          <div className="text-[11px] text-muted-foreground">
                            履约 {so.fulfillmentLabel} · 回款{" "}
                            {so.collectionLabel} · 开票 {so.invoicingLabel}
                          </div>
                        </TableCell>
                        <TableCell className="hidden sm:table-cell">
                          {so.natureLabel}
                        </TableCell>
                        <TableCell>{so.primaryStatus.label}</TableCell>
                        <TableCell className="num hidden md:table-cell">
                          v{so.contractRevisionNo}
                        </TableCell>
                        <TableCell data-align="end">
                          <MoneyValue
                            value={so.amountGross}
                            taxBasis="gross"
                          />
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">
                当前合同尚无关联销售单。
              </p>
            )}
            {detail?.relatedSalesOrdersAsOf ? (
              <p className="text-[11px] text-muted-foreground">
                关联销售统计截至{" "}
                <span className="num">{detail.relatedSalesOrdersAsOf}</span>
                。
              </p>
            ) : null}
          </section>
        </div>
      </ScrollArea>
    </div>
  )
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
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
    <DescriptionItem>
      <DescriptionTerm>{label}</DescriptionTerm>
      <DescriptionDetails className={cn(numeric && "num")}>
        {value}
      </DescriptionDetails>
    </DescriptionItem>
  )
}
