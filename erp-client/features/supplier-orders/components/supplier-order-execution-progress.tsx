"use client"

import { DetailWorkspaceSection } from "@/components/business"
import { useSupplierExecutionSetupQuery } from "../hooks/use-execution-setup"
import { SupplierOrderExecutionTree } from "./supplier-order-execution-tree"
import { SupplierOrderExecutionShipments } from "./supplier-order-execution-shipments"

/** 履约分区承接完整子单和运单；无执行明细时不渲染空分区，避免页面留白与套娃卡片。 */
export function SupplierOrderExecutionProgress({
    orderId,
}: {
    orderId: string
}) {
    const query = useSupplierExecutionSetupQuery(orderId)
    const setup = query.data
    if (!setup || query.isError) return null

    const hasAddress = Boolean(setup.supplier_address)
    const hasTree = Boolean(setup.order_tree)
    const hasShipments = Boolean(setup.shipments?.length)
    if (!hasAddress && !hasTree && !hasShipments) return null

    return (
        <>
            {setup.supplier_address ? (
                <DetailWorkspaceSection
                    title="已确认供应商区域"
                    width="form"
                    subtitle="仅适用于本次冻结地址与采购数量，变更后须重新核验"
                >
                    <p className="text-body-compact text-foreground">
                        {[
                            setup.supplier_address.province.name,
                            setup.supplier_address.city.name,
                            setup.supplier_address.county.name,
                            setup.supplier_address.town.name,
                        ].join(" / ")}
                    </p>
                </DetailWorkspaceSection>
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
