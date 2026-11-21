import { TableEmptyCell, TableIdentityCell } from "@/components/business"

export function WarehouseSkuIdentity({
    warehouseName,
    warehouseCode,
    skuCode,
    skuName,
    specSummary,
}: {
    warehouseName: string
    warehouseCode?: string
    skuCode: string
    skuName: string
    specSummary?: string
}) {
    const primary = [warehouseName, warehouseCode].filter(Boolean).join(" · ")
    const secondary = [skuCode, skuName !== skuCode ? skuName : "", specSummary]
        .filter(Boolean)
        .join(" · ")
    return (
        <TableIdentityCell
            primary={primary || <TableEmptyCell />}
            primaryTitle={primary}
            secondary={secondary}
            secondaryTitle={secondary}
            className={warehouseCode ? "num" : undefined}
            numericSecondary
        />
    )
}
