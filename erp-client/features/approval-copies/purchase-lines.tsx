import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
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
    const { names } = useProductUnitNames([], lines.length > 0)
    if (!lines.length) return null
    return (
        <DetailWorkspaceSection title="本次采购提交明细">
            <div
                className={detailWorkspaceStyles.inlineTableWrapper}
                data-slot="detail-inline-table"
            >
                <Table className="[&_:is(th,td):first-child]:ps-0">
                    <TableHeader>
                        <TableRow>
                            <TableHead>序号</TableHead>
                            <TableHead>商品或费用</TableHead>
                            <TableHead>规格</TableHead>
                            <TableHead data-align="end">数量</TableHead>
                            <TableHead data-align="end">含税单价</TableHead>
                            <TableHead data-align="end">含税金额</TableHead>
                            <TableHead>预计交付日期</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {lines.map((line) => (
                            <TableRow key={line.id}>
                                <TableCell className="num">
                                    {line.line_no}
                                </TableCell>
                                <TableCell>{line.title}</TableCell>
                                <TableCell>
                                    {line.specification || "—"}
                                </TableCell>
                                <TableCell className="num" data-align="end">
                                    {line.quantity ?? "—"}{" "}
                                    {line.unitName ??
                                        names.get(line.unit?.trim() ?? "")}
                                </TableCell>
                                <TableCell className="num" data-align="end">
                                    {line.unit_cost_gross ?? "—"}
                                </TableCell>
                                <TableCell className="num" data-align="end">
                                    {line.gross_amount ?? "—"}
                                </TableCell>
                                <TableCell>
                                    {line.expected_delivery_date || "—"}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
        </DetailWorkspaceSection>
    )
}
