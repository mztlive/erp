"use client"

import * as React from "react"

import { buildAdjustmentColumns } from "@/features/inventory/components/columns/adjustment-columns"
import {
    buildBalanceColumns,
    type BalanceColumnsInput,
} from "@/features/inventory/components/columns/balance-columns"
import { buildMovementColumns } from "@/features/inventory/components/columns/movement-columns"
import { buildReservationColumns } from "@/features/inventory/components/columns/reservation-columns"

export type InventoryColumnsInput = BalanceColumnsInput

function useInventoryColumns({
    isPhoneNarrow,
    rowFocusRef,
    openDetail,
    startAdjustment,
    isCreating,
}: InventoryColumnsInput) {
    const balanceColumns = React.useMemo(
        () =>
            buildBalanceColumns({
                isPhoneNarrow,
                rowFocusRef,
                openDetail,
                startAdjustment,
                isCreating,
            }),
        [openDetail, startAdjustment, isCreating, isPhoneNarrow, rowFocusRef],
    )

    const movementColumns = React.useMemo(
        () => buildMovementColumns(),
        [],
    )

    const reservationColumns = React.useMemo(
        () => buildReservationColumns(),
        [],
    )

    const adjustmentColumns = React.useMemo(
        () => buildAdjustmentColumns(),
        [],
    )

    return {
        adjustmentColumns,
        balanceColumns,
        movementColumns,
        reservationColumns,
    }
}

export { useInventoryColumns }
