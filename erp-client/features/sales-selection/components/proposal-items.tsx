"use client"

import { Fragment } from "react"

import {
    BusinessEmptyState,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { SnapshotImage } from "@/features/sales-selection/components/snapshot-image"
import type { ProposalView } from "@/features/sales-selection/types"

type SkuLine = ProposalView["sku_lines"][number]

function ProductIdentity({
    name,
    specification,
}: Pick<SkuLine, "name" | "specification">) {
    const description = specification
        ?.map((attr) => `${attr.name}：${attr.value}`)
        .join(" · ")
    return (
        <div className="min-w-0 space-y-1 whitespace-normal break-words">
            <p className="font-medium text-foreground">{name}</p>
            {description ? (
                <p className="text-xs leading-relaxed text-muted-foreground">
                    {description}
                </p>
            ) : null}
        </div>
    )
}

function LineValues({
    line,
    unit,
}: {
    line: {
        unit_price: string
        quantity?: number | null
        line_amount?: string | null
    }
    unit?: string
}) {
    return (
        <>
            <TableCell data-align="end">
                <MoneyValue display="exact" value={line.unit_price} />
            </TableCell>
            <TableCell data-align="end">
                {line.quantity != null ? (
                    <QuantityValue value={String(line.quantity)} unit={unit} />
                ) : (
                    "—"
                )}
            </TableCell>
            <TableCell data-align="end">
                <MoneyValue
                    value={line.line_amount}
                    className="font-semibold"
                />
            </TableCell>
        </>
    )
}

export function ProposalItems({ proposal }: { proposal: ProposalView }) {
    const isPackage = proposal.form === "PACKAGE"
    const title = isPackage ? "套餐清单" : "商品清单"
    const linesByItem = new Map<string, SkuLine[]>()
    for (const line of proposal.sku_lines) {
        const group = linesByItem.get(line.display_item_id) ?? []
        group.push(line)
        linesByItem.set(line.display_item_id, group)
    }

    return (
        <DetailWorkspaceSection
            headingId="selection-proposal-items-title"
            title={title}
            subtitle={
                <span className="num">{proposal.display_lines.length} 款</span>
            }
        >
            {proposal.display_lines.length === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无确认明细"
                    description="该方案尚无可展示的确认明细。"
                />
            ) : (
                <div
                    data-slot="detail-inline-table"
                    className={detailWorkspaceStyles.inlineTableWrapper}
                >
                    <Table
                        data-density="comfortable"
                        className="min-w-[560px] table-fixed"
                    >
                        <TableHeader>
                            <TableRow>
                                <TableHead>
                                    {isPackage ? "套餐 / 内含商品" : "商品"}
                                </TableHead>
                                <TableHead data-align="end" className="w-36">
                                    单价
                                </TableHead>
                                <TableHead data-align="end" className="w-24">
                                    数量
                                </TableHead>
                                <TableHead data-align="end" className="w-36">
                                    小计
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {proposal.display_lines.map((line, index) => {
                                const members =
                                    linesByItem.get(line.display_item_id) ?? []
                                const product = members[0]
                                const name = isPackage
                                    ? `套餐 ${index + 1}`
                                    : (product?.name ?? "商品")
                                const imagePath = line.cover_asset_id
                                    ? `/admin/sales-selection-books/${encodeURIComponent(proposal.booklet_id)}/images?ref=${encodeURIComponent(line.cover_asset_id)}`
                                    : undefined

                                return (
                                    <Fragment key={line.display_item_id}>
                                        <TableRow>
                                            <TableCell>
                                                <div className="flex items-center gap-4">
                                                    <div className="size-20 shrink-0 overflow-hidden rounded-lg bg-muted/50 text-center whitespace-normal">
                                                        {imagePath ? (
                                                            <SnapshotImage
                                                                path={imagePath}
                                                                alt={name}
                                                            />
                                                        ) : (
                                                            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
                                                                暂无图片
                                                            </div>
                                                        )}
                                                    </div>
                                                    <div className="min-w-0">
                                                        <ProductIdentity
                                                            name={name}
                                                            specification={
                                                                isPackage
                                                                    ? undefined
                                                                    : product?.specification
                                                            }
                                                        />
                                                        {isPackage ? (
                                                            <p className="mt-1 text-xs text-muted-foreground">
                                                                含{" "}
                                                                <span className="num">
                                                                    {
                                                                        members.length
                                                                    }
                                                                </span>{" "}
                                                                款商品
                                                            </p>
                                                        ) : null}
                                                    </div>
                                                </div>
                                            </TableCell>
                                            <LineValues
                                                line={line}
                                                unit={
                                                    isPackage
                                                        ? "份"
                                                        : product?.unit
                                                }
                                            />
                                        </TableRow>
                                        {isPackage
                                            ? members.map(
                                                  (member, memberIndex) => (
                                                      <TableRow
                                                          key={`${line.display_item_id}-${memberIndex}`}
                                                          className="bg-muted/20"
                                                      >
                                                          <TableCell>
                                                              <div className="pl-6">
                                                                  <ProductIdentity
                                                                      name={
                                                                          member.name
                                                                      }
                                                                      specification={
                                                                          member.specification
                                                                      }
                                                                  />
                                                              </div>
                                                          </TableCell>
                                                          <LineValues
                                                              line={member}
                                                              unit={member.unit}
                                                          />
                                                      </TableRow>
                                                  ),
                                              )
                                            : null}
                                    </Fragment>
                                )
                            })}
                        </TableBody>
                    </Table>
                </div>
            )}
        </DetailWorkspaceSection>
    )
}
