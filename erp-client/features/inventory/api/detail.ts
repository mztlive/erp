/**
 * W10 库存台账 · 余额详情 HTTP 入口。
 */

import { apiGet } from "@/lib/api"
import type { BalanceDetailView } from "@/features/inventory/types"
import { isApiError } from "@/features/inventory/api/display"
import {
    mapAdjustment,
    mapBalance,
    mapMovement,
    mapReservation,
} from "@/features/inventory/api/mappers"
import type { BackendStockBalanceDetail } from "@/features/inventory/api/dto"

export async function fetchBalanceDetail(
    balanceId: string,
): Promise<BalanceDetailView | null> {
    try {
        const detail = await apiGet<BackendStockBalanceDetail>(
            `/admin/stock-balances/${encodeURIComponent(balanceId)}`,
        )
        const balance = mapBalance(detail.balance)
        const recentMovements = detail.recent_movements.map((m) =>
            mapMovement(m, {
                warehouseName: balance.warehouseName,
                skuCode: balance.skuCode,
                skuName: balance.skuName,
            }),
        )
        const reservations = detail.active_reservations.map((r) => {
            const row = mapReservation(r)
            return {
                ...row,
                warehouseName: balance.warehouseName,
                skuCode: balance.skuCode,
                skuName: balance.skuName,
                balanceId: balance.balanceId,
            }
        })
        const sourceMap = new Map<
            string,
            BalanceDetailView["sourceDocuments"][number]
        >()
        for (const m of recentMovements) {
            const key = `${m.sourceDocumentType}:${m.sourceDocumentId}`
            if (sourceMap.has(key)) continue
            sourceMap.set(key, {
                documentType: m.sourceDocumentType,
                documentId: m.sourceDocumentId,
                documentNo: m.sourceDocumentNo,
                label: m.movementTypeLabel,
                href: m.sourceHref,
                workspaceId:
                    m.sourceDocumentType === "PURCHASE_RECEIPT" ||
                    m.sourceDocumentType === "WAREHOUSE_DISPATCH"
                        ? "W09"
                        : undefined,
            })
        }
        const pendingAdjustments = detail.pending_adjustments.map((a) =>
            mapAdjustment(a, {
                id: "",
                sku_id: balance.skuId,
                quantity: "",
                direction: "INCREASE",
            }),
        )

        return {
            balance,
            recentMovements,
            reservations,
            sourceDocuments: [...sourceMap.values()],
            pendingAdjustments,
            queriedAt: new Date().toISOString(),
        }
    } catch (error) {
        if (
            isApiError(error) &&
            (error.status === 404 || error.status === 403)
        ) {
            return null
        }
        throw error
    }
}
