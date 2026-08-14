/** 计量单位的创建 / 修订 / 停用命令（单位代码创建后不可改）。 */

import { apiPost, apiPut } from "@/lib/api"
import type { UnitOfMeasureDto } from "@/features/master-data/api/contracts"
import { isoNow } from "@/features/master-data/api/presentation"
import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
    UnitOfMeasureFields,
} from "@/features/master-data/types"
import { mapMutationError, parseQuantityScale } from "./shared"

export async function createUnitOfMeasure(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as UnitOfMeasureFields
    const quantityScale = parseQuantityScale(fields.quantityScale)
    if (quantityScale === null) {
        return {
            outcome: "blocked",
            code: "UNIT_QUANTITY_SCALE_INVALID",
            message: "数量小数位必须是 0–6 的整数。",
        }
    }
    if (!fields.code.trim()) {
        return {
            outcome: "blocked",
            code: "UNIT_CODE_REQUIRED",
            message: "请填写单位代码。",
        }
    }
    if (!fields.symbol.trim()) {
        return {
            outcome: "blocked",
            code: "UNIT_SYMBOL_REQUIRED",
            message: "请填写单位符号。",
        }
    }
    try {
        const created = await apiPost<UnitOfMeasureDto>(
            "/admin/unit-of-measures",
            {
                unit_code: fields.code.trim(),
                name: input.name.trim(),
                symbol: fields.symbol.trim(),
                quantity_scale: quantityScale,
                status: "active",
            },
        )
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.unit_code,
            revisionId: created.id,
            revisionNo: created.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.unit_code}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

export async function updateUnitOfMeasureRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    try {
        const fields = input.fields as UnitOfMeasureFields
        const quantityScale = parseQuantityScale(fields.quantityScale)
        if (quantityScale === null) {
            return {
                outcome: "blocked",
                code: "UNIT_QUANTITY_SCALE_INVALID",
                message: "数量小数位必须是 0–6 的整数。",
            }
        }
        if (!fields.symbol.trim()) {
            return {
                outcome: "blocked",
                code: "UNIT_SYMBOL_REQUIRED",
                message: "请填写单位符号。",
            }
        }
        const updated = await apiPut<UnitOfMeasureDto>(
            `/admin/unit-of-measures/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                name: input.name.trim(),
                symbol: fields.symbol.trim(),
                quantity_scale: quantityScale,
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.unit_code,
            revisionId: updated.id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-REV-${updated.unit_code}-v${updated.version}`,
            nextActions: ["查看变更历史", "返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

export async function disableUnitOfMeasure(
    input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    try {
        const updated = await apiPut<UnitOfMeasureDto>(
            `/admin/unit-of-measures/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                status: "disabled",
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.unit_code,
            revisionId: updated.id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-DIS-${updated.unit_code}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}
