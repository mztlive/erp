/** 计量单位对象中心：全量拉取后按稳定 ID 匹配。 */

import type { UnitOfMeasureDto } from "@/features/master-data/api/contracts"
import { mapUnitOfMeasureRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerUnitOfMeasure(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<UnitOfMeasureDto>(
        "/admin/unit-of-measures",
        {},
    )
    const dto = items.find((u) => u.id === stableId)
    if (!dto) return null
    const row = mapUnitOfMeasureRow(dto)
    return baseCenter("unit-of-measures", row)
}
