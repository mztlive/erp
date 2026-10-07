"use client"

import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { ContractBindingCheck } from "@/features/sales-orders/api/sales-order-contract"
import { getErrorMessage } from "@/lib/api/errors"

/** 原单与合同逐项对照；失败和核对中均由父表单禁止提交。 */
export function ContractSupplementCheck({
    data,
    loading,
    error,
}: {
    data?: ContractBindingCheck
    loading: boolean
    error: unknown
}) {
    if (error)
        return (
            <p role="alert" className="text-sm text-destructive">
                {getErrorMessage(
                    error,
                    "合同核对失败，请重新选择合同或刷新页面后重试",
                )}
            </p>
        )
    if (loading || !data)
        return (
            <p role="status" className="text-sm text-muted-foreground">
                正在核对原单与合同条款…
            </p>
        )
    return (
        <div
            id="sales-order-contract-supplement-check"
            className="space-y-2"
            aria-live="polite"
        >
            <p className="text-sm text-muted-foreground">
                核对依据：{data.basis}
            </p>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>核对项目</TableHead>
                        <TableHead>原销售单</TableHead>
                        <TableHead>所选合同</TableHead>
                        <TableHead>结果</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {data.items.map((item) => (
                        <TableRow key={item.field}>
                            <TableCell>{item.label}</TableCell>
                            <TableCell className="whitespace-normal">
                                {item.sales_value || "未填写"}
                            </TableCell>
                            <TableCell className="whitespace-normal">
                                {item.contract_value || "未填写"}
                            </TableCell>
                            <TableCell
                                className={
                                    item.matches
                                        ? "text-muted-foreground"
                                        : "text-destructive"
                                }
                            >
                                {item.matches ? "一致" : "不一致"}
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
            <p
                className={
                    data.matches
                        ? "text-sm text-muted-foreground"
                        : "text-sm text-destructive"
                }
            >
                {data.matches
                    ? "条款一致，可以补录。原单内容和审批记录保持不变。"
                    : "条款不一致，不能补录。请先修正销售草稿；已生效的销售单须完成销售变更后，再重新核对。"}
            </p>
        </div>
    )
}
