"use client"

import { useSupplierExecutionSetupQuery } from "../hooks/use-execution-setup"
import { SupplierOrderExecutionTree } from "./supplier-order-execution-tree"
import { SupplierOrderExecutionShipments } from "./supplier-order-execution-shipments"

/** 履约分区承接完整子单和运单，页头仅保留当前异常与办理入口。 */
export function SupplierOrderExecutionProgress({
    orderId,
}: {
    orderId: string
}) {
    const query = useSupplierExecutionSetupQuery(orderId)
    const setup = query.data
    if (!setup || query.isError) return null
    return (
        <>
            {setup.supplier_address ? (
                <div className="space-y-1 rounded-lg border p-3 text-sm">
                    <p className="font-medium">已确认供应商区域</p>
                    <p className="text-muted-foreground">
                        {[
                            setup.supplier_address.province.name,
                            setup.supplier_address.city.name,
                            setup.supplier_address.county.name,
                            setup.supplier_address.town.name,
                        ].join(" / ")}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        仅适用于本次冻结地址与采购数量，变更后须重新核验。
                    </p>
                </div>
            ) : null}
            {setup.order_tree ? (
                <SupplierOrderExecutionTree
                    tree={setup.order_tree}
                    orderId={orderId}
                    confirmationUnknown={setup.stage === "CONFIRMATION_UNKNOWN"}
                />
            ) : null}
            {setup.shipments?.length ? (
                <SupplierOrderExecutionShipments shipments={setup.shipments} />
            ) : null}
        </>
    )
}
