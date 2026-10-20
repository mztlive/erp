import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"

/** 仅展示抄送接口已经授权的采购提交行。 */
export function CopyPurchaseLines({
    lines,
}: {
    lines: ApprovalPurchaseLine[]
}) {
    if (!lines.length) return null
    return (
        <section className="space-y-3 rounded-lg border p-4">
            <h2 className="font-medium">本次采购提交明细</h2>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>序号</TableHead>
                        <TableHead>商品或费用</TableHead>
                        <TableHead>规格</TableHead>
                        <TableHead>数量</TableHead>
                        <TableHead>含税单价</TableHead>
                        <TableHead>含税金额</TableHead>
                        <TableHead>预计交付日期</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {lines.map((line) => (
                        <TableRow key={line.id}>
                            <TableCell>{line.line_no}</TableCell>
                            <TableCell>{line.title}</TableCell>
                            <TableCell>{line.specification || "—"}</TableCell>
                            <TableCell className="num">
                                {line.quantity ?? "—"} {line.unit}
                            </TableCell>
                            <TableCell className="num">
                                {line.unit_cost_gross ?? "—"}
                            </TableCell>
                            <TableCell className="num">
                                {line.gross_amount ?? "—"}
                            </TableCell>
                            <TableCell>
                                {line.expected_delivery_date || "—"}
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </section>
    )
}
