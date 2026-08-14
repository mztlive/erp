import { describe, expect, it } from "vitest"

import { makeItem } from "../hooks/test-fixtures"
import {
    formalStatus,
    isPanelErrorClass,
    mapPanelStatus,
    newKey,
} from "./helpers"

describe("mapPanelStatus", () => {
    it("maps AUTO_RETRYING to auto-retrying", () => {
        expect(
            mapPanelStatus(
                makeItem({
                    status: { code: "AUTO_RETRYING", label: "自动重试中" },
                }),
            ),
        ).toBe("auto-retrying")
    })

    it("maps manual labels to manual-required", () => {
        expect(
            mapPanelStatus(
                makeItem({
                    status: { code: "X", label: "需人工核查" },
                }),
            ),
        ).toBe("manual-required")
    })

    it("maps MANUAL_REQUIRED code to manual-required", () => {
        expect(
            mapPanelStatus(
                makeItem({
                    status: { code: "MANUAL_REQUIRED", label: "待处理" },
                }),
            ),
        ).toBe("manual-required")
    })

    it("maps COMPLETED / 已解决 to resolved", () => {
        expect(
            mapPanelStatus(
                makeItem({
                    status: { code: "COMPLETED", label: "完成" },
                }),
            ),
        ).toBe("resolved")
        expect(
            mapPanelStatus(
                makeItem({
                    status: { code: "X", label: "已解决" },
                }),
            ),
        ).toBe("resolved")
    })

    it("maps CLOSED / 关闭 to closed", () => {
        expect(
            mapPanelStatus(
                makeItem({ status: { code: "CLOSED", label: "关闭" } }),
            ),
        ).toBe("closed")
        expect(
            mapPanelStatus(
                makeItem({
                    status: { code: "X", label: "手动关闭" },
                }),
            ),
        ).toBe("closed")
    })

    it("defaults to pending", () => {
        expect(
            mapPanelStatus(
                makeItem({ status: { code: "OPEN", label: "待处理" } }),
            ),
        ).toBe("pending")
    })
})

describe("isPanelErrorClass", () => {
    it("accepts interface error classes", () => {
        expect(isPanelErrorClass("parameter-or-mapping")).toBe(true)
        expect(isPanelErrorClass("business-rejected")).toBe(true)
    })

    it("rejects reconciliation-difference", () => {
        expect(isPanelErrorClass("reconciliation-difference")).toBe(false)
    })
})

describe("formalStatus", () => {
    it("maps failed to rejected", () => {
        expect(formalStatus("failed")).toBe("rejected")
    })

    it("passes through other statuses", () => {
        expect(formalStatus("succeeded")).toBe("succeeded")
        expect(formalStatus("blocked")).toBe("blocked")
        expect(formalStatus("rejected")).toBe("rejected")
        expect(formalStatus("unknown")).toBe("unknown")
    })
})

describe("newKey", () => {
    it("prefixes with the given tag and produces unique values", () => {
        const a = newKey("w29:t")
        const b = newKey("w29:t")
        expect(a.startsWith("w29:t:")).toBe(true)
        expect(b.startsWith("w29:t:")).toBe(true)
        expect(a).not.toBe(b)
    })
})
