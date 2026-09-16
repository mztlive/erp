/**
 * 销售方案详情（只读）：表头 + 双层明细 + 合计守恒说明。
 * 不展示公开令牌、成本与内部堆栈。
 */

"use client"

import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { MoneyValue } from "@/components/business/values"
import type { SelectionProposal } from "@/features/sales-selection/types"
import {
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

/**
 * 方案详情只读展示。
 * @param proposal 销售方案
 */
export const ProposalDetail = ({
    proposal,
}: {
    proposal: SelectionProposal
}) => (
    <div className="flex flex-col gap-4">
        <Card>
            <CardHeader>
                <CardTitle className="flex flex-wrap items-center gap-2 text-base">
                    <span className="num">{proposal.proposal_no}</span>
                    <Badge variant="success">已提交</Badge>
                </CardTitle>
            </CardHeader>
            <CardContent>
                <dl className="grid grid-cols-1 gap-x-6 gap-y-2 text-sm sm:grid-cols-2">
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">客户</dt>
                        <dd className="text-right font-medium">
                            {proposal.customer_name}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">负责销售</dt>
                        <dd className="num text-right">
                            {proposal.sales_owner_user_id}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">选品册</dt>
                        <dd className="num text-right">
                            {proposal.booklet_id}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">选品形态</dt>
                        <dd className="text-right">
                            {SELECTION_FORM_LABEL[proposal.form]}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">提交方式</dt>
                        <dd className="text-right">
                            {SUBMIT_MODE_LABEL[proposal.submit_mode]}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">提交时间</dt>
                        <dd className="num text-right">
                            {proposal.submitted_at}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">提交来源</dt>
                        <dd className="text-right">公开链接</dd>
                    </div>
                </dl>
            </CardContent>
        </Card>

        <Card>
            <CardHeader>
                <CardTitle className="text-sm">陈列项行</CardTitle>
            </CardHeader>
            <CardContent className="p-0">
                <div className="overflow-x-auto">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>陈列项</TableHead>
                                {proposal.form === "PACKAGE" ? (
                                    <TableHead>档位</TableHead>
                                ) : null}
                                <TableHead>售价</TableHead>
                                {proposal.submit_mode === "BY_QUANTITY" ? (
                                    <>
                                        <TableHead>份数</TableHead>
                                        <TableHead className="text-right">
                                            行金额
                                        </TableHead>
                                    </>
                                ) : null}
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {proposal.display_lines.map((line) => (
                                <TableRow key={line.display_item_id}>
                                    <TableCell className="font-medium">
                                        {line.display_item_id}
                                    </TableCell>
                                    {proposal.form === "PACKAGE" ? (
                                        <TableCell>
                                            {line.tier_id ?? "—"}
                                        </TableCell>
                                    ) : null}
                                    <TableCell>
                                        <MoneyValue
                                            value={line.unit_price}
                                            taxBasis="gross"
                                        />
                                    </TableCell>
                                    {proposal.submit_mode === "BY_QUANTITY" ? (
                                        <>
                                            <TableCell className="num">
                                                {line.quantity ?? "—"}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <MoneyValue
                                                    value={
                                                        line.line_amount ??
                                                        "0.00"
                                                    }
                                                    taxBasis="gross"
                                                />
                                            </TableCell>
                                        </>
                                    ) : null}
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
            </CardContent>
        </Card>

        <Card>
            <CardHeader>
                <CardTitle className="text-sm">SKU 行</CardTitle>
            </CardHeader>
            <CardContent className="p-0">
                <div className="overflow-x-auto">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>商品</TableHead>
                                <TableHead>单价</TableHead>
                                {proposal.submit_mode === "BY_QUANTITY" ? (
                                    <>
                                        <TableHead>数量</TableHead>
                                        <TableHead className="text-right">
                                            行金额
                                        </TableHead>
                                    </>
                                ) : null}
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {proposal.sku_lines.map((line, index) => (
                                <TableRow
                                    key={`${line.display_item_id}-${line.name}-${index}`}
                                >
                                    <TableCell>
                                        <div className="flex flex-col">
                                            <span className="font-medium">
                                                {line.name}
                                            </span>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={line.unit_price}
                                            taxBasis="gross"
                                        />
                                    </TableCell>
                                    {proposal.submit_mode === "BY_QUANTITY" ? (
                                        <>
                                            <TableCell className="num">
                                                {line.quantity ?? "—"}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <MoneyValue
                                                    value={
                                                        line.line_amount ??
                                                        "0.00"
                                                    }
                                                    taxBasis="gross"
                                                />
                                            </TableCell>
                                        </>
                                    ) : null}
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
            </CardContent>
        </Card>

        <Card>
            <CardContent className="flex flex-col gap-1.5 p-4 text-sm">
                {proposal.submit_mode === "BY_QUANTITY" ? (
                    <div className="flex items-center justify-between">
                        <span className="text-muted-foreground">
                            方案合计（含税）
                        </span>
                        <MoneyValue
                            value={proposal.total_amount ?? "0.00"}
                            taxBasis="gross"
                        />
                    </div>
                ) : (
                    <p className="text-muted-foreground">
                        商城兑换方案不记录份数与成交合计，售价仅供阅读。
                    </p>
                )}
                <p className="text-xs text-muted-foreground">
                    陈列项行金额之和等于 SKU
                    行金额之和，明细保留来源关系，未在存储时合并。
                </p>
            </CardContent>
        </Card>
    </div>
)
