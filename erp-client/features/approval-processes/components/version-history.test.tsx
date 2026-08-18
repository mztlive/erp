import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { versionsFixture } from "../fixtures"
import { VersionHistory } from "./version-history"

describe("version history", () => {
    it("renders versions as read-only history without edit actions", () => {
        render(
            <VersionHistory
                versions={versionsFixture()}
                selectedVersion="1"
                onSelect={vi.fn()}
            />,
        )
        expect(screen.getByText("已发布")).toBeTruthy()
        expect(screen.getByText("草稿")).toBeTruthy()
        expect(screen.getAllByText("查看")).toHaveLength(2)
        expect(screen.queryByText("保存草稿")).toBeNull()
        expect(screen.queryByText("发布")).toBeNull()
        expect(screen.queryByText("退役")).toBeNull()
    })

    it("shows empty history copy", () => {
        render(<VersionHistory versions={[]} onSelect={vi.fn()} />)
        expect(
            screen.getByTestId("version-history-empty").textContent,
        ).toContain("还没有历史版本")
    })
})
