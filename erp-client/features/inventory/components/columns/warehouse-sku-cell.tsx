/** 库存流水与预留列表共用的「仓库 / SKU」两行单元格。 */
export function WarehouseSkuIdentity({
    warehouseName,
    skuCode,
    skuName,
}: {
    warehouseName: string
    skuCode: string
    skuName: string
}) {
    return (
        <div className="min-w-0 text-sm">
            <div className="truncate font-medium">{warehouseName}</div>
            <div className="truncate">
                <span className="num">{skuCode}</span>
                <span className="text-muted-foreground"> · </span>
                {skuName}
            </div>
        </div>
    )
}
