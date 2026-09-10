import { describe, expect, it } from "vitest"
import { buildSupplierProfileQualifications } from "../api/mutations/supplier"
import type { SupplierFields } from "../types"

describe("合同日期与无附件合同保存", () => {
    it("保留导入的合同编号与明确到期日，不补导入日也不要求假附件", () => {
        const fields = {
            company: "示例供应商",
            contractNo: "HT-2026-01",
            contractValidTo: "2025-12-31",
        } as SupplierFields
        const result = buildSupplierProfileQualifications(
            fields,
            "2026-09-10",
            [],
        )
        expect(result).toHaveLength(1)
        expect(result[0]).toMatchObject({
            qualification_type: "contract",
            certificate_no: "HT-2026-01",
            valid_from: null,
            valid_to: "2025-12-31",
            attachment_id: null,
        })
    })
    it("补齐日期后保留合同及适用能力，同一份合同不因有附件重复创建", () => {
        const fields = {
            company: "示例供应商",
            contractNo: "HT-1",
            contractFile: "合同.pdf",
            contractFileAssetIds: { "合同.pdf": "asset-1" },
            contractValidFrom: "2026-01-01",
            contractValidTo: "2027-01-01",
            qualificationCapabilityCodes: { "contract::HT-1": ["physical"] },
        } as SupplierFields
        const result = buildSupplierProfileQualifications(
            fields,
            "2026-09-10",
            ["physical", "virtual"],
        )
        expect(result).toHaveLength(1)
        expect(result[0]).toMatchObject({
            valid_from: "2026-01-01",
            valid_to: "2027-01-01",
            attachment_id: "asset-1",
            capability_codes: ["physical"],
        })
    })
    it("未填写合同信息时不创建空合同", () => {
        expect(
            buildSupplierProfileQualifications(
                { company: "示例供应商" },
                "2026-09-10",
                [],
            ),
        ).toEqual([])
    })
})
